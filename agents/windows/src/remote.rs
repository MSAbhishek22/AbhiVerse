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
    ice_candidate_message, now_unix, session_answer_message, session_reject_message,
    signed_get_message, AnswerSessionRequest, IceCandidateListResponse, IceCandidateRequest,
    InputEvent, RtcConfig, SessionListResponse, SessionStatus, SessionView,
};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

/// What to do with an incoming session offer.
///
/// The default is Prompt: a human at this keyboard approves every
/// session. That is the safe default — but it makes unattended use
/// (laptop at home, owner in the market) impossible, because nobody
/// is here to press `y`. `trust_paired` is the explicit opt-in for
/// that case, and it is deliberately narrow: it auto-accepts ONLY
/// offers from the controller this device is already paired with.
/// An offer from anyone else is rejected outright, never prompted
/// (a prompt nobody reads is not consent, it is a hang).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OfferDecision {
    AutoAccept,
    Prompt,
    Reject,
}

fn decide_offer(
    session_controller_id: &str,
    paired_controller_id: &str,
    trust_paired: bool,
) -> OfferDecision {
    if session_controller_id == paired_controller_id {
        if trust_paired {
            OfferDecision::AutoAccept
        } else {
            OfferDecision::Prompt
        }
    } else {
        // Session creation is pairing-gated server-side, so this
        // should not happen; if it does, fail closed and loud.
        OfferDecision::Reject
    }
}

pub fn run(
    identity: DeviceIdentity,
    allow_input: bool,
    trust_paired: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(identity, allow_input, trust_paired))
}

struct Signalling {
    client: reqwest::Client,
    api: String,
    identity: Arc<DeviceIdentity>,
}

impl Signalling {
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        let device_id = &self.identity.info.device_id;
        let signed_path = if path.contains("as=") {
            path.to_string()
        } else if path.contains('?') {
            format!("{path}&as={device_id}")
        } else {
            format!("{path}?as={device_id}")
        };
        let ts = now_unix();
        let message = signed_get_message(&signed_path, device_id, ts);
        Ok(self
            .client
            .get(format!("{}{signed_path}", self.api))
            .header("x-abhiverse-timestamp", ts.to_string())
            .header("x-abhiverse-signature", self.identity.sign(&message))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}

async fn run_async(
    identity: DeviceIdentity,
    allow_input: bool,
    trust_paired: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data_dir = abhiverse_identity::default_data_dir();
    let Some(record) = abhiverse_agent_windows::load_pairing(&data_dir) else {
        return Err("this device is not paired — run --pair first".into());
    };
    let identity = Arc::new(identity);
    let sig = Signalling {
        // 10 s timeout: signalling polls must never hang forever.
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?,
        api: record.api_url.clone(),
        identity: identity.clone(),
    };
    let device_id = identity.info.device_id.clone();
    println!(
        "Remote desktop listener running for '{}' ({}). Waiting for session requests…",
        identity.info.name, device_id
    );
    if allow_input {
        println!("Input injection is ENABLED (--allow-input): an approved controller can move the mouse and type on this desktop.");
    } else {
        println!("Input injection is disabled: incoming input events will be logged only. Restart with --remote --allow-input to enable control.");
    }
    if trust_paired {
        println!("UNATTENDED MODE (--trust-paired): sessions from your paired controller are auto-accepted WITHOUT a prompt here. Anyone else is rejected. Revoke the pairing to stop this instantly.");
        if allow_input {
            println!("UNATTENDED MODE + INPUT: your paired phone can move the mouse and type on this desktop while you are away. Lock this computer (Win+L) when you leave it, and revoke on loss.");
        }
    }

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
            let accept = match decide_offer(
                &session.controller_id,
                &record.controller_id,
                trust_paired,
            ) {
                OfferDecision::AutoAccept => {
                    println!(
                        "Auto-accepting remote desktop session {sid} from paired controller '{who}' (--trust-paired)."
                    );
                    true
                }
                OfferDecision::Prompt => {
                    tokio::task::spawn_blocking(move || {
                        print!(
                            "Remote desktop request from '{who}' (session {sid}). Accept? [y/N] "
                        );
                        let _ = std::io::stdout().flush();
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                        line.trim().eq_ignore_ascii_case("y")
                    })
                    .await
                    .unwrap_or(false)
                }
                OfferDecision::Reject => {
                    eprintln!(
                        "Rejecting remote desktop session {sid} from '{}': not the paired controller.",
                        session.controller_id
                    );
                    false
                }
            };

            if accept {
                let sig_client = Signalling {
                    client: sig.client.clone(),
                    api: sig.api.clone(),
                    identity: sig.identity.clone(),
                };
                let id_clone = identity.clone();
                let api_clone = webrtc_api.clone();
                let config_clone = config.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_session(
                        sig_client,
                        id_clone,
                        api_clone,
                        config_clone,
                        session,
                        allow_input,
                    )
                    .await
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
    allow_input: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let device_id = identity.info.device_id.clone();
    let session_id = session.session_id.clone();
    let pc = Arc::new(webrtc_api.new_peer_connection(config).await?);
    let done = Arc::new(AtomicBool::new(false));

    // Local ICE candidates -> API (trickle), each post signed so
    // a stolen session id alone cannot inject candidates.
    {
        let client = sig.client.clone();
        let api = sig.api.clone();
        let sid = session_id.clone();
        let from = device_id.clone();
        let ice_identity = identity.clone();
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let client = client.clone();
            let api = api.clone();
            let sid = sid.clone();
            let from = from.clone();
            let ice_identity = ice_identity.clone();
            Box::pin(async move {
                if let Some(c) = candidate {
                    println!("agent local ICE candidate: {c}");
                    if let Ok(init) = c.to_json() {
                        let candidate_hash =
                            abhiverse_file_transfer::sha256_hex(init.candidate.as_bytes());
                        let ts = now_unix();
                        let message = ice_candidate_message(&sid, &from, &candidate_hash, ts);
                        let signature = ice_identity.sign(&message);
                        let body = IceCandidateRequest {
                            from_id: from,
                            candidate: init.candidate,
                            sdp_mid: init.sdp_mid,
                            sdp_mline_index: init.sdp_mline_index.map(u32::from),
                        };
                        let _ = client
                            .post(format!("{api}/sessions/{sid}/ice"))
                            .header("x-abhiverse-timestamp", ts.to_string())
                            .header("x-abhiverse-signature", signature)
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
                            Ok(event) => {
                                if allow_input {
                                    let actions = abhiverse_win_input::event_to_actions(
                                        &event,
                                        abhiverse_win_input::screen_size(),
                                    );
                                    match abhiverse_win_input::apply(&actions) {
                                        Ok(()) => println!("INPUT APPLIED: {event:?}"),
                                        Err(e) => println!("INPUT FAILED ({e}): {event:?}"),
                                    }
                                } else {
                                    println!("INPUT EVENT (injection disabled): {event:?}");
                                }
                            }
                            Err(_) => println!("INPUT (raw): {text}"),
                        }
                    }
                })
            }));
        })
    }));

    // Video (006b): if the offer asks for video, attach an H.264
    // sample track fed by desktop capture (Windows/DXGI) with a
    // test-pattern fallback, encoded with OpenH264.
    // The OpenH264 encoder is not Send, so video runs on a
    // dedicated thread with a stop flag.
    let video_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let quality = Arc::new(std::sync::Mutex::new(QualityTier::High));
    if session.sdp_offer.contains("m=video") {
        let track = Arc::new(TrackLocalStaticSample::new(
            webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {
                mime_type: webrtc::api::media_engine::MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                        .to_owned(),
                rtcp_feedback: vec![],
            },
            "video".to_owned(),
            "abhiverse-screen".to_owned(),
        ));
        match pc
            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
            .await
        {
            Ok(_) => {
                println!("Video track attached (H.264).");
                let stop = Arc::clone(&video_stop);
                let tier = Arc::clone(&quality);
                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => rt,
                        Err(e) => {
                            eprintln!("video: could not start runtime: {e}");
                            return;
                        }
                    };
                    rt.block_on(video_loop(track, stop, tier));
                });
            }
            Err(e) => {
                println!("Could not attach video track ({e}); continuing without video.");
            }
        }
    }

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
    println!("============================================================");
    println!(
        "  REMOTE SESSION ACTIVE — '{}' is connected to this computer.",
        session.controller_id
    );
    println!("  Keep this window open; closing it ends remote access.");
    println!("============================================================");
    abhiverse_win_input::indicator::session_started(&session.controller_id);

    // Remote ICE polling + remote end detection.
    let last_seq = Arc::new(AtomicU64::new(0));
    let mut polls: u64 = 0;
    while !done.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(700)).await;
        polls += 1;
        // Adaptive quality (006d): every ~3 s, read the WebRTC stats
        // and let pick_tier move the stream down/up a tier.
        if polls.is_multiple_of(4) {
            let stats = pc.get_stats().await;
            let mut fraction_lost = 0.0f64;
            let mut available_bps = 0.0f64;
            for report in stats.reports.values() {
                match report {
                    webrtc::stats::StatsReportType::RemoteInboundRTP(r) => {
                        fraction_lost = fraction_lost.max(r.fraction_lost);
                    }
                    webrtc::stats::StatsReportType::CandidatePair(c) => {
                        available_bps = available_bps.max(c.available_outgoing_bitrate);
                    }
                    _ => {}
                }
            }
            let mut current = quality.lock().unwrap();
            let next = pick_tier(*current, fraction_lost, available_bps);
            if next != *current {
                println!(
                    "video quality: {:?} -> {:?} (loss {:.1}%, est. {:.0} kbps available)",
                    *current,
                    next,
                    fraction_lost * 100.0,
                    available_bps / 1000.0
                );
                *current = next;
            }
        }
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
    video_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = pc.close().await;
    println!("============================================================");
    println!("  REMOTE SESSION ENDED ({session_id}).");
    println!("============================================================");
    abhiverse_win_input::indicator::session_ended();
    Ok(())
}

/// Split an Annex-B byte stream into NAL units (payload only,
/// start codes stripped).
fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i + 3 <= data.len() {
        let is_start = data[i] == 0
            && data[i + 1] == 0
            && (data[i + 2] == 1 || (i + 4 <= data.len() && data[i + 2] == 0 && data[i + 3] == 1));
        if is_start {
            let code_len = if data[i + 2] == 1 { 3 } else { 4 };
            if let Some(s) = start {
                if s < i {
                    units.push(&data[s..i]);
                }
            }
            start = Some(i + code_len);
            i += code_len;
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        if s < data.len() {
            units.push(&data[s..]);
        }
    }
    units.into_iter().filter(|u| !u.is_empty()).collect()
}

/// Adaptive stream quality. Downgrades react fast (packet loss or
/// an estimated available bitrate below the current target);
/// upgrades need a clean channel and clear headroom, so the stream
/// does not oscillate on a borderline link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum QualityTier {
    High,
    Medium,
    Low,
}

impl QualityTier {
    /// (width, height, fps, bitrate bps)
    fn params(self) -> (u32, u32, u32, u32) {
        match self {
            QualityTier::High => (640, 360, 10, 1_200_000),
            QualityTier::Medium => (640, 360, 5, 700_000),
            QualityTier::Low => (480, 270, 5, 350_000),
        }
    }
    fn down(self) -> QualityTier {
        match self {
            QualityTier::High => QualityTier::Medium,
            QualityTier::Medium | QualityTier::Low => QualityTier::Low,
        }
    }
    fn up(self) -> QualityTier {
        match self {
            QualityTier::Low => QualityTier::Medium,
            QualityTier::Medium | QualityTier::High => QualityTier::High,
        }
    }
}

fn pick_tier(current: QualityTier, fraction_lost: f64, available_bps: f64) -> QualityTier {
    let (_, _, _, bitrate) = current.params();
    if fraction_lost > 0.08 || (available_bps > 0.0 && available_bps < bitrate as f64 * 0.7) {
        return current.down();
    }
    if fraction_lost < 0.01 && current != QualityTier::High {
        let target = current.up();
        if available_bps > target.params().3 as f64 * 1.6 {
            return target;
        }
    }
    current
}

/// Encode and send screen frames until aborted.
///
/// Source priority: DXGI desktop duplication on Windows; the
/// synthetic test pattern everywhere else (and as a fallback).
/// The first real-hardware run happens on the user's machine —
/// see docs/remote-desktop.md.
async fn video_loop(
    track: Arc<TrackLocalStaticSample>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    tier: Arc<std::sync::Mutex<QualityTier>>,
) {
    use abhiverse_win_input::capture::{bgra_to_i420, test_pattern_frame, BgraFrame};
    let mut encoder: Option<openh264::encoder::Encoder> = None;
    let mut encoder_params: Option<(u32, u32, u32)> = None; // (w, h, bitrate)
    let mut width = 640u32;
    let mut height = 360u32;
    let mut frame_ms = 100u64;
    #[cfg(windows)]
    let mut capturer = abhiverse_win_input::capture::DxgiCapturer::new().ok();
    #[cfg(windows)]
    if capturer.is_some() {
        println!("video: streaming desktop via DXGI duplication");
    } else {
        println!("video: DXGI unavailable — streaming test pattern");
    }
    let mut tick: u64 = 0;
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(frame_ms));
    loop {
        ticker.tick().await;
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        tick += 1;
        // Adopt the latest quality tier (set by the stats watcher):
        // recreate the encoder when resolution or bitrate change,
        // and repace the frame ticker when the fps changes.
        let (tw, th, fps, bitrate) = tier.lock().unwrap().params();
        if encoder_params != Some((tw, th, bitrate)) {
            let config = openh264::encoder::EncoderConfig::new()
                .set_bitrate_bps(bitrate)
                .max_frame_rate(fps as f32);
            match openh264::encoder::Encoder::with_api_config(
                openh264::OpenH264API::from_source(),
                config,
            ) {
                Ok(e) => {
                    encoder = Some(e);
                    encoder_params = Some((tw, th, bitrate));
                }
                Err(e) => {
                    eprintln!("video: OpenH264 encoder failed to start: {e}");
                    return;
                }
            }
        }
        if tw != width || th != height || 1000 / fps as u64 != frame_ms {
            width = tw;
            height = th;
            frame_ms = 1000 / fps as u64;
            ticker = tokio::time::interval(std::time::Duration::from_millis(frame_ms));
        }
        let frame: BgraFrame = {
            #[cfg(windows)]
            {
                match capturer.as_mut().map(|c| c.capture(0)) {
                    Some(Ok(Some(f))) => downscale(f, width, height),
                    _ => test_pattern_frame(width, height, tick),
                }
            }
            #[cfg(not(windows))]
            {
                test_pattern_frame(width, height, tick)
            }
        };
        let i420 = bgra_to_i420(&frame);
        let yuv = openh264::formats::YUVBuffer::from_vec(i420, width as usize, height as usize);
        let encoder = encoder.as_mut().expect("encoder created above");
        let bitstream = match encoder.encode(&yuv) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("video: encode failed: {e}");
                continue;
            }
        };
        let au = bitstream.to_vec();
        if au.is_empty() {
            continue;
        }
        let units = split_annexb(&au);
        let count = units.len();
        for (i, nal) in units.into_iter().enumerate() {
            let sample = Sample {
                data: bytes::Bytes::copy_from_slice(nal),
                duration: if i + 1 == count {
                    std::time::Duration::from_millis(frame_ms)
                } else {
                    std::time::Duration::ZERO
                },
                ..Default::default()
            };
            if track.write_sample(&sample).await.is_err() {
                return; // peer went away
            }
        }
    }
}

/// Nearest-neighbour downscale to the streaming size (capture
/// frames arrive at native resolution).
#[cfg(windows)]
fn downscale(
    frame: abhiverse_win_input::capture::BgraFrame,
    width: u32,
    height: u32,
) -> abhiverse_win_input::capture::BgraFrame {
    use abhiverse_win_input::capture::BgraFrame;
    let (WIDTH, HEIGHT) = (width, height);
    if frame.width == WIDTH && frame.height == HEIGHT {
        return frame;
    }
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        let sy = y * frame.height / HEIGHT;
        for x in 0..WIDTH {
            let sx = x * frame.width / WIDTH;
            let si = ((sy * frame.width + sx) * 4) as usize;
            let di = ((y * WIDTH + x) * 4) as usize;
            pixels[di..di + 4].copy_from_slice(&frame.pixels[si..si + 4]);
        }
    }
    BgraFrame {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::QualityTier::{High, Low, Medium};
    use super::{pick_tier, QualityTier};

    #[test]
    fn tier_downgrades_on_loss() {
        assert_eq!(pick_tier(High, 0.20, 0.0), Medium);
        assert_eq!(pick_tier(Medium, 0.20, 0.0), Low);
        assert_eq!(pick_tier(Low, 0.50, 0.0), Low);
    }

    #[test]
    fn tier_downgrades_when_bandwidth_starved() {
        // High needs 1.2 Mbps; 500 kbps estimated -> down a tier.
        assert_eq!(pick_tier(High, 0.0, 500_000.0), Medium);
        // Unknown estimate (0) never downgrades on bandwidth alone.
        assert_eq!(pick_tier(High, 0.0, 0.0), High);
    }

    #[test]
    fn tier_upgrades_only_with_headroom() {
        // Medium -> High needs > 1.6x of 1.2 Mbps and ~no loss.
        assert_eq!(pick_tier(Medium, 0.0, 2_500_000.0), High);
        assert_eq!(pick_tier(Medium, 0.0, 1_500_000.0), Medium);
        assert_eq!(pick_tier(Medium, 0.05, 2_500_000.0), Medium);
        let _ = QualityTier::High; // keep imports obviously used
    }

    use super::decide_offer;
    use super::OfferDecision::{AutoAccept, Prompt, Reject};

    #[test]
    fn offer_decision_trusts_only_the_paired_controller() {
        // Paired + trusted: unattended auto-accept (the market case).
        assert_eq!(decide_offer("ctrl-1", "ctrl-1", true), AutoAccept);
        // Paired + not trusted: the safe default, prompt every time.
        assert_eq!(decide_offer("ctrl-1", "ctrl-1", false), Prompt);
        // Anyone else: rejected in both modes — trust is
        // per-controller, never blanket, and never a prompt that
        // nobody is here to answer.
        assert_eq!(decide_offer("ctrl-2", "ctrl-1", true), Reject);
        assert_eq!(decide_offer("ctrl-2", "ctrl-1", false), Reject);
    }

    use super::split_annexb;

    #[test]
    fn annexb_splits_into_nal_units() {
        let stream = [
            0, 0, 0, 1, 0x67, 0xAA, // SPS-ish
            0, 0, 1, 0x68, 0xBB, // PPS-ish (3-byte code)
            0, 0, 0, 1, 0x65, 0x01, 0x02, // IDR
        ];
        let units = split_annexb(&stream);
        assert_eq!(units.len(), 3);
        assert_eq!(units[0], [0x67, 0xAA]);
        assert_eq!(units[1], [0x68, 0xBB]);
        assert_eq!(units[2], [0x65, 0x01, 0x02]);
        assert!(split_annexb(&[]).is_empty());
        assert!(split_annexb(&[1, 2, 3]).is_empty());
    }

    #[test]
    fn openh264_encodes_a_pattern_frame() {
        use abhiverse_win_input::capture::{bgra_to_i420, test_pattern_frame};
        let config = openh264::encoder::EncoderConfig::new()
            .set_bitrate_bps(1_200_000)
            .max_frame_rate(10.0);
        let mut encoder = openh264::encoder::Encoder::with_api_config(
            openh264::OpenH264API::from_source(),
            config,
        )
        .expect("encoder starts");
        let frame = test_pattern_frame(640, 360, 1);
        let yuv = openh264::formats::YUVBuffer::from_vec(bgra_to_i420(&frame), 640, 360);
        let bitstream = encoder.encode(&yuv).expect("frame encodes");
        let au = bitstream.to_vec();
        assert!(!au.is_empty(), "encoded access unit must not be empty");
        let units = split_annexb(&au);
        assert!(!units.is_empty());
        // First unit should be SPS (NAL type 7) for a fresh encoder.
        assert_eq!(units[0][0] & 0x1F, 7);
    }
}
