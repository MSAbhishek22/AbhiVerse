//! Remote desktop sessions (Task 006, slice 006a).
//!
//! Polls the coordination API for session offers addressed to this
//! device. Each offer needs the local user's approval (terminal
//! prompt) before the agent builds a WebRTC peer connection and
//! posts its signed answer. Input events arriving on the
//! `abhiverse-input` data channel are logged — capture/encode and
//! input injection land in later slices (006b/006c), which need a
//! real Windows desktop to verify.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use abhiverse_identity::DeviceIdentity;
use abhiverse_protocol::{
    now_unix, session_answer_message, session_reject_message, AnswerSessionRequest,
    IceCandidateListResponse, IceCandidateRequest, InputEvent, RtcConfig, SessionListResponse,
    SessionStatus, SessionView,
};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

pub fn run(identity: DeviceIdentity) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(identity))
}

struct Signalling {
    client: reqwest::Client,
    api: String,
}

impl Signalling {
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .client
            .get(format!("{}{path}", self.api))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}

async fn run_async(
    identity: DeviceIdentity,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data_dir = abhiverse_identity::default_data_dir();
    let Some(record) = abhiverse_agent_windows::load_pairing(&data_dir) else {
        return Err("this device is not paired — run --pair first".into());
    };
    let sig = Signalling {
        client: reqwest::Client::new(),
        api: record.api_url.clone(),
    };
    let device_id = identity.info.device_id.clone();
    println!(
        "Remote desktop listener running for '{}' ({}). Waiting for session requests…",
        identity.info.name, device_id
    );

    let rtc: RtcConfig = sig.get_json("/config/rtc").await?;
    let config = rtc_configuration(&rtc);

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut m)?;
    let webrtc_api = Arc::new(
        APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build(),
    );

    let identity = Arc::new(identity);
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let list: SessionListResponse =
            match sig.get_json(&format!("/sessions?for={device_id}")).await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("session poll failed: {e}");
                    continue;
                }
            };
        for session in list.sessions {
            if session.status != SessionStatus::Offered || seen.contains(&session.session_id) {
                continue;
            }
            seen.insert(session.session_id.clone());
            let who = if session.controller_id == record.controller_id {
                record.controller_name.clone()
            } else {
                session.controller_id.clone()
            };
            let sid = session.session_id.clone();
            let accept = tokio::task::spawn_blocking(move || {
                print!("Remote desktop request from '{who}' (session {sid}). Accept? [y/N] ");
                let _ = std::io::stdout().flush();
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                line.trim().eq_ignore_ascii_case("y")
            })
            .await
            .unwrap_or(false);

            if accept {
                let sig_client = Signalling {
                    client: sig.client.clone(),
                    api: sig.api.clone(),
                };
                let id_clone = identity.clone();
                let api_clone = webrtc_api.clone();
                let config_clone = config.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_session(sig_client, id_clone, api_clone, config_clone, session).await
                    {
                        eprintln!("session failed: {e}");
                    }
                });
            } else {
                let ts = now_unix();
                let message = session_reject_message(&session.session_id, ts);
                let _ = sig
                    .client
                    .post(format!(
                        "{}/sessions/{}/reject",
                        sig.api, session.session_id
                    ))
                    .header("x-abhiverse-timestamp", ts.to_string())
                    .header("x-abhiverse-signature", identity.sign(&message))
                    .send()
                    .await;
                println!("Session {} rejected.", session.session_id);
            }
        }
    }
}

pub fn rtc_configuration(rtc: &RtcConfig) -> RTCConfiguration {
    RTCConfiguration {
        ice_servers: rtc
            .ice_servers
            .iter()
            .map(|s| RTCIceServer {
                urls: s.urls.clone(),
                username: s.username.clone().unwrap_or_default(),
                credential: s.credential.clone().unwrap_or_default(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

async fn handle_session(
    sig: Signalling,
    identity: Arc<DeviceIdentity>,
    webrtc_api: Arc<webrtc::api::API>,
    config: RTCConfiguration,
    session: SessionView,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let device_id = identity.info.device_id.clone();
    let session_id = session.session_id.clone();
    let pc = Arc::new(webrtc_api.new_peer_connection(config).await?);
    let done = Arc::new(AtomicBool::new(false));

    // Local ICE candidates -> API (trickle).
    {
        let client = sig.client.clone();
        let api = sig.api.clone();
        let sid = session_id.clone();
        let from = device_id.clone();
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let client = client.clone();
            let api = api.clone();
            let sid = sid.clone();
            let from = from.clone();
            Box::pin(async move {
                if let Some(c) = candidate {
                    println!("agent local ICE candidate: {c}");
                    if let Ok(init) = c.to_json() {
                        let body = IceCandidateRequest {
                            from_id: from,
                            candidate: init.candidate,
                            sdp_mid: init.sdp_mid,
                            sdp_mline_index: init.sdp_mline_index.map(u32::from),
                        };
                        let _ = client
                            .post(format!("{api}/sessions/{sid}/ice"))
                            .json(&body)
                            .send()
                            .await;
                    }
                }
            })
        }));
    }

    // Connection state -> mark done when the peer goes away.
    {
        let done = done.clone();
        pc.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
            let done = done.clone();
            Box::pin(async move {
                println!("Session peer connection state: {state}");
                if matches!(
                    state,
                    RTCPeerConnectionState::Closed | RTCPeerConnectionState::Failed
                ) {
                    done.store(true, Ordering::Relaxed);
                }
            })
        }));
    }

    // ICE connection state for diagnostics.
    pc.on_ice_connection_state_change(Box::new(move |state| {
        Box::pin(async move {
            println!("Session ICE connection state: {state}");
        })
    }));

    // Incoming data channels (the controller opens `abhiverse-input`).
    pc.on_data_channel(Box::new(move |dc| {
        Box::pin(async move {
            println!("Data channel opened: {}", dc.label());
            dc.on_message(Box::new(move |msg: DataChannelMessage| {
                Box::pin(async move {
                    if msg.is_string {
                        let text = String::from_utf8_lossy(&msg.data);
                        match serde_json::from_str::<InputEvent>(&text) {
                            Ok(event) => println!("INPUT EVENT: {event:?}"),
                            Err(_) => println!("INPUT (raw): {text}"),
                        }
                    }
                })
            }));
        })
    }));

    // Offer -> answer, posted with the device signature.
    pc.set_remote_description(RTCSessionDescription::offer(session.sdp_offer.clone())?)
        .await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;
    let local = pc
        .local_description()
        .await
        .ok_or("missing local description")?;
    let ts = now_unix();
    let answer_hash = abhiverse_file_transfer::sha256_hex(local.sdp.as_bytes());
    let message = session_answer_message(&session_id, &answer_hash, ts);
    sig.client
        .post(format!("{}/sessions/{}/answer", sig.api, session_id))
        .header("x-abhiverse-timestamp", ts.to_string())
        .header("x-abhiverse-signature", identity.sign(&message))
        .json(&AnswerSessionRequest {
            sdp_answer: local.sdp.clone(),
        })
        .send()
        .await?
        .error_for_status()?;
    println!("Session {session_id} answered; waiting for the peer to connect…");

    // Remote ICE polling + remote end detection.
    let last_seq = Arc::new(AtomicU64::new(0));
    while !done.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(700)).await;
        let after = last_seq.load(Ordering::Relaxed);
        if let Ok(list) = sig
            .get_json::<IceCandidateListResponse>(&format!(
                "/sessions/{session_id}/ice?as={device_id}&after={after}"
            ))
            .await
        {
            for cand in list.candidates {
                last_seq.fetch_max(cand.seq, Ordering::Relaxed);
                let init = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                    candidate: cand.candidate,
                    sdp_mid: cand.sdp_mid,
                    sdp_mline_index: cand.sdp_mline_index.map(|v| v as u16),
                    username_fragment: None,
                };
                if let Err(e) = pc.add_ice_candidate(init).await {
                    eprintln!("could not add remote ICE candidate: {e}");
                }
            }
        }
        if let Ok(view) = sig
            .get_json::<SessionView>(&format!("/sessions/{session_id}?as={device_id}"))
            .await
        {
            if matches!(
                view.status,
                SessionStatus::Ended | SessionStatus::Rejected | SessionStatus::Expired
            ) {
                println!("Session {session_id} ended by the other side.");
                break;
            }
        }
    }
    let _ = pc.close().await;
    println!("Session {session_id} closed.");
    Ok(())
}
