use std::io::Write;
use std::time::Duration;

use abhiverse_agent_windows::{load_identity, load_pairing, save_pairing, PairingRecord};
use abhiverse_identity::default_data_dir;
use abhiverse_protocol::{
    now_unix, pairing_approve_message, pairing_reject_message, PairingStatus,
};

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
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
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            let ts = now_unix();
            let signature =
                thread_identity.sign(&abhiverse_protocol::heartbeat_message(&device_id, ts));
            let url = format!("{api_url}/devices/{device_id}/heartbeat");
            match ureq::post(&url)
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

    let created: abhiverse_protocol::CreatePairingResponse =
        ureq::post(&format!("{api_url}/pairing/requests"))
            .send_json(serde_json::json!({ "agent": identity.info }))?
            .into_json()?;
    println!();
    println!("  Pairing code: {}", created.code);
    println!("  Enter this code in the Abhiverse app within 5 minutes.");
    println!();

    let mut decided = false;
    loop {
        if now_unix() > created.expires_at {
            println!("Pairing code expired. Run --pair again for a new code.");
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
        let view: abhiverse_protocol::PairingView =
            ureq::get(&format!("{api_url}/pairing/{}", created.pairing_id))
                .call()?
                .into_json()?;
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
                ureq::post(&url).send_json(serde_json::json!({ "signature": sig }))?;
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
    match ureq::request("DELETE", &url)
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
