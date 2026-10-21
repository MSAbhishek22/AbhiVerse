//! Headless controller probe for Task 006a end-to-end verification.
//!
//! Acts as the controller peer: creates a session through the
//! coordination API signalling, waits for the agent's signed answer,
//! connects over WebRTC, opens the `abhiverse-input` data channel
//! and sends two input events. Exits 0 on success.
//!
//! The probe owns a real controller identity (Task 007): pass
//! `--controller-dir <dir>` for its key storage and, for a first
//! run, `--pair-code <code>` so it can claim the pairing itself.
//!
//! Usage: cargo run --example rtc_probe -- --api http://127.0.0.1:8080 \
//!          --controller-dir <dir> --device-id <agent-device-id> [--pair-code <code>]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use abhiverse_protocol::{
    CreateSessionRequest, EndSessionRequest, IceCandidateListResponse, InputEvent, RtcConfig,
    SessionStatus, SessionView, INPUT_CHANNEL_LABEL,
};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let api = arg(&args, "--api").unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let controller_dir = arg(&args, "--controller-dir").expect("--controller-dir required");
    let device_id = arg(&args, "--device-id").expect("--device-id required");
    let client = reqwest::Client::new();

    // This probe IS a controller: it holds an Ed25519 identity and
    // signs its session requests, exactly like the phone app.
    let identity = abhiverse_identity::ensure_identity(
        std::path::Path::new(&controller_dir),
        "Probe Controller",
        abhiverse_protocol::DeviceKind::Controller,
    )?;
    let controller_id = identity.info.device_id.clone();
    if let Some(code) = arg(&args, "--pair-code") {
        let resp = client
            .post(format!("{api}/pairing/claim"))
            .json(&serde_json::json!({ "code": code, "controller": identity.info }))
            .send()
            .await?;
        println!("probe: pairing claim -> {}", resp.status());
        resp.error_for_status()?;
    }

    let rtc: RtcConfig = client
        .get(format!("{api}/config/rtc"))
        .send()
        .await?
        .json()
        .await?;
    let config = RTCConfiguration {
        ice_servers: rtc
            .ice_servers
            .iter()
            .map(|s| RTCIceServer {
                urls: s.urls.clone(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut m)?;
    let webrtc_api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();
    let pc = Arc::new(webrtc_api.new_peer_connection(config).await?);

    let sent = Arc::new(AtomicBool::new(false));
    let dc = pc.create_data_channel(INPUT_CHANNEL_LABEL, None).await?;
    {
        let sent = sent.clone();
        let dc2 = dc.clone();
        dc.on_open(Box::new(move || {
            let sent = sent.clone();
            let dc2 = dc2.clone();
            Box::pin(async move {
                println!("probe: data channel open");
                let move_event =
                    serde_json::to_string(&InputEvent::MouseMove { x: 0.5, y: 0.25 }).unwrap();
                let key_event = serde_json::to_string(&InputEvent::KeyDown {
                    code: "KeyA".to_string(),
                })
                .unwrap();
                dc2.send_text(move_event).await.expect("send mouse event");
                dc2.send_text(key_event).await.expect("send key event");
                sent.store(true, Ordering::Relaxed);
                println!("probe: input events sent");
            })
        }));
    }

    pc.on_ice_connection_state_change(Box::new(move |state| {
        Box::pin(async move {
            println!("probe: ICE connection state: {state}");
        })
    }));
    pc.on_peer_connection_state_change(Box::new(move |state| {
        Box::pin(async move {
            println!("probe: peer connection state: {state}");
        })
    }));

    // Offer, with ICE candidates gathered into the SDP (the agent
    // side trickles its candidates through the API instead).
    let offer = pc.create_offer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(offer).await?;
    let _ = gather_complete.recv().await;
    let local = pc.local_description().await.expect("local description");

    let offer_hash = abhiverse_file_transfer::sha256_hex(local.sdp.as_bytes());
    let request_id = abhiverse_file_transfer::sha256_hex(
        format!("probe:{controller_id}:{}", std::process::id()).as_bytes(),
    )[..32]
        .to_string();
    let ts = abhiverse_protocol::now_unix();
    let message = abhiverse_protocol::session_create_message(
        &device_id,
        &controller_id,
        &offer_hash,
        ts,
        &request_id,
    );
    let view: SessionView = client
        .post(format!("{api}/sessions"))
        .header("x-abhiverse-timestamp", ts.to_string())
        .header("x-abhiverse-signature", identity.sign(&message))
        .json(&CreateSessionRequest {
            device_id: device_id.clone(),
            controller_id: controller_id.clone(),
            sdp_offer: local.sdp.clone(),
            request_id: request_id.clone(),
        })
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let sid = view.session_id.clone();
    println!("probe: session {sid} created, waiting for the agent to answer…");

    // Wait for the signed answer.
    let mut answered = false;
    for _ in 0..180 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let view: SessionView = client
            .get(format!("{api}/sessions/{sid}?as={controller_id}"))
            .send()
            .await?
            .json()
            .await?;
        match view.status {
            SessionStatus::Answered => {
                let answer = view.sdp_answer.clone().expect("answer SDP");
                pc.set_remote_description(RTCSessionDescription::answer(answer)?)
                    .await?;
                println!("probe: answer applied");
                answered = true;
                break;
            }
            SessionStatus::Rejected => return Err("agent rejected the session".into()),
            SessionStatus::Expired => return Err("session expired before answer".into()),
            _ => {}
        }
    }
    if !answered {
        return Err("timed out waiting for the agent's answer".into());
    }

    // Consume the agent's trickled ICE candidates until events sent.
    let mut after: u64 = 0;
    for _ in 0..120 {
        if sent.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let list: IceCandidateListResponse = client
            .get(format!(
                "{api}/sessions/{sid}/ice?as={controller_id}&after={after}"
            ))
            .send()
            .await?
            .json()
            .await?;
        for cand in list.candidates {
            after = after.max(cand.seq);
            let init = RTCIceCandidateInit {
                candidate: cand.candidate,
                sdp_mid: cand.sdp_mid,
                sdp_mline_index: cand.sdp_mline_index.map(|v| v as u16),
                username_fragment: None,
            };
            println!("probe: adding remote candidate (seq {})", cand.seq);
            if let Err(e) = pc.add_ice_candidate(init).await {
                eprintln!("probe: add_ice_candidate failed: {e}");
            }
        }
    }
    if !sent.load(Ordering::Relaxed) {
        return Err("data channel never opened — input events not sent".into());
    }
    // Give the agent a moment to receive/log the events.
    tokio::time::sleep(Duration::from_secs(1)).await;

    let _ = client
        .post(format!("{api}/sessions/{sid}/end"))
        .json(&EndSessionRequest {
            from_id: controller_id.clone(),
        })
        .send()
        .await;
    let _ = pc.close().await;
    println!("PROBE OK");
    Ok(())
}
