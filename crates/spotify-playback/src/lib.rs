//! Experimental normal librespot Player output adapter. All Spotify protocol,
//! credential and decoder behavior stays in unmodified upstream librespot.
//! This library's custom sink only accepts normally decoded floating-point PCM.
//! Run it in an isolated child: upstream Session can exit on a non-Premium account.

mod diagnostics;

use librespot_core::{authentication::Credentials, Session, SessionConfig, SpotifyUri};
use librespot_metadata::audio::item::{AudioItem, UniqueFields};
use librespot_playback::{
    audio_backend::{Sink, SinkError, SinkResult},
    config::{Bitrate, PlayerConfig},
    convert::Converter,
    decoder::AudioPacket,
    mixer::NoOpVolume,
    player::{Player, PlayerEvent},
    NUM_CHANNELS, SAMPLE_RATE,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_PCM_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_FRAMES: usize = MAX_PCM_BYTES / std::mem::size_of::<[f32; 2]>();
pub const PREPARATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAGIC: &[u8; 8] = b"PULSESP1";
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_PACKET_FRAMES: usize = 4096;
const METADATA: u8 = 1;
const PCM: u8 = 2;
const COMPLETE: u8 = 3;
const ERROR: u8 = 4;

// Credentials are native-only. Deliberately no Debug implementation.
#[derive(Serialize, Deserialize)]
pub struct PrepareRequest {
    pub client_id: String,
    pub access_token: String,
    pub track_uri: String,
    pub expected_duration_ms: Option<u64>,
}
impl Drop for PrepareRequest {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpotifyTrackMetadata {
    pub uri: String,
    /// Provider-listed playable release chosen for the requested URI.
    #[serde(default)]
    pub resolved_uri: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u64,
}
pub struct PreparedSpotifyTrack {
    pub samples: Vec<[f32; 2]>,
    pub sample_rate: u32,
    pub metadata: SpotifyTrackMetadata,
}
#[derive(Serialize, Deserialize)]
struct Completion {
    frames: u64,
    sample_rate: u32,
}

/// Identity used by unmodified librespot for native playback on this platform.
/// Obtain a separate native grant for this identity; Web API grants stay separate.
pub fn native_client_id() -> String {
    SessionConfig::default().client_id
}

#[derive(Debug, Serialize)]
pub struct ClientIdentityProbe {
    pub developer_accepted: bool,
    pub native_accepted: bool,
    pub developer_error: Option<String>,
    pub native_error: Option<String>,
}

fn validate_public_client_id(client_id: &str) -> Result<(), String> {
    if client_id.len() != 32 || !client_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Client-token diagnostic requires a public 32-character hexadecimal developer Client ID".into());
    }
    Ok(())
}

fn client_probe_error(error: librespot_core::Error) -> String {
    use librespot_core::http_client::HttpClientError;
    let status = match error.error.downcast_ref::<HttpClientError>() {
        Some(HttpClientError::StatusCode(status)) => format!("; HTTP {}", status.as_u16()),
        _ => String::new(),
    };
    format!("Client-token acquisition failed ({:?}{status}); no account sign-in or audio request was made", error.kind)
}

async fn probe_session_client_token(config: SessionConfig) -> Result<(), String> {
    let session = Session::new(config, None);
    let result = match tokio::time::timeout(Duration::from_secs(20), session.spclient().client_token())
        .await
    {
        Ok(Ok(mut token)) => {
            token.zeroize();
            Ok(())
        }
        Ok(Err(error)) => Err(client_probe_error(error)),
        Err(_) => Err("Client-token acquisition timed out after 20 seconds; no account sign-in or audio request was made".into()),
    };
    session.shutdown();
    result
}

/// Credential-free diagnostic for a dedicated pre-Tauri CLI process. Both normal
/// SDK requests use the same device/OS/version configuration and no account login.
/// Only the public client identity differs; returned SDK tokens are never emitted.
pub fn probe_client_identity(developer_id: &str) -> Result<ClientIdentityProbe, String> {
    validate_public_client_id(developer_id)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "Could not start client-token diagnostic runtime".to_string())?;
    let native_config = SessionConfig::default();
    let mut developer_config = native_config.clone();
    developer_config.client_id = developer_id.to_string();
    runtime.block_on(async {
        let developer = probe_session_client_token(developer_config).await;
        let native = probe_session_client_token(native_config).await;
        Ok(ClientIdentityProbe {
            developer_accepted: developer.is_ok(),
            native_accepted: native.is_ok(),
            developer_error: developer.err(),
            native_error: native.err(),
        })
    })
}

fn parse_track_uri(value: &str) -> Result<SpotifyUri, String> {
    let id = value.strip_prefix("spotify:track:").ok_or_else(|| {
        "Expected a Spotify catalog track URI: spotify:track:<22-character ID>; copy the complete URI from the library result".to_string()
    })?;
    if id.len() != 22 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(format!("Spotify track IDs must contain exactly 22 ASCII letters or digits; received {} characters. Copy the complete URI from the library result rather than reconstructing the ID", id.chars().count()));
    }
    let uri = SpotifyUri::from_uri(value).map_err(|_| {
        "Invalid Spotify catalog track ID; copy the complete URI from the library result"
            .to_string()
    })?;
    if !matches!(uri, SpotifyUri::Track { .. }) {
        return Err("Only Spotify catalog track URIs can be prepared for these decks".into());
    }
    Ok(uri)
}

/// Validate the same source identity before queuing, source mutation or network access.
pub fn validate_track_uri(value: &str) -> Result<(), String> {
    parse_track_uri(value).map(|_| ())
}

fn validate_request(request: &PrepareRequest) -> Result<SpotifyUri, String> {
    if request.client_id != native_client_id() {
        return Err("Native Spotify playback requires a separate native login for this platform's librespot identity; a Web API application grant cannot be substituted".into());
    }
    if request.access_token.is_empty() || request.access_token.len() > 8192 {
        return Err("Your Spotify session has no usable playback token; sign in again".into());
    }
    let uri = parse_track_uri(&request.track_uri)?;
    if let Some(duration) = request.expected_duration_ms {
        validate_duration(duration)?;
    }
    Ok(uri)
}
fn validate_duration(duration_ms: u64) -> Result<(), String> {
    if duration_ms == 0 {
        return Err("Spotify did not provide a valid track duration".into());
    }
    let frames = (u128::from(duration_ms) * u128::from(SAMPLE_RATE)).div_ceil(1000);
    if frames > MAX_FRAMES as u128 {
        return Err("This experimental Spotify source is limited to about 12 minutes 40 seconds / 256 MiB of decoded audio per deck".into());
    }
    Ok(())
}
fn validate_integrity(
    frames: usize,
    metadata: &SpotifyTrackMetadata,
    expected_duration_ms: Option<u64>,
) -> Result<(), String> {
    validate_duration(metadata.duration_ms)?;
    if frames == 0 || frames > MAX_FRAMES {
        return Err("Spotify returned no complete bounded audio track".into());
    }
    let actual_ms = (frames as u64).saturating_mul(1000) / u64::from(SAMPLE_RATE);
    if actual_ms.abs_diff(metadata.duration_ms) > 1000 {
        return Err("Spotify preparation ended before a full track was decoded; partial or preview audio was discarded".into());
    }
    if expected_duration_ms.is_some_and(|expected| expected.abs_diff(metadata.duration_ms) > 1000) {
        return Err(
            "Spotify returned a different track duration; the source was not loaded".into(),
        );
    }
    Ok(())
}
fn write_record(writer: &mut impl Write, kind: u8, bytes: &[u8]) -> std::io::Result<()> {
    writer.write_all(&[kind])?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}
struct Output {
    writer: Mutex<BufWriter<std::io::Stdout>>,
    frames: AtomicU64,
    failure: Mutex<Option<String>>,
}
impl Output {
    fn record(&self, kind: u8, bytes: &[u8]) -> Result<(), String> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| "Playback output unavailable".to_string())?;
        write_record(&mut *writer, kind, bytes)
            .map_err(|_| "Native playback output pipe closed".into())
    }
    fn fail(&self, message: &str) -> SinkError {
        if let Ok(mut failed) = self.failure.lock() {
            *failed = Some(message.into());
        }
        SinkError::OnWrite(message.into())
    }
}
struct MemorySink {
    output: Arc<Output>,
}
impl Sink for MemorySink {
    fn write(&mut self, packet: AudioPacket, _: &mut Converter) -> SinkResult<()> {
        let AudioPacket::Samples(samples) = packet else {
            return Err(self
                .output
                .fail("Decoded PCM was unavailable; encoded/passthrough output is not accepted"));
        };
        if !samples.len().is_multiple_of(NUM_CHANNELS as usize) {
            return Err(self
                .output
                .fail("Spotify decoder returned an invalid stereo packet"));
        }
        let frames = samples.len() / NUM_CHANNELS as usize;
        let old = self.output.frames.load(Ordering::Acquire);
        if old.saturating_add(frames as u64) > MAX_FRAMES as u64 {
            return Err(self
                .output
                .fail("Spotify decoded audio exceeded the 256 MiB preparation limit"));
        }
        if samples
            .iter()
            .any(|sample| !sample.is_finite() || sample.abs() > f32::MAX as f64)
        {
            return Err(self
                .output
                .fail("Spotify decoder returned invalid audio samples"));
        }
        for chunk in samples.chunks(MAX_PACKET_FRAMES * NUM_CHANNELS as usize) {
            let mut pcm = Vec::with_capacity(chunk.len() * 4);
            for sample in chunk {
                pcm.extend_from_slice(&(*sample as f32).to_le_bytes());
            }
            self.output
                .record(PCM, &pcm)
                .map_err(|_| self.output.fail("Native playback output pipe closed"))?;
        }
        self.output
            .frames
            .fetch_add(frames as u64, Ordering::Release);
        Ok(())
    }
}
fn metadata(item: &AudioItem) -> SpotifyTrackMetadata {
    let (artists, album) = match &item.unique_fields {
        UniqueFields::Track { artists, album, .. } => (
            artists.iter().map(|a| a.name.clone()).collect(),
            album.clone(),
        ),
        _ => (Vec::new(), String::new()),
    };
    SpotifyTrackMetadata {
        uri: item.uri.clone(),
        resolved_uri: None,
        title: item.name.clone(),
        artists,
        album,
        duration_ms: u64::from(item.duration_ms),
    }
}
// Upstream's connection module is private, so its AP ErrorCode enum cannot be
// downcast here. Classify only its exact fixed public error phrases, never echo
// arbitrary upstream Display/debug output (which can contain sensitive URLs).
fn authentication_reason(message: &str) -> Option<&'static str> {
    match message {
        "Login failed with reason: Protocol error" => Some("protocol error"),
        "Login failed with reason: Try another access point" => Some("access point unavailable"),
        "Login failed with reason: Bad connection ID" => Some("invalid connection"),
        "Login failed with reason: Travel restriction" => Some("account travel restriction"),
        "Login failed with reason: Premium account required" => Some("Premium account required"),
        "Login failed with reason: Bad credentials" => Some("token credentials rejected"),
        "Login failed with reason: Could not validate credentials" => {
            Some("token credentials could not be validated")
        }
        "Login failed with reason: Account exists" => Some("account state rejected"),
        "Login failed with reason: Extra verification required" => {
            Some("additional account verification required")
        }
        "Login failed with reason: Invalid app key" => Some("application access rejected"),
        "Login failed with reason: Application banned" => Some("application access disabled"),
        _ => None,
    }
}
fn authentication_error(error: librespot_core::Error) -> String {
    // The message is used exclusively as a lookup key and is never returned or logged.
    let reason = authentication_reason(&error.error.to_string());
    let stage = if reason.is_some() {
        "Spotify access-point token sign-in"
    } else {
        "Native Spotify session sign-in"
    };
    let reason = reason
        .map(|reason| format!("; {reason}"))
        .unwrap_or_default();
    format!("{stage} failed ({:?}{reason}) before loading track metadata or audio. Sign in again using the separate native playback login. Spotify may still refuse this session or account. No alternate application identity was attempted.", error.kind)
}

fn upstream_stage_error(stage: &str, error: librespot_core::Error) -> String {
    use librespot_core::{
        http_client::HttpClientError, spclient::SpClientError, spotify_id::SpotifyIdError,
        spotify_uri::SpotifyUriError,
    };
    use librespot_metadata::MetadataError;
    let detail = if let Some(HttpClientError::StatusCode(status)) =
        error.error.downcast_ref::<HttpClientError>()
    {
        format!("HTTP {}", status.as_u16())
    } else if error.error.downcast_ref::<SpotifyIdError>().is_some() {
        "invalid native metadata entity identifier".into()
    } else if error.error.downcast_ref::<SpotifyUriError>().is_some() {
        "native source URI conversion failed".into()
    } else if let Some(error) = error.error.downcast_ref::<MetadataError>() {
        match error {
            MetadataError::Empty => "empty native metadata response",
            MetadataError::NonPlayable => "source is not playable",
            MetadataError::InvalidDuration(_) => "invalid native metadata duration",
            MetadataError::ExplicitContentFiltered => {
                "account explicit-content setting blocks this source"
            }
        }
        .into()
    } else if error.error.downcast_ref::<SpClientError>().is_some() {
        "missing expected native service response field".into()
    } else {
        "no additional credential-safe provider detail available".into()
    };
    format!("Spotify {stage} failed ({:?}; {detail}) after Premium session sign-in and before audio decoding. This identifies a native service or metadata failure, not an audio-device error. No alternate application identity or token path was attempted.", error.kind)
}

/// Resolve only the alternatives explicitly advertised for this source by
/// Spotify metadata. Keep upstream availability/explicit-content enforcement.
async fn playable_item(session: &Session, item: AudioItem) -> Result<AudioItem, String> {
    if item.availability.is_err() {
        return Err("Spotify says the selected track is unavailable for this account".into());
    }
    if !item.files.is_empty() {
        return Ok(item);
    }
    if let Some(alternatives) = item.alternatives {
        for uri in alternatives.0 {
            if !matches!(uri, SpotifyUri::Track { .. }) {
                continue;
            }
            if let Ok(candidate) = AudioItem::get_file(session, uri).await {
                if candidate.availability.is_ok() && !candidate.files.is_empty() {
                    return Ok(candidate);
                }
            }
        }
    }
    Err("Spotify supplied no playable release for the selected track".into())
}

fn selected_metadata(item: &AudioItem, requested: &str) -> SpotifyTrackMetadata {
    let mut info = metadata(item);
    if info.uri != requested {
        info.resolved_uri = Some(info.uri.clone());
    }
    info.uri = requested.into();
    info
}

async fn worker_prepare(request: &PrepareRequest, output: Arc<Output>) -> Result<(), String> {
    let track = validate_request(request)?;
    // The caller supplies a separately authorized native token. Validation pins
    // its declared identity to upstream's platform default; never rebind a web
    // grant or try alternate identities after a rejection.
    let config = SessionConfig {
        client_id: request.client_id.clone(),
        device_id: uuid::Uuid::new_v4().to_string(),
        proxy: None,
        ap_port: None,
        tmp_dir: std::env::temp_dir(),
        autoplay: Some(false),
    };
    let session = Session::new(config, None); // No persistent credential/audio/source cache.
    session
        .connect(
            Credentials::with_access_token(request.access_token.clone()),
            false,
        )
        .await
        .map_err(authentication_error)?;
    // Preserve upstream Premium enforcement. Unknown account state is not a grant.
    let premium_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match session.get_user_attribute("type").as_deref() {
            Some("premium") => break,
            Some(_) => {
                session.shutdown();
                return Err("Normal Spotify playback requires an eligible Premium account".into());
            }
            None if Instant::now() < premium_deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            None => {
                session.shutdown();
                return Err(
                    "Spotify did not confirm this session's Premium playback eligibility".into(),
                );
            }
        }
    }
    // These are the same normal, cached upstream requests needed by metadata
    // retrieval. Name the failing stage without changing identity or transport.
    session
        .spclient()
        .client_token()
        .await
        .map_err(|error| upstream_stage_error("client-token acquisition", error))?;
    session
        .login5()
        .auth_token()
        .await
        .map_err(|error| upstream_stage_error("native session token renewal (login5)", error))?;
    let item = AudioItem::get_file(&session, track.clone())
        .await
        .map_err(|error| upstream_stage_error("full-track metadata retrieval", error))?;
    let item = playable_item(&session, item).await?;
    let resolved_track = item.track_id.clone();
    let resolved_uri = item.uri.clone();
    let info = selected_metadata(&item, &request.track_uri);
    validate_duration(info.duration_ms)?;
    if request
        .expected_duration_ms
        .is_some_and(|expected| expected.abs_diff(info.duration_ms) > 1000)
    {
        return Err("Spotify source metadata did not match the selected track duration".into());
    }
    let bytes =
        serde_json::to_vec(&info).map_err(|_| "Could not encode playback metadata".to_string())?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Spotify metadata exceeded the preparation limit".into());
    }
    output.record(METADATA, &bytes)?;
    let make_player = || {
        let sink_output = output.clone();
        Player::new(
            PlayerConfig {
                bitrate: Bitrate::Bitrate320,
                normalisation: false,
                passthrough: false,
                gapless: false,
                ..PlayerConfig::default()
            },
            session.clone(),
            Box::new(NoOpVolume),
            move || {
                Box::new(MemorySink {
                    output: sink_output,
                })
            },
        )
    };
    let mut player = make_player();
    let mut events = player.get_player_event_channel();
    let mut retry_attempts = 0;
    player.load(resolved_track.clone(), true, 0);
    let result = loop {
        if let Some(error) = output
            .failure
            .lock()
            .map_err(|_| "Playback state unavailable")?
            .clone()
        {
            break Err(error);
        }
        if session.is_invalid() {
            break Err("Spotify ended this playback session; audio was not loaded".into());
        }
        match tokio::time::timeout(Duration::from_millis(100), events.recv()).await {
            Ok(Some(PlayerEvent::EndOfTrack { track_id, .. })) if track_id == resolved_track => {
                let frames = output.frames.load(Ordering::Acquire) as usize;
                let checked = validate_integrity(frames, &info, request.expected_duration_ms);
                if let Err(error) = checked {
                    break Err(error);
                }
                let complete = serde_json::to_vec(&Completion {
                    frames: frames as u64,
                    sample_rate: SAMPLE_RATE,
                })
                .map_err(|_| "Could not encode playback completion".to_string())?;
                break output.record(COMPLETE, &complete);
            }
            Ok(Some(PlayerEvent::Unavailable { track_id, .. })) if track_id == resolved_track => {
                if diagnostics::retryable_before_audio(
                    output.frames.load(Ordering::Acquire),
                    retry_attempts,
                ) {
                    // Retry an upstream metadata/delivery request or key timeout with the
                    // same accepted session. No credential/format fallback, and
                    // never append a second stream after PCM has started.
                    player.stop();
                    drop(player);
                    retry_attempts += 1;
                    tokio::time::sleep(Duration::from_millis(750)).await;
                    diagnostics::reset();
                    player = make_player();
                    events = player.get_player_event_channel();
                    player.load(resolved_track.clone(), true, 0);
                    continue;
                }
                break Err(diagnostics::unavailable_error());
            }
            Ok(Some(PlayerEvent::TrackChanged { audio_item }))
                if audio_item.uri != resolved_uri =>
            {
                break Err(
                    "Spotify selected a different source; it was not loaded into the deck".into(),
                )
            }
            Ok(None) => {
                break Err("Spotify's playback worker stopped before the track was complete".into())
            }
            _ => {}
        }
    };
    player.stop();
    session.shutdown();
    drop(player);
    result
}

/// Call only from the isolated binary's `--spotify-worker` mode, before Tauri or
/// logging initialization. JSON input holds the user's native token; only framed
/// metadata / normal decoded PCM / sanitized errors are written to stdout.
pub fn run_worker_from_stdio() -> Result<(), String> {
    diagnostics::install();
    let output = Arc::new(Output {
        writer: Mutex::new(BufWriter::new(std::io::stdout())),
        frames: AtomicU64::new(0),
        failure: Mutex::new(None),
    });
    {
        let mut writer = output
            .writer
            .lock()
            .map_err(|_| "Worker output unavailable".to_string())?;
        writer
            .write_all(MAGIC)
            .and_then(|_| writer.flush())
            .map_err(|_| "Worker output pipe closed".to_string())?;
    }
    let result = (|| {
        let mut bytes = Zeroizing::new(Vec::new());
        std::io::stdin()
            .take((MAX_MESSAGE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read native playback request".to_string())?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err("Native playback request exceeded its size limit".into());
        }
        let request: PrepareRequest = serde_json::from_slice(&bytes)
            .map_err(|_| "Native playback request was invalid".to_string())?;
        bytes.zeroize();
        validate_request(&request)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "Could not start native playback worker runtime".to_string())?;
        runtime.block_on(async {
            tokio::time::timeout(
                PREPARATION_TIMEOUT,
                worker_prepare(&request, output.clone()),
            )
            .await
            .map_err(|_| {
                "Spotify preparation timed out after 120 seconds; no partial audio was loaded"
                    .to_string()
            })?
        })
    })();
    if let Err(error) = &result {
        let _ = output.record(ERROR, error.as_bytes());
    }
    result
}

struct ChildRun {
    child: Arc<Mutex<Child>>,
    done: Arc<AtomicBool>,
    reason: Arc<AtomicU8>,
    watchdog: Option<std::thread::JoinHandle<()>>,
}
impl ChildRun {
    fn new(child: Child, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        let child = Arc::new(Mutex::new(child));
        let done = Arc::new(AtomicBool::new(false));
        let reason = Arc::new(AtomicU8::new(0));
        let watched_child = child.clone();
        let watched_done = done.clone();
        let watched_reason = reason.clone();
        let watchdog = std::thread::Builder::new()
            .name("spotify-worker-watchdog".into())
            .spawn(move || {
                let deadline = Instant::now() + PREPARATION_TIMEOUT + Duration::from_secs(5);
                while !watched_done.load(Ordering::Acquire) {
                    let stopped = if cancel.load(Ordering::Acquire) {
                        1
                    } else if Instant::now() >= deadline {
                        2
                    } else {
                        0
                    };
                    if stopped != 0 {
                        watched_reason.store(stopped, Ordering::Release);
                        if let Ok(mut child) = watched_child.lock() {
                            let _ = child.kill();
                        }
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            })
            .map_err(|_| {
                if let Ok(mut child) = child.lock() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                "Could not start playback cancellation watchdog".to_string()
            })?;
        Ok(Self {
            child,
            done,
            reason,
            watchdog: Some(watchdog),
        })
    }
    fn error(&self) -> String {
        match self.reason.load(Ordering::Acquire) {
            1 => "Spotify track preparation was cancelled".into(),
            2 => "Spotify preparation timed out after 120 seconds; no partial audio was loaded".into(),
            _ => "Spotify playback worker ended without complete audio. Check Premium eligibility and the separate native playback login; no fallback identity was used.".into(),
        }
    }
    fn wait(&self) -> Result<ExitStatus, String> {
        loop {
            if let Some(status) = self
                .child
                .lock()
                .map_err(|_| "Playback worker process unavailable")?
                .try_wait()
                .map_err(|_| "Could not inspect playback worker process")?
            {
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for ChildRun {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}
fn read_output(
    reader: &mut impl Read,
    request: &PrepareRequest,
) -> Result<PreparedSpotifyTrack, String> {
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| "Worker protocol was not initialized".to_string())?;
    if &magic != MAGIC {
        return Err("Worker returned an invalid audio protocol".into());
    }
    let mut metadata: Option<SpotifyTrackMetadata> = None;
    let mut samples = Vec::new();
    loop {
        let mut header = [0u8; 5];
        reader
            .read_exact(&mut header)
            .map_err(|_| "Worker stopped before full-track completion".to_string())?;
        let size = u32::from_le_bytes(header[1..].try_into().unwrap()) as usize;
        let limit = if header[0] == PCM {
            MAX_PACKET_FRAMES * 8
        } else {
            MAX_MESSAGE_BYTES
        };
        if size > limit {
            return Err("Worker returned an oversized audio record".into());
        }
        let mut bytes = vec![0u8; size];
        reader
            .read_exact(&mut bytes)
            .map_err(|_| "Worker returned incomplete audio data".to_string())?;
        match header[0] {
            METADATA if metadata.is_none() && samples.is_empty() => {
                let info: SpotifyTrackMetadata = serde_json::from_slice(&bytes)
                    .map_err(|_| "Worker returned invalid track metadata")?;
                if info.uri != request.track_uri {
                    return Err("Prepared Spotify source did not match the selected track".into());
                }
                if let Some(uri) = &info.resolved_uri {
                    if validate_track_uri(uri).is_err() {
                        return Err("Worker returned an invalid resolved Spotify source".into());
                    }
                }
                validate_duration(info.duration_ms)?;
                metadata = Some(info);
            }
            PCM if metadata.is_some() => {
                if size == 0
                    || !size.is_multiple_of(8)
                    || samples.len().saturating_add(size / 8) > MAX_FRAMES
                {
                    return Err("Worker returned invalid or excessive stereo PCM".into());
                }
                samples
                    .try_reserve(size / 8)
                    .map_err(|_| "Not enough memory to prepare this Spotify track")?;
                for frame in bytes.as_chunks::<8>().0 {
                    let left = f32::from_le_bytes(frame[..4].try_into().unwrap());
                    let right = f32::from_le_bytes(frame[4..].try_into().unwrap());
                    if !left.is_finite() || !right.is_finite() {
                        return Err("Worker returned non-finite audio samples".into());
                    }
                    samples.push([left, right]);
                }
            }
            COMPLETE if metadata.is_some() => {
                let complete: Completion = serde_json::from_slice(&bytes)
                    .map_err(|_| "Worker returned invalid completion data")?;
                if complete.sample_rate != SAMPLE_RATE || complete.frames != samples.len() as u64 {
                    return Err("Worker completion did not match the prepared audio".into());
                }
                let info = metadata.take().unwrap();
                validate_integrity(samples.len(), &info, request.expected_duration_ms)?;
                return Ok(PreparedSpotifyTrack {
                    samples,
                    sample_rate: complete.sample_rate,
                    metadata: info,
                });
            }
            ERROR => {
                return Err(String::from_utf8(bytes)
                    .map_err(|_| "Worker returned an invalid error message")?)
            }
            _ => return Err("Worker returned audio records in an invalid order".into()),
        }
    }
}

/// Prepares a full track in an isolated child. Tokens travel through private stdin,
/// never command-line arguments, environment, logs, frontend responses or files.
/// Cancellation kills the worker and discards all incomplete audio.
pub fn prepare_in_worker(
    executable: &Path,
    request: PrepareRequest,
    cancel: Arc<AtomicBool>,
) -> Result<PreparedSpotifyTrack, String> {
    validate_request(&request)?;
    if cancel.load(Ordering::Acquire) {
        return Err("Spotify track preparation was cancelled".into());
    }
    let mut child = Command::new(executable)
        .arg("--spotify-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("RUST_LOG")
        .spawn()
        .map_err(|_| "Could not start the isolated Spotify playback worker".to_string())?;
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Playback worker input pipe unavailable".into());
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Playback worker output pipe unavailable".into());
    };
    let run = ChildRun::new(child, cancel)?;
    let mut bytes = Zeroizing::new(
        serde_json::to_vec(&request).map_err(|_| "Could not encode native playback request")?,
    );
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Native playback request exceeded its size limit".into());
    }
    {
        let mut stdin = stdin;
        stdin.write_all(&bytes).map_err(|_| run.error())?;
    }
    bytes.zeroize();
    let result = read_output(&mut BufReader::new(stdout), &request);
    if run.reason.load(Ordering::Acquire) != 0 {
        return Err(run.error());
    }
    match result {
        Ok(prepared) => {
            let status = run.wait()?;
            if !status.success() || run.reason.load(Ordering::Acquire) != 0 {
                return Err(run.error());
            }
            Ok(prepared)
        }
        Err(error) if error.starts_with("Worker") => Err(run.error()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn request(duration: u64) -> PrepareRequest {
        PrepareRequest {
            client_id: native_client_id(),
            access_token: "unit-test-token-not-an-account-credential".into(),
            track_uri: "spotify:track:4uLU6hMCjMI75M1A2tKUQC".into(),
            expected_duration_ms: Some(duration),
        }
    }
    #[test]
    fn native_identity_is_separate_and_arbitrary_web_identity_is_rejected() {
        assert_eq!(native_client_id(), SessionConfig::default().client_id);
        let mut web_grant = request(1000);
        web_grant.client_id = "0123456789abcdef0123456789abcdef".into();
        assert_ne!(web_grant.client_id, native_client_id());
        let error = validate_request(&web_grant).unwrap_err();
        assert!(error.contains("separate native login"));
        assert_eq!(web_grant.client_id, "0123456789abcdef0123456789abcdef");
        assert!(validate_request(&request(1000)).is_ok());
    }
    #[test]
    fn credential_free_probe_rejects_nonpublic_ids_and_sanitizes_errors() {
        for invalid in [
            "",
            "https://private.test/?access_token=secret",
            "GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG",
        ] {
            let diagnostic = probe_client_identity(invalid).unwrap_err();
            assert!(diagnostic.contains("public 32-character hexadecimal"));
            assert!(!diagnostic.contains("private.test"));
            assert!(!diagnostic.contains("access_token"));
        }
        assert!(validate_public_client_id(&native_client_id()).is_ok());
        let diagnostic = client_probe_error(librespot_core::Error::invalid_argument(
            "private-provider-response https://private.test/?access_token=secret",
        ));
        assert!(diagnostic.contains("InvalidArgument"));
        assert!(diagnostic.contains("no account sign-in"));
        assert!(!diagnostic.contains("private-provider-response"));
        assert!(!diagnostic.contains("access_token"));
        let http = librespot_core::Error::from(
            librespot_core::http_client::HttpClientError::StatusCode(400u16.try_into().unwrap()),
        );
        assert!(client_probe_error(http).contains("HTTP 400"));
    }
    fn info(duration: u64) -> SpotifyTrackMetadata {
        SpotifyTrackMetadata {
            uri: request(duration).track_uri.clone(),
            resolved_uri: None,
            title: "Fixture".into(),
            artists: vec!["Fixture artist".into()],
            album: "Test album".into(),
            duration_ms: duration,
        }
    }
    fn record_json(out: &mut Vec<u8>, kind: u8, value: &impl Serialize) {
        write_record(out, kind, &serde_json::to_vec(value).unwrap()).unwrap();
    }
    fn pcm(out: &mut Vec<u8>, frames: usize, left: f32, right: f32) {
        for n in (0..frames).step_by(MAX_PACKET_FRAMES) {
            let size = (frames - n).min(MAX_PACKET_FRAMES);
            let mut bytes = Vec::with_capacity(size * 8);
            for _ in 0..size {
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
            write_record(out, PCM, &bytes).unwrap();
        }
    }
    #[test]
    fn relink_wire_preserves_requested_identity_and_rejects_unrelated_identity() {
        let mut selected = info(1000);
        selected.resolved_uri = Some("spotify:track:67Hna13dNDkZvBpTXRIaOJ".into());
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &selected);
        pcm(&mut wire, SAMPLE_RATE as usize, 0.25, -0.25);
        record_json(
            &mut wire,
            COMPLETE,
            &Completion {
                frames: SAMPLE_RATE as u64,
                sample_rate: SAMPLE_RATE,
            },
        );
        let decoded = read_output(&mut Cursor::new(wire), &request(1000)).unwrap();
        assert_eq!(decoded.metadata.uri, request(1000).track_uri);
        assert_eq!(decoded.metadata.resolved_uri, selected.resolved_uri);
        selected.uri = "spotify:track:67Hna13dNDkZvBpTXRIaOJ".into();
        let mut wrong = MAGIC.to_vec();
        record_json(&mut wrong, METADATA, &selected);
        assert!(read_output(&mut Cursor::new(wrong), &request(1000)).is_err());
    }
    #[test]
    fn catalog_uri_validation_explains_truncated_ids_without_network_or_credentials() {
        let error = validate_track_uri("spotify:track:5u4hhtZ7f4rWkMZEZcTKr").unwrap_err();
        assert!(error.contains("22 ASCII"), "{error}");
        assert!(error.contains("received 21 characters"), "{error}");
        assert!(error.contains("complete URI"), "{error}");
        assert!(validate_track_uri("spotify:track:4uLU6hMCjMI75M1A2tKUQC").is_ok());
        for value in [
            "spotify:episode:4uLU6hMCjMI75M1A2tKUQC",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "spotify:track:4uLU6hMCjMI75M1A2tKUQ!",
            "spotify:track:4uLU6hMCjMI75M1A2tKUQC ",
        ] {
            assert!(validate_track_uri(value).is_err(), "{value}");
        }
        // Accepted identity parsing is shared with the worker request path.
        let mut valid = request(1000);
        valid.track_uri = "spotify:track:5u4hhtZ7f4rWkMZEZcTKr".into();
        assert_eq!(validate_request(&valid).unwrap_err(), error);
    }
    #[test]
    fn request_and_memory_bounds_are_enforced_before_network() {
        assert!(validate_request(&request(180_000)).is_ok());
        assert!(validate_duration(0).is_err());
        assert!(validate_duration(1_000_000).is_err());
        let mut invalid = request(1000);
        invalid.client_id = String::new();
        assert!(validate_request(&invalid).is_err());
        invalid.client_id = request(1000).client_id.clone();
        invalid.track_uri = "spotify:episode:4uLU6hMCjMI75M1A2tKUQC".into();
        assert!(validate_request(&invalid).is_err());
    }
    #[test]
    fn full_track_completion_returns_owned_stereo_audio() {
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &info(1000));
        pcm(&mut wire, SAMPLE_RATE as usize, 0.25, -0.25);
        record_json(
            &mut wire,
            COMPLETE,
            &Completion {
                frames: SAMPLE_RATE as u64,
                sample_rate: SAMPLE_RATE,
            },
        );
        let result = read_output(&mut Cursor::new(wire), &request(1000)).unwrap();
        assert_eq!(result.samples.len(), 44100);
        assert_eq!(result.sample_rate, 44100);
        assert_eq!(result.samples[99], [0.25, -0.25]);
        assert_eq!(result.metadata.title, "Fixture");
    }
    #[test]
    fn preview_or_partial_audio_never_becomes_a_native_track() {
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &info(180_000));
        pcm(&mut wire, 44100, 0.1, 0.1);
        record_json(
            &mut wire,
            COMPLETE,
            &Completion {
                frames: 44100,
                sample_rate: SAMPLE_RATE,
            },
        );
        assert!(read_output(&mut Cursor::new(wire), &request(180_000))
            .err()
            .unwrap()
            .contains("partial or preview"));
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &info(1000));
        pcm(&mut wire, 44100, 0.1, 0.1);
        assert!(read_output(&mut Cursor::new(wire), &request(1000)).is_err());
    }
    #[test]
    fn invalid_samples_order_size_and_completion_are_rejected() {
        let mut wire = MAGIC.to_vec();
        pcm(&mut wire, 1, 0., 0.);
        assert!(read_output(&mut Cursor::new(wire), &request(1000)).is_err());
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &info(1000));
        pcm(&mut wire, 1, f32::NAN, 0.);
        assert!(read_output(&mut Cursor::new(wire), &request(1000)).is_err());
        let mut wire = MAGIC.to_vec();
        wire.push(METADATA);
        wire.extend_from_slice(&(u32::MAX).to_le_bytes());
        assert!(read_output(&mut Cursor::new(wire), &request(1000)).is_err());
        let mut wire = MAGIC.to_vec();
        record_json(&mut wire, METADATA, &info(1000));
        pcm(&mut wire, 44100, 0., 0.);
        record_json(
            &mut wire,
            COMPLETE,
            &Completion {
                frames: 44100,
                sample_rate: 48000,
            },
        );
        assert!(read_output(&mut Cursor::new(wire), &request(1000)).is_err());
        assert!(validate_integrity(44100, &info(1000), Some(180_000)).is_err());
    }
    #[test]
    fn metadata_and_audio_results_do_not_contain_credentials() {
        let metadata = serde_json::to_string(&info(1000)).unwrap();
        assert!(!metadata.contains("access_token"));
        assert!(!metadata.contains("unit-test-token"));
        assert!(!metadata.contains("client_id"));
    }
    #[cfg(unix)]
    #[test]
    fn cancellation_kills_only_the_isolated_worker_and_reaps_it() {
        let child = Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = ChildRun::new(child, cancel.clone()).unwrap();
        let start = Instant::now();
        cancel.store(true, Ordering::Release);
        assert!(!run.wait().unwrap().success());
        assert_eq!(run.reason.load(Ordering::Acquire), 1);
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(run);
    }
    #[test]
    fn authentication_diagnostics_never_echo_unknown_upstream_messages() {
        let known =
            librespot_core::Error::permission_denied("Login failed with reason: Bad credentials");
        let known = authentication_error(known);
        assert!(known.contains("access-point token sign-in"));
        assert!(known.contains("token credentials rejected"));
        assert!(known.contains("separate native playback login"));
        assert!(!known.contains("Client ID and existing token"));
        let unknown = librespot_core::Error::permission_denied(
            "private-token-should-never-be-shown https://example.test/?access_token=secret",
        );
        let unknown = authentication_error(unknown);
        assert!(unknown.contains("Native Spotify session sign-in"));
        assert!(!unknown.contains("private-token"));
        assert!(!unknown.contains("https://"));
        assert!(!unknown.contains("access_token"));
        assert!(authentication_reason(
            "Login failed with reason: Bad credentials; access_token=secret"
        )
        .is_none());
    }
    #[test]
    fn metadata_stage_distinguishes_safe_http_status_and_identifier_errors() {
        use librespot_core::{http_client::HttpClientError, spotify_id::SpotifyIdError};
        let http =
            librespot_core::Error::from(HttpClientError::StatusCode(400u16.try_into().unwrap()));
        let diagnostic = upstream_stage_error("client-token acquisition", http);
        assert!(diagnostic.contains("client-token acquisition"));
        assert!(diagnostic.contains("HTTP 400"));
        assert!(diagnostic.contains("before audio decoding"));
        let id = librespot_core::Error::from(SpotifyIdError::InvalidId);
        let diagnostic = upstream_stage_error("full-track metadata retrieval", id);
        assert!(diagnostic.contains("invalid native metadata entity identifier"));
        let secret =
            librespot_core::Error::invalid_argument("https://private.test/?access_token=secret");
        let diagnostic = upstream_stage_error("native session token renewal (login5)", secret);
        assert!(diagnostic.contains("login5"));
        assert!(!diagnostic.contains("private.test"));
        assert!(!diagnostic.contains("access_token"));
    }
}
