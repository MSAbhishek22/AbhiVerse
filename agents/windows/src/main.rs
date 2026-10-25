use std::io::Write;
use std::time::Duration;

use abhiverse_agent_windows::{load_identity, load_pairing, save_pairing, PairingRecord};

mod remote;
use abhiverse_identity::default_data_dir;
use abhiverse_protocol::{
    now_unix, pairing_approve_message, pairing_reject_message, PairingStatus,
};

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

/// HTTP client for control-plane calls: a 10 s timeout, so a hung
/// connection can never stall the heartbeat/pairing loops forever
/// (previously bare `ureq` calls had no timeout at all).
fn http_client() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
}

/// HTTP client for chunk transfers: a 256 KiB chunk may legitimately
/// take a while on a slow link, but must still be bounded.
fn http_transfer_client() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(120))
        .build()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("abhiverse-agent {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // --pair --api <url>: run the (blocking) pairing flow and exit.
    if args.iter().any(|a| a == "--pair") {
        let api_url =
            arg_value(&args, "--api").unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
        return run_pairing(&api_url);
    }

    // --send-file <path>: upload a file to the paired controller.
    if let Some(path) = arg_value(&args, "--send-file") {
        let identity = load_identity()?;
        return run_send_file(&identity, &path);
    }

    // --remote: answer remote desktop sessions (with local approval).
    // --allow-input additionally enables injecting the controller's
    // input into this desktop (006c); without it, input is only logged.
    if args.iter().any(|a| a == "--remote") {
        let allow_input = args.iter().any(|a| a == "--allow-input");
        let identity = load_identity()?;
        return remote::run(identity, allow_input).map_err(|e| -> Box<dyn std::error::Error> { e });
    }

    // --receive: download files addressed to this device.
    if args.iter().any(|a| a == "--receive") {
        let identity = load_identity()?;
        return run_receive(&identity);
    }

    let identity = load_identity()?;
    if identity.created {
        println!("Created a new device identity for this computer.");
    }

    // --once / --identity: print machine-readable identity and exit.
    if args.iter().any(|a| a == "--once" || a == "--identity") {
        println!("{}", serde_json::to_string(&identity.info)?);
        println!("device_id: {}", identity.info.device_id);
        return Ok(());
    }

    // --revoke: self-revoke this device from its paired API.
    if args.iter().any(|a| a == "--revoke") {
        return run_revoke(&identity);
    }

    println!(
        "Abhiverse agent {} running as '{}' (device {})",
        env!("CARGO_PKG_VERSION"),
        identity.info.name,
        identity.info.device_id
    );

    // Heartbeat thread when this device is paired.
    let data_dir = default_data_dir();
    if let Some(record) = load_pairing(&data_dir) {
        println!(
            "Paired with controller '{}' — sending presence heartbeats.",
            record.controller_name
        );
        let device_id = identity.info.device_id.clone();
        let api_url = record.api_url.clone();
        let shared = std::sync::Arc::new(identity);
        let thread_identity = shared.clone();
        let http = http_client();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            let ts = now_unix();
            let signature =
                thread_identity.sign(&abhiverse_protocol::heartbeat_message(&device_id, ts));
            let url = format!("{api_url}/devices/{device_id}/heartbeat");
            match http
                .post(&url)
                .send_json(serde_json::json!({ "timestamp": ts, "signature": signature }))
            {
                Ok(_) => {}
                Err(ureq::Error::Status(403, _)) | Err(ureq::Error::Status(404, _)) => {
                    eprintln!("This device has been revoked or removed; heartbeats stopped.");
                    return;
                }
                Err(_) => {} // transient network error: keep trying
            }
        });
    } else {
        println!("Not paired yet. Pair with: abhiverse-agent --pair --api <api-url>");
    }

    // Wait for Ctrl+C and shut down cleanly.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let _ = tokio::signal::ctrl_c().await;
    });
    println!("Shutdown signal received, exiting cleanly.");
    Ok(())
}

fn run_pairing(api_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let identity = load_identity()?;
    println!(
        "Pairing '{}' ({}) with {api_url}",
        identity.info.name, identity.info.device_id
    );

    let created: abhiverse_protocol::CreatePairingResponse = http_client()
        .post(&format!("{api_url}/pairing/requests"))
        .send_json(serde_json::json!({ "agent": identity.info }))?
        .into_json()?;
    println!();
    println!("  Pairing code: {}", created.code);
    println!("  Enter this code in the Abhiverse app within 5 minutes,");
    println!("  or scan this QR code with the app:");
    println!();
    match qrcode::QrCode::new(format!("abhiverse://pair?code={}", created.code)) {
        Ok(qr) => {
            let rendered = qr
                .render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build();
            for line in rendered.lines() {
                println!("  {line}");
            }
        }
        Err(e) => println!("  (could not render QR code: {e})"),
    }
    println!();

    let mut decided = false;
    loop {
        if now_unix() > created.expires_at {
            println!("Pairing code expired. Run --pair again for a new code.");
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
        let view: abhiverse_protocol::PairingView = match http_client()
            .get(&format!("{api_url}/pairing/{}", created.pairing_id))
            .call()
        {
            Ok(resp) => resp.into_json()?,
            Err(ureq::Error::Status(404, _)) => {
                // Pruned after expiry (see API retention): same
                // outcome as an expired code.
                println!("Pairing code expired. Run --pair again for a new code.");
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        match view.status {
            PairingStatus::Claimed if !decided => {
                decided = true;
                let who = view
                    .controller_name
                    .clone()
                    .unwrap_or_else(|| "A controller".into());
                print!("{who} wants to pair with this computer. Approve? [y/N] ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                let approve = line.trim().eq_ignore_ascii_case("y");
                let (url, sig) = if approve {
                    (
                        format!("{api_url}/pairing/{}/approve", created.pairing_id),
                        identity.sign(&pairing_approve_message(&created.pairing_id, &created.code)),
                    )
                } else {
                    (
                        format!("{api_url}/pairing/{}/reject", created.pairing_id),
                        identity.sign(&pairing_reject_message(&created.pairing_id, &created.code)),
                    )
                };
                http_client()
                    .post(&url)
                    .send_json(serde_json::json!({ "signature": sig }))?;
                if !approve {
                    println!("Pairing rejected.");
                    return Ok(());
                }
            }
            PairingStatus::Approved => {
                let record = PairingRecord {
                    api_url: api_url.to_string(),
                    pairing_id: created.pairing_id.clone(),
                    controller_name: view.controller_name.clone().unwrap_or_default(),
                    controller_id: view.controller_id.clone().unwrap_or_default(),
                };
                save_pairing(&default_data_dir(), &record)?;
                println!("Paired successfully with '{}'.", record.controller_name);
                println!("Run abhiverse-agent normally to stay reachable (heartbeats).");
                return Ok(());
            }
            PairingStatus::Rejected => {
                println!("Pairing was rejected.");
                return Ok(());
            }
            PairingStatus::Expired => {
                println!("Pairing code expired. Run --pair again for a new code.");
                return Ok(());
            }
            _ => {} // Pending / already decided: keep waiting
        }
    }
}

/// Unique per-request ID covered by create signatures (v0.5.2):
/// distinguishes a legitimate duplicate create from a replay.
fn new_request_id(seed: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    abhiverse_file_transfer::sha256_hex(format!("{seed}:{}:{nanos}", std::process::id()).as_bytes())
        [..32]
        .to_string()
}

fn run_send_file(
    identity: &abhiverse_identity::DeviceIdentity,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Seek, SeekFrom};
    let data_dir = default_data_dir();
    let Some(record) = load_pairing(&data_dir) else {
        return Err("this device is not paired — run --pair first".into());
    };
    if record.controller_id.is_empty() {
        return Err("pairing record has no controller_id — re-run --pair to refresh it".into());
    }
    let api = &record.api_url;
    let file_path = std::path::Path::new(path);
    let size = std::fs::metadata(file_path)?.len();
    let sha = abhiverse_file_transfer::sha256_file(file_path)?;
    let total = abhiverse_file_transfer::total_chunks(size);
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("file path has no usable name")?
        .to_string();

    let request_id = new_request_id(&identity.info.device_id);
    let ts = abhiverse_protocol::now_unix();
    let message = abhiverse_protocol::transfer_create_message(
        &identity.info.device_id,
        &record.controller_id,
        &sha,
        ts,
        &request_id,
    );
    let view: abhiverse_protocol::TransferView = http_client()
        .post(&format!("{api}/transfers"))
        .set("x-abhiverse-timestamp", &ts.to_string())
        .set("x-abhiverse-signature", &identity.sign(&message))
        .send_json(serde_json::json!({
            "from_id": identity.info.device_id,
            "to_id": record.controller_id,
            "file_name": file_name,
            "size_bytes": size,
            "sha256": sha,
            "total_chunks": total,
            "request_id": request_id,
        }))?
        .into_json()?;
    println!(
        "Transfer {} created ({size} bytes, {total} chunks, sha256 {sha})",
        view.transfer_id
    );

    let mut file = std::fs::File::open(file_path)?;
    for index in 0..total {
        let len = abhiverse_file_transfer::chunk_len(size, index) as usize;
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(
            index as u64 * abhiverse_file_transfer::CHUNK_SIZE,
        ))?;
        file.read_exact(&mut buf)?;
        let ts = abhiverse_protocol::now_unix();
        let chunk_hash = abhiverse_file_transfer::sha256_hex(&buf);
        let message =
            abhiverse_protocol::chunk_upload_message(&view.transfer_id, index, &chunk_hash, ts);
        http_transfer_client()
            .put(&format!(
                "{api}/transfers/{}/chunks/{index}",
                view.transfer_id
            ))
            .set("x-abhiverse-timestamp", &ts.to_string())
            .set("x-abhiverse-signature", &identity.sign(&message))
            .send_bytes(&buf)?;
        println!("  chunk {}/{} uploaded", index + 1, total);
    }
    if total == 0 {
        println!("  (empty file — no chunks to upload)");
    }

    let ts = abhiverse_protocol::now_unix();
    let message = abhiverse_protocol::transfer_complete_message(&view.transfer_id, ts);
    let done: abhiverse_protocol::TransferView = http_client()
        .post(&format!("{api}/transfers/{}/complete", view.transfer_id))
        .set("x-abhiverse-timestamp", &ts.to_string())
        .set("x-abhiverse-signature", &identity.sign(&message))
        .send_json(serde_json::json!({}))?
        .into_json()?;
    println!(
        "Sent '{}' — server verified the file hash (status: {:?}).",
        file_name, done.status
    );
    println!(
        "Your controller can now download it (transfer {}).",
        view.transfer_id
    );
    Ok(())
}

fn run_receive(
    identity: &abhiverse_identity::DeviceIdentity,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read as _;
    let data_dir = default_data_dir();
    let Some(record) = load_pairing(&data_dir) else {
        return Err("this device is not paired — run --pair first".into());
    };
    let api = &record.api_url;
    let device_id = &identity.info.device_id;
    let ts = abhiverse_protocol::now_unix();
    let list_path = format!("/transfers?for={device_id}&as={device_id}");
    let list_message = abhiverse_protocol::signed_get_message(&list_path, device_id, ts);
    let list: abhiverse_protocol::TransferListResponse = http_client()
        .get(&format!("{api}{list_path}"))
        .set("x-abhiverse-timestamp", &ts.to_string())
        .set("x-abhiverse-signature", &identity.sign(&list_message))
        .call()?
        .into_json()?;
    let receive_dir = data_dir.join("received");
    std::fs::create_dir_all(&receive_dir)?;
    let mut handled = 0;
    for view in &list.transfers {
        if view.status != abhiverse_protocol::TransferStatus::Complete {
            println!(
                "Transfer {} ('{}') is {:?} — waiting for the sender to finish.",
                view.transfer_id, view.file_name, view.status
            );
            continue;
        }
        let already = receive_dir.join(&view.file_name);
        if already.exists()
            && abhiverse_file_transfer::sha256_file(&already)
                .ok()
                .as_deref()
                == Some(view.sha256.as_str())
        {
            println!(
                "'{}' already received and verified — skipping.",
                view.file_name
            );
            handled += 1;
            continue;
        }
        let progress_path = receive_dir.join(format!("{}.json", view.transfer_id));
        let mut progress = abhiverse_file_transfer::ReceiveProgress::load(&progress_path)
            .unwrap_or_else(|| {
                abhiverse_file_transfer::ReceiveProgress::new(
                    &view.transfer_id,
                    &view.file_name,
                    view.size_bytes,
                    &view.sha256,
                )
            });
        for index in progress.missing() {
            let ts = abhiverse_protocol::now_unix();
            let message = abhiverse_protocol::chunk_download_message(&view.transfer_id, index, ts);
            let mut bytes = Vec::new();
            http_transfer_client()
                .get(&format!(
                    "{api}/transfers/{}/chunks/{index}",
                    view.transfer_id
                ))
                .set("x-abhiverse-timestamp", &ts.to_string())
                .set("x-abhiverse-signature", &identity.sign(&message))
                .call()?
                .into_reader()
                .read_to_end(&mut bytes)?;
            abhiverse_file_transfer::write_chunk(&receive_dir, &view.transfer_id, index, &bytes)?;
            progress.received.push(index);
            progress.save(&progress_path)?;
            println!(
                "  '{}' chunk {}/{} downloaded",
                view.file_name,
                progress.received.len(),
                view.total_chunks.max(1)
            );
        }
        if view.total_chunks == 0 {
            let name = abhiverse_file_transfer::sanitize_file_name(&view.file_name)
                .ok_or("unsafe file name from server")?;
            std::fs::write(receive_dir.join(name), b"")?;
            println!("Received '{}' (empty file)", view.file_name);
        } else {
            let out = abhiverse_file_transfer::finish_download(&receive_dir, &progress)?;
            println!(
                "Received '{}' -> {} (SHA-256 verified locally)",
                view.file_name,
                out.display()
            );
        }
        let _ = std::fs::remove_file(&progress_path);
        handled += 1;
    }
    if handled == 0 && list.transfers.is_empty() {
        println!("No transfers addressed to this device.");
    }
    Ok(())
}

fn run_revoke(
    identity: &abhiverse_identity::DeviceIdentity,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir();
    let Some(record) = load_pairing(&data_dir) else {
        println!("This device is not paired, nothing to revoke.");
        return Ok(());
    };
    let device_id = &identity.info.device_id;
    let ts = now_unix();
    let signature = identity.sign(&abhiverse_protocol::revoke_message(device_id, ts));
    let url = format!("{}/devices/{}", record.api_url, device_id);
    match http_client()
        .request("DELETE", &url)
        .send_json(serde_json::json!({ "timestamp": ts, "signature": signature }))
    {
        Ok(_) => {
            let _ = std::fs::remove_file(abhiverse_agent_windows::pairing_path(&data_dir));
            println!("Device revoked and pairing removed.");
            Ok(())
        }
        Err(e) => Err(format!("revoke failed: {e}").into()),
    }
}
