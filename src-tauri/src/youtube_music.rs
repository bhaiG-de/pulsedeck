//! YouTube Music library import through the official YouTube Data API.
//! YouTube audio is never requested: an imported song plays through the local
//! file or Spotify catalog track that matches its title, artist and duration.
use crate::spotify::{
    challenge, now_seconds, random_secret, read_request, respond, validate_callback, CallbackError,
    LoginStart, SpotifyService,
};
use crate::spotify_vault::{Credential, Vault};
use reqwest::{blocking::Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use url::Url;

const SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API: &str = "https://www.googleapis.com/youtube/v3";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Liked videos; YouTube Music likes are kept in the same collection.
pub const LIKED: &str = "liked";
const MUSIC_CATEGORY: &str = "10";
const MAX_MATCH_BATCH: usize = 50;
/// Minimum score for a song to count as the same recording.
const ACCEPT: f64 = 0.72;
/// A local file at least this close is used without asking Spotify.
const LOCAL_FIRST: f64 = 0.85;

#[derive(Clone, Default, Deserialize, Serialize)]
struct Config {
    client_id: Option<String>,
    client_secret: Option<String>,
}
#[derive(Clone)]
struct Token {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
}
#[derive(Default)]
struct Inner {
    config: Config,
    token: Option<Token>,
    account: Option<String>,
    authorizing: bool,
    restoring: bool,
    last_error: Option<String>,
}

#[derive(Clone)]
pub struct YouTubeMusicService {
    inner: Arc<Mutex<Inner>>,
    operation: Arc<Mutex<()>>,
    generation: Arc<AtomicU64>,
    /// A newer match request stops an older one at the next song.
    match_generation: Arc<AtomicU64>,
    client: Client,
    config_path: PathBuf,
    vault: Arc<Vault>,
}
#[derive(Debug, Serialize)]
pub struct YouTubeMusicStatus {
    pub client_id: Option<String>,
    pub configured: bool,
    pub connected: bool,
    pub authorizing: bool,
    pub restoring: bool,
    pub account: Option<String>,
    pub last_error: Option<String>,
    /// Whether the Spotify library sign-in is available for matching.
    pub spotify_matching: bool,
}
#[derive(Debug, Serialize)]
pub struct YouTubePlaylist {
    pub id: String,
    pub name: String,
    pub total: Option<u64>,
    pub artwork_url: Option<String>,
    pub external_url: String,
}
#[derive(Debug, Serialize)]
pub struct YouTubeTrack {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub raw_title: String,
    pub channel: String,
    pub duration_ms: u64,
    pub artwork_url: Option<String>,
    pub external_url: String,
    /// YouTube's Music category, or an auto-generated artist "Topic" upload.
    pub music: bool,
    #[serde(rename = "match")]
    pub matched: Option<TrackMatch>,
    /// A lookup is cached, so `match: null` means nothing matched.
    pub match_checked: bool,
}
#[derive(Debug, Serialize)]
pub struct YouTubePage<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
    pub total: Option<u64>,
    /// Raw provider entries, including private and deleted videos.
    pub fetched_count: u64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TrackMatch {
    /// "local" or "spotify".
    pub provider: String,
    /// A local path, or a spotify:track URI.
    pub source: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: u64,
    pub artwork_url: Option<String>,
    pub confidence: f64,
}
#[derive(Debug, Serialize)]
pub struct MatchResult {
    pub video_id: String,
    #[serde(rename = "match")]
    pub matched: Option<TrackMatch>,
}
#[derive(Debug, Serialize)]
pub struct MatchReport {
    pub results: Vec<MatchResult>,
    pub cancelled: bool,
    /// The first provider failure; earlier results are still saved.
    pub error: Option<String>,
}

fn lock_error() -> String {
    "YouTube Music state is unavailable; restart the application".into()
}
fn valid_playlist_id(id: &str) -> bool {
    (2..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn validate_client(id: &str, secret: &str) -> Result<(String, String), String> {
    let (id, secret) = (id.trim(), secret.trim());
    let id_ok = id.len() <= 200
        && id
            .strip_suffix(".apps.googleusercontent.com")
            .is_some_and(|p| {
                !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
    if !id_ok {
        return Err("Enter the Client ID of your own Google Cloud Desktop app OAuth client; it ends with .apps.googleusercontent.com".into());
    }
    if secret.is_empty()
        || secret.len() > 128
        || !secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Enter the client secret shown with that Desktop app OAuth client".into());
    }
    Ok((id.into(), secret.into()))
}
fn credential_account(client_id: &str) -> String {
    format!("ytmusic:{client_id}")
}
// Optional provider settings must never prevent local decks from opening.
fn load_config(path: &Path) -> (Config, Option<String>) {
    let result = (|| -> Result<Config, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(_) => {
                return Err(
                    "Could not read YouTube Music settings; configure them again in Settings"
                        .into(),
                )
            }
        };
        let config: Config = serde_json::from_slice(&bytes).map_err(|_| {
            "YouTube Music settings are invalid; configure them again in Settings".to_string()
        })?;
        match (&config.client_id, &config.client_secret) {
            (None, None) => Ok(Config::default()),
            (Some(id), Some(secret)) => {
                let (id, secret) = validate_client(id, secret)?;
                Ok(Config {
                    client_id: Some(id),
                    client_secret: Some(secret),
                })
            }
            _ => Err(
                "YouTube Music settings are incomplete; configure them again in Settings".into(),
            ),
        }
    })();
    match result {
        Ok(config) => (config, None),
        Err(error) => (Config::default(), Some(error)),
    }
}
/// The settings file holds the Desktop client secret, so only its owner may read it.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|_| "Could not save YouTube Music settings".to_string())
}

impl YouTubeMusicService {
    pub fn new(config_path: PathBuf, vault: Arc<Vault>) -> Result<Self, String> {
        let (config, last_error) = load_config(&config_path);
        let restore_id = config.client_id.clone();
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("PulseDeck/0.1")
            .build()
            .map_err(|_| "Could not initialize the YouTube Music network client".to_string())?;
        let service = Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                restoring: restore_id.is_some(),
                last_error,
                ..Inner::default()
            })),
            operation: Arc::new(Mutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
            match_generation: Arc::new(AtomicU64::new(0)),
            client,
            config_path,
            vault,
        };
        if let Some(id) = restore_id {
            let restore = service.clone();
            // Keychain may prompt after a rebuild. Never block the app's main/UI thread.
            std::thread::Builder::new()
                .name("pulsedeck-ytmusic-restore".into())
                .spawn(move || {
                    let saved = restore.vault.get(&credential_account(&id));
                    if let Ok(mut inner) = restore.inner.lock() {
                        inner.restoring = false;
                        if restore.generation.load(Ordering::SeqCst) != 0
                            || inner.authorizing
                            || inner.token.is_some()
                        {
                            return;
                        }
                        match saved {
                            Ok(saved) => {
                                inner.token = saved.map(|saved| Token {
                                    access_token: String::new(),
                                    refresh_token: saved.refresh_token,
                                    expires_at: 0,
                                })
                            }
                            Err(_) => {
                                inner.last_error = Some("Saved YouTube Music sign-in is unavailable because Keychain access was not granted; sign in again".into())
                            }
                        }
                    }
                })
                .map_err(|_| "Could not start saved YouTube Music sign-in restore".to_string())?;
        }
        Ok(service)
    }

    pub fn status(&self, spotify_matching: bool) -> Result<YouTubeMusicStatus, String> {
        let inner = self.inner.lock().map_err(|_| lock_error())?;
        Ok(YouTubeMusicStatus {
            client_id: inner.config.client_id.clone(),
            configured: inner.config.client_id.is_some(),
            connected: inner.token.is_some(),
            authorizing: inner.authorizing,
            restoring: inner.restoring,
            account: inner.account.clone(),
            last_error: inner.last_error.clone(),
            spotify_matching,
        })
    }
    fn client_config(&self) -> Result<(String, String), String> {
        let inner = self.inner.lock().map_err(|_| lock_error())?;
        match (&inner.config.client_id, &inner.config.client_secret) {
            (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
            _ => Err("Save your Google OAuth Client ID and secret first".into()),
        }
    }

    pub fn configure(&self, client_id: &str, client_secret: &str) -> Result<(), String> {
        let (id, secret) = validate_client(client_id, client_secret)?;
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        let old = self.inner.lock().map_err(|_| lock_error())?.config.clone();
        if old.client_id.as_deref() == Some(&id) && old.client_secret.as_deref() == Some(&secret) {
            return Ok(());
        }
        let config = Config {
            client_id: Some(id),
            client_secret: Some(secret),
        };
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Could not create the YouTube Music settings directory".to_string())?;
        }
        // Never save a refresh or access token in the settings file.
        let temp = self.config_path.with_extension("json.tmp");
        write_private(
            &temp,
            &serde_json::to_vec_pretty(&config)
                .map_err(|_| "Could not encode YouTube Music settings".to_string())?,
        )?;
        if let Some(old) = old.client_id {
            self.vault.remove(&credential_account(&old))?;
        }
        std::fs::rename(temp, &self.config_path)
            .map_err(|_| "Could not replace YouTube Music settings".to_string())?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.match_generation.fetch_add(1, Ordering::SeqCst);
        *self.inner.lock().map_err(|_| lock_error())? = Inner {
            config,
            ..Inner::default()
        };
        Ok(())
    }

    pub fn disconnect(&self) -> Result<(), String> {
        // Cancellation precedes serialized network operations so a pending login cannot reconnect.
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.match_generation.fetch_add(1, Ordering::SeqCst);
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        let id = {
            let mut inner = self.inner.lock().map_err(|_| lock_error())?;
            inner.token = None;
            inner.account = None;
            inner.authorizing = false;
            inner.last_error = None;
            inner.config.client_id.clone()
        };
        if let Some(id) = id {
            self.vault.remove(&credential_account(&id))?;
        }
        Ok(())
    }

    fn cancel_login(&self, message: &str) -> Result<(), String> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut inner = self.inner.lock().map_err(|_| lock_error())?;
        inner.authorizing = false;
        inner.last_error = Some(message.into());
        Ok(())
    }

    pub fn begin_login(&self) -> Result<LoginStart, String> {
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        {
            let inner = self.inner.lock().map_err(|_| lock_error())?;
            if inner.restoring {
                return Err("Wait for saved YouTube Music sign-in to restore, or approve the Keychain prompt".into());
            }
            if inner.authorizing {
                return Err("YouTube Music sign-in is already waiting in your browser; finish it or disconnect to cancel".into());
            }
        }
        let (id, _) = self.client_config()?;
        // Google accepts any loopback port for Desktop app clients.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|_| "Could not open a local port for YouTube Music sign-in".to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Could not configure the YouTube Music sign-in callback".to_string())?;
        let port = listener.local_addr().map_err(|_| lock_error())?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}/ytmusic");
        let verifier = random_secret();
        let state = random_secret();
        let mut authorization = Url::parse(AUTHORIZE_URL).map_err(|_| lock_error())?;
        authorization.query_pairs_mut().extend_pairs([
            ("client_id", id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", SCOPE),
            ("state", state.as_str()),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge(&verifier).as_str()),
            // A refresh token keeps the sign-in across launches.
            ("access_type", "offline"),
            ("prompt", "consent"),
        ]);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut inner = self.inner.lock().map_err(|_| lock_error())?;
            inner.authorizing = true;
            inner.last_error = None;
        }
        let service = self.clone();
        let redirect = redirect_uri.clone();
        std::thread::Builder::new()
            .name("pulsedeck-ytmusic-auth".into())
            .spawn(move || {
                let result =
                    service.await_login(listener, &redirect, &state, &verifier, generation);
                if service.generation.load(Ordering::SeqCst) == generation {
                    if let Ok(mut inner) = service.inner.lock() {
                        inner.authorizing = false;
                        if let Err(error) = result {
                            inner.last_error = Some(error);
                        }
                    }
                }
            })
            .map_err(|_| {
                if let Ok(mut inner) = self.inner.lock() {
                    inner.authorizing = false;
                }
                "Could not start the YouTube Music sign-in worker".to_string()
            })?;
        Ok(LoginStart {
            authorization_url: authorization.to_string(),
            redirect_uri,
            expires_in_seconds: LOGIN_TIMEOUT.as_secs(),
        })
    }

    fn await_login(
        &self,
        listener: TcpListener,
        redirect: &str,
        state: &str,
        verifier: &str,
        generation: u64,
    ) -> Result<(), String> {
        let expected_host = redirect
            .strip_prefix("http://")
            .and_then(|s| s.split('/').next())
            .ok_or_else(lock_error)?;
        let deadline = Instant::now() + LOGIN_TIMEOUT;
        while Instant::now() < deadline {
            if self.generation.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            let (mut stream, peer) = match listener.accept() {
                Ok(accepted) => accepted,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(_) => return Err("YouTube Music sign-in callback could not be read".into()),
            };
            // BSD sockets inherit the listener's non-blocking mode.
            if !peer.ip().is_loopback() || stream.set_nonblocking(false).is_err() {
                continue;
            }
            let Ok(target) = read_request(&mut stream, expected_host) else {
                respond(&mut stream, 400, "Invalid callback request.");
                continue;
            };
            match validate_callback(&target, redirect, state) {
                Ok(Some(code)) => {
                    let result = self.finish_login(&code, redirect, verifier, generation);
                    if result.is_ok() {
                        respond(
                            &mut stream,
                            200,
                            "YouTube Music sign-in completed. Return to PulseDeck.",
                        );
                    } else {
                        respond(
                            &mut stream,
                            400,
                            "YouTube Music sign-in failed. Return to PulseDeck for details.",
                        );
                    }
                    return result;
                }
                Ok(None) => respond(&mut stream, 404, "Callback not found."),
                Err(CallbackError::Invalid) => {
                    respond(&mut stream, 400, "Invalid sign-in callback.")
                }
                Err(CallbackError::Denied) => {
                    respond(
                        &mut stream,
                        400,
                        "YouTube Music sign-in was declined. Return to PulseDeck.",
                    );
                    return Err("Google permission was declined; sign in again when ready".into());
                }
            }
        }
        Err("YouTube Music sign-in timed out after five minutes; sign in again".into())
    }

    fn finish_login(
        &self,
        code: &str,
        redirect: &str,
        verifier: &str,
        generation: u64,
    ) -> Result<(), String> {
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        if self.generation.load(Ordering::SeqCst) != generation {
            return Err("YouTube Music sign-in was cancelled".into());
        }
        let (id, secret) = self.client_config()?;
        let token = self.exchange(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", &id),
            ("client_secret", &secret),
            ("code_verifier", verifier),
        ])?;
        self.install_token(&id, token, None)?;
        if self.generation.load(Ordering::SeqCst) != generation {
            // Disconnect requested during the token request: erase only our new credential.
            self.vault.remove(&credential_account(&id))?;
            self.inner.lock().map_err(|_| lock_error())?.token = None;
            return Err("YouTube Music sign-in was cancelled".into());
        }
        // The channel name is informational; an account without one still signs in.
        if let Ok(value) = self.api_locked(
            "channels",
            &[("part", "snippet".into()), ("mine", "true".into())],
        ) {
            self.inner.lock().map_err(|_| lock_error())?.account = value["items"][0]["snippet"]
                ["title"]
                .as_str()
                .map(str::to_owned);
        }
        Ok(())
    }

    fn exchange(&self, form: &[(&str, &str)]) -> Result<TokenResponse, String> {
        let response = self.client.post(TOKEN_URL).form(form).send().map_err(|_| {
            "Google authorization request failed; check the network connection".to_string()
        })?;
        let status = response.status();
        if !status.is_success() {
            let body: Value = response.json().unwrap_or(Value::Null);
            return Err(token_error(status.as_u16(), body["error"].as_str()));
        }
        let token: TokenResponse = response
            .json()
            .map_err(|_| "Google returned an invalid token response".to_string())?;
        if token.access_token.is_empty()
            || !token.token_type.eq_ignore_ascii_case("bearer")
            || token.expires_in == 0
        {
            return Err("Google returned an unusable authorization token".into());
        }
        Ok(token)
    }
    fn install_token(
        &self,
        id: &str,
        response: TokenResponse,
        previous: Option<Token>,
    ) -> Result<(), String> {
        // Google returns a refresh token only with consent; refreshes keep the old one.
        let refresh_token = response
            .refresh_token
            .or_else(|| previous.map(|t| t.refresh_token))
            .filter(|s| !s.is_empty())
            .ok_or("Google did not provide a refresh token; sign in again")?;
        self.vault.set(
            &credential_account(id),
            Credential {
                refresh_token: refresh_token.clone(),
                scope: SCOPE.into(),
            },
        )?;
        let mut inner = self.inner.lock().map_err(|_| lock_error())?;
        inner.token = Some(Token {
            access_token: response.access_token,
            refresh_token,
            expires_at: now_seconds().saturating_add(response.expires_in),
        });
        inner.last_error = None;
        Ok(())
    }
    fn access_token(&self, force_refresh: bool) -> Result<String, String> {
        let token = self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .token
            .clone()
            .ok_or("Sign in to YouTube Music first")?;
        if !force_refresh
            && !token.access_token.is_empty()
            && token.expires_at > now_seconds().saturating_add(30)
        {
            return Ok(token.access_token);
        }
        let (id, secret) = self.client_config()?;
        let response = self.exchange(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &token.refresh_token),
            ("client_id", &id),
            ("client_secret", &secret),
        ]);
        match response {
            Ok(response) => self.install_token(&id, response, Some(token))?,
            Err(error) => {
                if error.starts_with("Google HTTP 400:") || error.starts_with("Google HTTP 401:") {
                    self.forget_invalid_session(&id, &error)?;
                } else {
                    // Keep the refresh token on transient network/server failures.
                    self.inner.lock().map_err(|_| lock_error())?.last_error = Some(error.clone());
                }
                return Err(error);
            }
        }
        self.inner
            .lock()
            .map_err(|_| lock_error())?
            .token
            .as_ref()
            .map(|t| t.access_token.clone())
            .ok_or_else(lock_error)
    }
    fn forget_invalid_session(&self, id: &str, error: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().map_err(|_| lock_error())?;
            inner.token = None;
            inner.account = None;
            inner.last_error = Some(error.into());
        }
        self.vault.remove(&credential_account(id))
    }
    fn api(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        self.api_locked(path, query)
    }
    fn api_locked(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let url = format!("{API}/{path}");
        for attempt in 0..2 {
            let token = self.access_token(attempt != 0)?;
            let response = self
                .client
                .get(&url)
                .bearer_auth(token)
                .query(query)
                .send()
                .map_err(|_| "YouTube request failed; check the network connection".to_string())?;
            let status = response.status();
            if status == StatusCode::UNAUTHORIZED && attempt == 0 {
                continue;
            }
            if !status.is_success() {
                let body: Value = response.json().unwrap_or(Value::Null);
                let error = api_error(
                    status.as_u16(),
                    body["error"]["errors"][0]["reason"]
                        .as_str()
                        .unwrap_or_default(),
                );
                if status == StatusCode::UNAUTHORIZED {
                    if let Ok((id, _)) = self.client_config() {
                        self.forget_invalid_session(&id, &error)?;
                    }
                }
                return Err(error);
            }
            return response
                .json()
                .map_err(|_| "YouTube returned an invalid API response".into());
        }
        Err("YouTube Music sign-in expired; sign in again".into())
    }

    pub fn playlists(
        &self,
        page_token: Option<&str>,
    ) -> Result<YouTubePage<YouTubePlaylist>, String> {
        let mut query = vec![
            ("part", "snippet,contentDetails".to_string()),
            ("mine", "true".into()),
            ("maxResults", "50".into()),
        ];
        query.extend(page_query(page_token)?);
        let raw = self.api("playlists", &query)?;
        let values = raw["items"]
            .as_array()
            .ok_or("YouTube did not return a playlist page")?;
        Ok(page(
            &raw,
            values.iter().filter_map(parse_playlist).collect(),
        ))
    }

    pub fn tracks(
        &self,
        playlist: &str,
        page_token: Option<&str>,
    ) -> Result<YouTubePage<YouTubeTrack>, String> {
        if playlist == LIKED {
            let mut query = vec![
                ("part", "snippet,contentDetails".to_string()),
                ("myRating", "like".into()),
                ("maxResults", "50".into()),
            ];
            query.extend(page_query(page_token)?);
            let raw = self.api("videos", &query)?;
            let values = raw["items"]
                .as_array()
                .ok_or("YouTube did not return a liked-video page")?;
            return Ok(page(&raw, values.iter().filter_map(parse_video).collect()));
        }
        if !valid_playlist_id(playlist) {
            return Err("Invalid YouTube playlist ID".into());
        }
        let mut query = vec![
            ("part", "contentDetails".to_string()),
            ("playlistId", playlist.into()),
            ("maxResults", "50".into()),
        ];
        query.extend(page_query(page_token)?);
        let raw = self.api("playlistItems", &query)?;
        let ids: Vec<String> = raw["items"]
            .as_array()
            .ok_or("YouTube did not return a playlist item page")?
            .iter()
            .filter_map(|item| item["contentDetails"]["videoId"].as_str())
            .filter(|id| valid_video_id(id))
            .map(str::to_owned)
            .collect();
        // Playlist items lack durations and categories; one videos call adds both.
        let mut videos: HashMap<String, Value> = HashMap::new();
        if !ids.is_empty() {
            let unique: Vec<&str> = ids
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            let details = self.api(
                "videos",
                &[
                    ("part", "snippet,contentDetails".into()),
                    ("id", unique.join(",")),
                ],
            )?;
            for video in details["items"].as_array().into_iter().flatten() {
                if let Some(id) = video["id"].as_str() {
                    videos.insert(id.into(), video.clone());
                }
            }
        }
        // Private and deleted videos have no details and are left out.
        let items = ids
            .iter()
            .filter_map(|id| videos.get(id).and_then(parse_video))
            .collect();
        Ok(page(&raw, items))
    }

    /// Song details for matching, in request order; unavailable videos are omitted.
    fn videos(&self, ids: &[String]) -> Result<Vec<YouTubeTrack>, String> {
        let raw = self.api(
            "videos",
            &[
                ("part", "snippet,contentDetails".into()),
                ("id", ids.join(",")),
            ],
        )?;
        let mut found: HashMap<String, YouTubeTrack> = raw["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(parse_video)
            .map(|t| (t.video_id.clone(), t))
            .collect();
        Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
    }
}

fn page_query(page_token: Option<&str>) -> Result<Vec<(&'static str, String)>, String> {
    match page_token {
        None | Some("") => Ok(vec![]),
        Some(token)
            if token.len() <= 256
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') =>
        {
            Ok(vec![("pageToken", token.into())])
        }
        Some(_) => Err("Invalid YouTube page token".into()),
    }
}
fn page<T>(raw: &Value, items: Vec<T>) -> YouTubePage<T> {
    YouTubePage {
        items,
        next_page_token: raw["nextPageToken"].as_str().map(str::to_owned),
        total: raw["pageInfo"]["totalResults"].as_u64(),
        fetched_count: raw["items"].as_array().map_or(0, |a| a.len()) as u64,
    }
}
fn token_error(status: u16, error: Option<&str>) -> String {
    let message = match error {
        Some("invalid_grant") => "YouTube Music sign-in expired or was revoked (Google ends Testing-mode sign-ins after 7 days); sign in again",
        Some("invalid_client" | "unauthorized_client") => "Google rejected the OAuth client; check the Client ID and secret of your Desktop app client",
        _ if status >= 500 => "Google sign-in is temporarily unavailable; try again later",
        _ => "Google could not complete the sign-in; check your OAuth client and sign in again",
    };
    format!("Google HTTP {status}: {message}")
}
fn api_error(status: u16, reason: &str) -> String {
    let message = match (status, reason) {
        (403, "quotaExceeded" | "dailyLimitExceeded" | "rateLimitExceeded") => "the YouTube Data API daily quota for your Google Cloud project is used up; try again tomorrow",
        (403, "accessNotConfigured") => "enable YouTube Data API v3 in your Google Cloud project, then retry",
        (403, _) => "Google denied access; check that your account is a test user of your OAuth app and that YouTube access was approved",
        (404, "playlistNotFound") => "this playlist is private or no longer exists",
        (404, "channelNotFound") => "this Google account has no YouTube channel, so it has no playlists to import",
        (401, _) => "YouTube Music sign-in expired or was revoked; sign in again",
        (429, _) => "YouTube is rate limiting requests; wait a moment and retry",
        (500..=599, _) => "YouTube is temporarily unavailable; try again later",
        _ => "YouTube could not complete this request",
    };
    format!("YouTube HTTP {status}: {message}")
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().trim().to_string()
}
fn thumbnail(snippet: &Value) -> Option<String> {
    ["medium", "high", "default"]
        .iter()
        .filter_map(|size| snippet["thumbnails"][size]["url"].as_str())
        .find(|url| Url::parse(url).is_ok_and(|u| u.scheme() == "https"))
        .map(str::to_owned)
}
fn parse_playlist(v: &Value) -> Option<YouTubePlaylist> {
    let id = v["id"]
        .as_str()
        .filter(|id| valid_playlist_id(id))?
        .to_string();
    Some(YouTubePlaylist {
        name: text(&v["snippet"], "title"),
        total: v["contentDetails"]["itemCount"].as_u64(),
        artwork_url: thumbnail(&v["snippet"]),
        external_url: format!("https://music.youtube.com/playlist?list={id}"),
        id,
    })
}
fn parse_video(v: &Value) -> Option<YouTubeTrack> {
    let id = v["id"]
        .as_str()
        .filter(|id| valid_video_id(id))?
        .to_string();
    let snippet = &v["snippet"];
    let raw_title = text(snippet, "title");
    let channel = text(snippet, "channelTitle");
    let (title, artists) = parse_song(&raw_title, &channel);
    Some(YouTubeTrack {
        music: snippet["categoryId"].as_str() == Some(MUSIC_CATEGORY)
            || channel.ends_with(" - Topic"),
        duration_ms: parse_duration(v["contentDetails"]["duration"].as_str().unwrap_or_default()),
        artwork_url: thumbnail(snippet),
        external_url: format!("https://music.youtube.com/watch?v={id}"),
        video_id: id,
        title,
        artists,
        raw_title,
        channel,
        matched: None,
        match_checked: false,
    })
}
/// ISO 8601 video durations such as PT1H2M3S, in milliseconds.
fn parse_duration(iso: &str) -> u64 {
    let Some(rest) = iso.strip_prefix('P') else {
        return 0;
    };
    let (mut seconds, mut number, mut time) = (0u64, 0u64, false);
    for c in rest.chars() {
        match c {
            '0'..='9' => {
                number = number
                    .saturating_mul(10)
                    .saturating_add(c as u64 - '0' as u64)
            }
            'T' => time = true,
            'D' if !time => seconds += number * 86_400,
            'H' if time => seconds += number * 3_600,
            'M' if time => seconds += number * 60,
            'S' if time => seconds += number,
            _ => return 0,
        }
        if !c.is_ascii_digit() {
            number = 0;
        }
    }
    seconds.saturating_mul(1000)
}

// Bracketed title notes that describe the upload rather than the recording.
const UPLOAD_NOTES: &[&str] = &[
    "official",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "visualizer",
    "visualiser",
    "mv",
    "hd",
    "hq",
    "4k",
    "explicit",
    "clean",
    "color coded",
];
/// Title words that name a different recording of the same song.
const VERSION_WORDS: &[&str] = &[
    "remix",
    "edit",
    "live",
    "acoustic",
    "instrumental",
    "extended",
    "radio",
    "vip",
    "dub",
    "rework",
    "bootleg",
    "flip",
    "cover",
    "karaoke",
    "sped",
    "slowed",
    "demo",
    "unplugged",
    "reprise",
    "mashup",
    "acapella",
];
const IGNORED_WORDS: &[&str] = &[
    "remaster",
    "remastered",
    "original",
    "mix",
    "version",
    "the",
    "a",
    "and",
];

/// Song title and artists from a YouTube upload title and its channel name.
fn parse_song(raw_title: &str, channel: &str) -> (String, Vec<String>) {
    let (title, mut featured) = strip_featuring(&strip_upload_notes(raw_title));
    let (title, artist) = if let Some(artist) = channel.strip_suffix(" - Topic") {
        (title, artist.to_string())
    } else if let Some((artist, song)) = split_dash(&title) {
        let (song, more) = strip_featuring(&song);
        featured.extend(more);
        (song, artist)
    } else {
        let artist = channel
            .strip_suffix("VEVO")
            .or_else(|| channel.strip_suffix(" Official"))
            .unwrap_or(channel);
        (title, artist.trim().to_string())
    };
    let mut artists = split_artists(&artist);
    for name in featured.iter().flat_map(|f| split_artists(f)) {
        if !artists.iter().any(|a| a.eq_ignore_ascii_case(&name)) {
            artists.push(name);
        }
    }
    (title, artists)
}
fn split_dash(title: &str) -> Option<(String, String)> {
    [" - ", " – ", " — "].iter().find_map(|dash| {
        let (left, right) = title.split_once(dash)?;
        (!left.trim().is_empty() && !right.trim().is_empty())
            .then(|| (left.trim().to_string(), right.trim().to_string()))
    })
}
fn strip_upload_notes(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest[start..].starts_with('(') {
            ')'
        } else {
            ']'
        };
        let Some(length) = rest[start..].find(close) else {
            break;
        };
        let note = rest[start + 1..start + length].to_lowercase();
        out.push_str(&rest[..start]);
        if !UPLOAD_NOTES.iter().any(|word| has_word(&note, word)) {
            out.push_str(&rest[start..=start + length]);
        }
        rest = &rest[start + length + 1..];
    }
    out.push_str(rest);
    // "Song | Official Video" style suffixes.
    if let Some((song, note)) = out.clone().split_once(" | ") {
        if UPLOAD_NOTES
            .iter()
            .any(|word| has_word(&note.to_lowercase(), word))
        {
            out = song.to_string();
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn has_word(text: &str, word: &str) -> bool {
    let normalized = format!(" {} ", normalize(text));
    normalized.contains(&format!(" {} ", normalize(word)))
}
/// Removes "(feat. X)", "[ft. X]" and trailing "feat. X", returning the featured names.
fn strip_featuring(title: &str) -> (String, Vec<String>) {
    // ASCII lowercasing keeps byte offsets valid for slicing the original.
    let lower = title.to_ascii_lowercase();
    let markers = ["feat. ", "feat ", "ft. ", "ft ", "featuring "];
    let found = markers
        .iter()
        .filter_map(|m| {
            lower
                .match_indices(m)
                .find(|(i, _)| *i == 0 || matches!(lower.as_bytes()[i - 1], b' ' | b'(' | b'['))
                .map(|(i, _)| (i, m.len()))
        })
        .min_by_key(|(i, _)| *i);
    let Some((index, length)) = found else {
        return (title.trim().to_string(), vec![]);
    };
    let opened = index > 0 && matches!(title.as_bytes()[index - 1], b'(' | b'[');
    let start = if opened { index - 1 } else { index };
    let after = &title[index + length..];
    let (name, rest) = if opened {
        let close = if title.as_bytes()[index - 1] == b'(' {
            ')'
        } else {
            ']'
        };
        match after.find(close) {
            Some(end) => (&after[..end], &after[end + 1..]),
            None => (after, ""),
        }
    } else {
        // An unbracketed credit runs to the next bracket.
        match after.find(['(', '[']) {
            Some(end) => (&after[..end], &after[end..]),
            None => (after, ""),
        }
    };
    let cleaned = format!("{} {}", &title[..start], rest);
    (
        cleaned.split_whitespace().collect::<Vec<_>>().join(" "),
        vec![name.trim().to_string()]
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect(),
    )
}
fn split_artists(names: &str) -> Vec<String> {
    let mut parts = vec![names.to_string()];
    for separator in [
        ", ",
        " & ",
        " x ",
        " X ",
        " and ",
        " feat. ",
        " ft. ",
        " featuring ",
    ] {
        parts = parts
            .iter()
            .flat_map(|p| p.split(separator).map(str::to_owned).collect::<Vec<_>>())
            .collect();
    }
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn title_words(title: &str) -> Vec<String> {
    normalize(&strip_featuring(title).0)
        .split(' ')
        .filter(|w| !w.is_empty() && !IGNORED_WORDS.contains(w))
        .filter(|w| !(w.len() == 4 && w.parse::<u32>().is_ok_and(|y| (1900..2100).contains(&y))))
        .map(str::to_owned)
        .collect()
}
fn overlap(a: &[String], b: &[String]) -> f64 {
    let (a, b): (HashSet<_>, HashSet<_>) = (a.iter().collect(), b.iter().collect());
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    a.intersection(&b).count() as f64 / a.len().max(b.len()) as f64
}

/// How likely a candidate is the same recording, from 0 to 1.
fn score(song: &YouTubeTrack, title: &str, artists: &[String], duration_ms: u64) -> f64 {
    let (ours, theirs) = (title_words(&song.title), title_words(title));
    let title_score = overlap(&ours, &theirs);
    let versions = |words: &[String]| -> HashSet<String> {
        words
            .iter()
            .filter(|w| VERSION_WORDS.contains(&w.as_str()))
            .cloned()
            .collect()
    };
    // A remix or live take is a different track to mix, even with the same name.
    let version_penalty = if versions(&ours) == versions(&theirs) {
        0.
    } else {
        0.25
    };
    let names = |list: &[String]| {
        list.iter()
            .map(|a| normalize(a))
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>()
    };
    let (our_artists, their_artists) = (names(&song.artists), names(artists));
    let artist_score = if our_artists.is_empty() || their_artists.is_empty() {
        0.5
    } else if our_artists.iter().any(|a| their_artists.contains(a)) {
        1.
    } else {
        let words = |list: &[String]| {
            list.iter()
                .flat_map(|a| a.split(' ').map(str::to_owned))
                .filter(|w| w != "the")
                .collect::<Vec<_>>()
        };
        overlap(&words(&our_artists), &words(&their_artists))
    };
    let timing = if song.duration_ms == 0 || duration_ms == 0 {
        0.85
    } else {
        match song.duration_ms.abs_diff(duration_ms) / 1000 {
            0..=3 => 1.,
            4..=8 => 0.9,
            9..=20 => 0.7,
            _ => 0.4,
        }
    };
    if title_score < 0.6 {
        return title_score * 0.5;
    }
    ((0.6 * title_score + 0.4 * artist_score) * timing - version_penalty).clamp(0., 1.)
}
fn round(value: f64) -> f64 {
    (value * 100.).round() / 100.
}

/// The closest local file, from (path, title, artist, seconds) rows.
fn best_local(
    song: &YouTubeTrack,
    candidates: &[(String, String, String, f64)],
) -> Option<TrackMatch> {
    let words: HashSet<String> = title_words(&song.title).into_iter().collect();
    candidates
        .iter()
        .filter_map(|(path, title, artist, seconds)| {
            // Untagged files are often named "Artist - Title".
            let (title, artist) = match (artist.trim().is_empty(), split_dash(title)) {
                (true, Some((artist, title))) => (title, artist),
                _ => (title.clone(), artist.clone()),
            };
            if !title_words(&title).iter().any(|w| words.contains(w)) {
                return None;
            }
            let artists = split_artists(&artist);
            let duration_ms = (seconds.max(0.) * 1000.).round() as u64;
            let confidence = score(song, &title, &artists, duration_ms);
            Some(TrackMatch {
                provider: "local".into(),
                source: path.clone(),
                title,
                artists,
                duration_ms,
                artwork_url: None,
                confidence: round(confidence),
            })
        })
        .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
}
fn spotify_query_text(text: &str) -> String {
    text.chars()
        .map(|c| if matches!(c, ':' | '"') { ' ' } else { c })
        .collect()
}
/// Best Spotify catalog candidate: a field search first, then a looser one.
fn best_spotify(
    spotify: &SpotifyService,
    song: &YouTubeTrack,
) -> Result<Option<TrackMatch>, String> {
    let title = spotify_query_text(&song.title);
    let artist = song
        .artists
        .first()
        .map(|a| spotify_query_text(a))
        .unwrap_or_default();
    let mut queries = vec![];
    if !artist.is_empty() {
        queries.push(format!("track:{title} artist:{artist}"));
    }
    queries.push(format!("{title} {artist}").trim().to_string());
    let mut best: Option<TrackMatch> = None;
    for query in queries {
        for track in spotify.search_tracks(&query, 5)? {
            if !track.available || track.is_local {
                continue;
            }
            let confidence = round(score(song, &track.title, &track.artists, track.duration_ms));
            if best.as_ref().is_none_or(|b| confidence > b.confidence) {
                best = Some(TrackMatch {
                    provider: "spotify".into(),
                    source: track.uri,
                    title: track.title,
                    artists: track.artists,
                    duration_ms: track.duration_ms,
                    artwork_url: track.artwork_url,
                    confidence,
                });
            }
        }
        if best.as_ref().is_some_and(|b| b.confidence >= ACCEPT) {
            break;
        }
    }
    Ok(best.filter(|b| b.confidence >= ACCEPT))
}
/// Whether a cached lookup can be shown without searching again.
fn cached_match(
    saved: &(Option<String>, bool),
    spotify_connected: bool,
    local_exists: impl Fn(&str) -> bool,
) -> Option<Option<TrackMatch>> {
    match &saved.0 {
        // A miss found before Spotify was connected deserves a catalog search now.
        None => (saved.1 || !spotify_connected).then_some(None),
        Some(json) => {
            let found: TrackMatch = serde_json::from_str(json).ok()?;
            (found.provider != "local" || local_exists(&found.source)).then_some(Some(found))
        }
    }
}
fn attach_cached(
    library: &crate::library::Library,
    tracks: &mut [YouTubeTrack],
    spotify_connected: bool,
) -> Result<(), String> {
    let ids: Vec<String> = tracks.iter().map(|t| t.video_id.clone()).collect();
    let saved = library.ytmusic_matches(&ids)?;
    for track in tracks {
        if let Some(found) = saved.get(&track.video_id).and_then(|saved| {
            cached_match(saved, spotify_connected, |path| {
                library.has_track_path(path).unwrap_or(false)
            })
        }) {
            track.matched = found;
            track.match_checked = true;
        }
    }
    Ok(())
}

async fn background<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|_| "YouTube Music background task failed".to_string())?
}
fn services(app: &tauri::AppHandle) -> (YouTubeMusicService, SpotifyService) {
    use tauri::Manager;
    (
        app.state::<YouTubeMusicService>().inner().clone(),
        app.state::<SpotifyService>().inner().clone(),
    )
}
fn spotify_connected(spotify: &SpotifyService) -> bool {
    spotify.status().is_ok_and(|s| s.connected)
}
pub async fn ytmusic_status(app: tauri::AppHandle) -> Result<YouTubeMusicStatus, String> {
    let (service, spotify) = services(&app);
    service.status(spotify_connected(&spotify))
}
pub async fn ytmusic_configure(
    app: tauri::AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<YouTubeMusicStatus, String> {
    let (service, spotify) = services(&app);
    background(move || {
        service.configure(&client_id, &client_secret)?;
        service.status(spotify_connected(&spotify))
    })
    .await
}
pub async fn ytmusic_begin_login(app: tauri::AppHandle) -> Result<LoginStart, String> {
    let (service, _) = services(&app);
    background(move || {
        let start = service.begin_login()?;
        #[cfg(target_os = "macos")]
        {
            if open::that(&start.authorization_url).is_err() {
                let message = "Could not open the default browser; retry YouTube Music sign-in";
                service.cancel_login(message)?;
                return Err(message.into());
            }
        }
        Ok(start)
    })
    .await
}
pub async fn ytmusic_disconnect(app: tauri::AppHandle) -> Result<YouTubeMusicStatus, String> {
    let (service, spotify) = services(&app);
    background(move || {
        service.disconnect()?;
        service.status(spotify_connected(&spotify))
    })
    .await
}
pub async fn ytmusic_playlists(
    app: tauri::AppHandle,
    page_token: Option<String>,
) -> Result<YouTubePage<YouTubePlaylist>, String> {
    let (service, _) = services(&app);
    background(move || service.playlists(page_token.as_deref())).await
}
pub async fn ytmusic_tracks(
    app: tauri::AppHandle,
    playlist_id: String,
    page_token: Option<String>,
) -> Result<YouTubePage<YouTubeTrack>, String> {
    let (service, spotify) = services(&app);
    background(move || {
        use tauri::Manager;
        let mut page = service.tracks(&playlist_id, page_token.as_deref())?;
        let state = app.state::<crate::AppState>();
        let library = state.library.lock().map_err(|_| crate::poisoned())?;
        attach_cached(&library, &mut page.items, spotify_connected(&spotify))?;
        Ok(page)
    })
    .await
}
/// Match songs to local files first, then the Spotify catalog. Every lookup is
/// saved as it completes, so a cancelled or rate-limited batch keeps its progress.
pub async fn ytmusic_match(
    app: tauri::AppHandle,
    video_ids: Vec<String>,
    force: bool,
) -> Result<MatchReport, String> {
    let mut ids = Vec::new();
    for id in video_ids {
        if !valid_video_id(&id) {
            return Err(format!("Invalid YouTube video ID: {id}"));
        }
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.len() > MAX_MATCH_BATCH {
        return Err(format!("Match at most {MAX_MATCH_BATCH} songs per request"));
    }
    let (service, spotify) = services(&app);
    let generation = service.match_generation.fetch_add(1, Ordering::SeqCst) + 1;
    background(move || {
        use tauri::Manager;
        let state = app.state::<crate::AppState>();
        let spotify_connected = spotify_connected(&spotify);
        let mut report = MatchReport {
            results: vec![],
            cancelled: false,
            error: None,
        };
        let (pending, candidates) = {
            let library = state.library.lock().map_err(|_| crate::poisoned())?;
            let saved = library.ytmusic_matches(&ids)?;
            let mut pending = vec![];
            for id in &ids {
                let cached = (!force).then(|| saved.get(id)).flatten().and_then(|saved| {
                    cached_match(saved, spotify_connected, |path| {
                        library.has_track_path(path).unwrap_or(false)
                    })
                });
                match cached {
                    Some(matched) => report.results.push(MatchResult {
                        video_id: id.clone(),
                        matched,
                    }),
                    None => pending.push(id.clone()),
                }
            }
            if pending.is_empty() {
                return Ok(report);
            }
            (pending, library.match_candidates()?)
        };
        let songs = service.videos(&pending)?;
        for id in pending
            .iter()
            .filter(|id| !songs.iter().any(|s| &s.video_id == *id))
        {
            // Private or deleted: nothing to match, now or later.
            state
                .library
                .lock()
                .map_err(|_| crate::poisoned())?
                .save_ytmusic_match(id, None, true)?;
            report.results.push(MatchResult {
                video_id: id.clone(),
                matched: None,
            });
        }
        for song in songs {
            if service.match_generation.load(Ordering::SeqCst) != generation {
                report.cancelled = true;
                break;
            }
            let mut best = best_local(&song, &candidates).filter(|m| m.confidence >= ACCEPT);
            let mut searched = false;
            if spotify_connected && best.as_ref().is_none_or(|m| m.confidence < LOCAL_FIRST) {
                match best_spotify(&spotify, &song) {
                    Ok(found) => {
                        searched = true;
                        if let Some(found) = found
                            .filter(|f| best.as_ref().is_none_or(|b| f.confidence > b.confidence))
                        {
                            best = Some(found);
                        }
                    }
                    Err(error) => {
                        report.error = Some(error);
                        break;
                    }
                }
            }
            let json = best
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| e.to_string())?;
            state
                .library
                .lock()
                .map_err(|_| crate::poisoned())?
                .save_ytmusic_match(&song.video_id, json.as_deref(), searched)?;
            report.results.push(MatchResult {
                video_id: song.video_id,
                matched: best,
            });
        }
        Ok(report)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artists: &[&str], seconds: u64) -> YouTubeTrack {
        YouTubeTrack {
            video_id: "dQw4w9WgXcQ".into(),
            title: title.into(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            raw_title: title.into(),
            channel: String::new(),
            duration_ms: seconds * 1000,
            artwork_url: None,
            external_url: String::new(),
            music: true,
            matched: None,
            match_checked: false,
        }
    }
    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn upload_titles_and_channels_become_song_title_and_artists() {
        let cases = [
            ("Strobe", "deadmau5 - Topic", "Strobe", vec!["deadmau5"]),
            (
                "Daft Punk - Get Lucky (Official Video) ft. Pharrell Williams, Nile Rodgers",
                "DaftPunkVEVO",
                "Get Lucky",
                vec!["Daft Punk", "Pharrell Williams", "Nile Rodgers"],
            ),
            (
                "Fred again.. – Delilah (pull me out of this) [Official Audio]",
                "Fred again..",
                "Delilah (pull me out of this)",
                vec!["Fred again.."],
            ),
            (
                "Losing It (Extended Mix)",
                "FISHER",
                "Losing It (Extended Mix)",
                vec!["FISHER"],
            ),
            (
                "Rhythm Is A Dancer | Official Music Video",
                "SNAPVEVO",
                "Rhythm Is A Dancer",
                vec!["SNAP"],
            ),
            (
                "One More Time (feat. Romanthony) [Remastered]",
                "Daft Punk - Topic",
                "One More Time [Remastered]",
                vec!["Daft Punk", "Romanthony"],
            ),
            (
                "Ferrari (Lyrics)",
                "James Hype Official",
                "Ferrari",
                vec!["James Hype"],
            ),
        ];
        for (raw, channel, title, artists) in cases {
            assert_eq!(
                parse_song(raw, channel),
                (title.to_string(), names(&artists)),
                "{raw}"
            );
        }
    }

    #[test]
    fn iso_durations_cover_hours_days_and_reject_garbage() {
        assert_eq!(parse_duration("PT3M45S"), 225_000);
        assert_eq!(parse_duration("PT1H2M3S"), 3_723_000);
        assert_eq!(parse_duration("PT59S"), 59_000);
        assert_eq!(parse_duration("P1DT1S"), 86_401_000);
        assert_eq!(parse_duration("P0D"), 0);
        assert_eq!(parse_duration("3:45"), 0);
        assert_eq!(parse_duration("PT3X"), 0);
    }

    #[test]
    fn scoring_accepts_the_same_recording_and_rejects_versions_covers_and_other_lengths() {
        let original = song("Get Lucky", &["Daft Punk", "Pharrell Williams"], 248);
        let same = score(
            &original,
            "Get Lucky (feat. Pharrell Williams & Nile Rodgers)",
            &names(&["Daft Punk", "Pharrell Williams", "Nile Rodgers"]),
            248_000,
        );
        assert!(same >= 0.95, "{same}");
        let remaster = score(
            &song("Africa", &["TOTO"], 295),
            "Africa - 2018 Remaster",
            &names(&["TOTO"]),
            296_000,
        );
        assert!(remaster >= ACCEPT, "{remaster}");
        let original_mix = score(
            &song("Opus (Original Mix)", &["Eric Prydz"], 543),
            "Opus",
            &names(&["Eric Prydz"]),
            542_000,
        );
        assert!(original_mix >= ACCEPT, "{original_mix}");
        let remix = score(
            &song("Opus", &["Eric Prydz"], 543),
            "Opus - Four Tet Remix",
            &names(&["Eric Prydz", "Four Tet"]),
            543_000,
        );
        assert!(remix < ACCEPT, "{remix}");
        let cover = score(
            &song("Hallelujah", &["Leonard Cohen"], 279),
            "Hallelujah",
            &names(&["Jeff Buckley"]),
            413_000,
        );
        assert!(cover < ACCEPT, "{cover}");
        let radio_vs_extended = score(
            &song("Losing It", &["FISHER"], 250),
            "Losing It (Extended Mix)",
            &names(&["FISHER"]),
            391_000,
        );
        assert!(radio_vs_extended < ACCEPT, "{radio_vs_extended}");
        let sequel = score(
            &song("Levels", &["Avicii"], 200),
            "Levels Part 2",
            &names(&["Avicii"]),
            200_000,
        );
        assert!(sequel < ACCEPT, "{sequel}");
    }

    #[test]
    fn local_match_reads_untagged_file_names_and_prefers_the_closest_length() {
        let candidates = vec![
            (
                "/music/strobe-edit.mp3".into(),
                "Strobe (Radio Edit)".into(),
                "deadmau5".into(),
                210.,
            ),
            (
                "/music/strobe.mp3".into(),
                "deadmau5 - Strobe".into(),
                String::new(),
                637.,
            ),
            (
                "/music/other.mp3".into(),
                "Ghosts n Stuff".into(),
                "deadmau5".into(),
                300.,
            ),
        ];
        let found = best_local(&song("Strobe", &["deadmau5"], 637), &candidates).unwrap();
        assert_eq!(found.source, "/music/strobe.mp3");
        assert_eq!(found.provider, "local");
        assert!(found.confidence >= LOCAL_FIRST);
        assert!(best_local(&song("Unrelated", &["Nobody"], 100), &candidates).is_none());
    }

    #[test]
    fn cached_lookups_retry_misses_once_spotify_connects_and_drop_removed_files() {
        let local = TrackMatch {
            provider: "local".into(),
            source: "/music/a.flac".into(),
            title: "A".into(),
            artists: vec![],
            duration_ms: 1,
            artwork_url: None,
            confidence: 1.,
        };
        let json = Some(serde_json::to_string(&local).unwrap());
        assert_eq!(
            cached_match(&(json.clone(), false), true, |_| true),
            Some(Some(local))
        );
        assert_eq!(cached_match(&(json, false), true, |_| false), None);
        assert_eq!(cached_match(&(None, false), false, |_| true), Some(None));
        assert_eq!(cached_match(&(None, false), true, |_| true), None);
        assert_eq!(cached_match(&(None, true), true, |_| true), Some(None));
        assert_eq!(
            cached_match(&(Some("{broken".into()), true), true, |_| true),
            None
        );
    }

    #[test]
    fn client_and_id_validation_reject_spotify_ids_and_unsafe_tokens() {
        assert!(validate_client("123-abc.apps.googleusercontent.com", "GOCSPX-secret_1").is_ok());
        assert!(validate_client("00000000000000000000000000000000", "GOCSPX-secret").is_err());
        assert!(validate_client(".apps.googleusercontent.com", "GOCSPX-secret").is_err());
        assert!(validate_client("123-abc.apps.googleusercontent.com", "").is_err());
        assert!(validate_client("123-abc.apps.googleusercontent.com", "bad secret").is_err());
        assert!(valid_video_id("dQw4w9WgXcQ") && !valid_video_id("dQw4w9WgXc&"));
        assert!(
            valid_playlist_id("PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf")
                && !valid_playlist_id("PL/../x")
        );
        assert!(page_query(Some("CDIQAA")).is_ok() && page_query(Some("a&b=c")).is_err());
    }

    #[test]
    fn settings_hold_no_tokens_are_private_and_invalid_files_are_preserved() {
        let root = std::env::temp_dir().join(format!("pd-ytmusic-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("youtube-music.json");
        for raw in ["{broken", r#"{"client_id":"x.apps.googleusercontent.com"}"#] {
            std::fs::write(&path, raw).unwrap();
            let (config, error) = load_config(&path);
            assert!(config.client_id.is_none() && error.is_some());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
        }
        std::fs::remove_file(&path).unwrap();
        let service = YouTubeMusicService::new(path.clone(), Arc::new(Vault::new(vec![]))).unwrap();
        service
            .configure("123-abc.apps.googleusercontent.com", "GOCSPX-secret")
            .unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("GOCSPX-secret") && !saved.contains("token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let status = service.status(false).unwrap();
        assert!(status.configured && !status.connected);
        assert!(service.tracks(LIKED, None).unwrap_err().contains("Sign in"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn match_cache_roundtrips_and_local_candidates_exclude_spotify() {
        let root = std::env::temp_dir().join(format!("pd-ytmusic-db-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let library = crate::library::Library::open(&root.join("library.sqlite3")).unwrap();
        library
            .save_ytmusic_match("dQw4w9WgXcQ", Some("{}"), false)
            .unwrap();
        library
            .save_ytmusic_match("dQw4w9WgXcQ", None, true)
            .unwrap();
        let saved = library
            .ytmusic_matches(&["dQw4w9WgXcQ".into(), "missing0000".into()])
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved["dQw4w9WgXcQ"], (None, true));
        assert!(library.match_candidates().unwrap().is_empty());
        assert!(!library.has_track_path("/nowhere.mp3").unwrap());
        drop(library);
        std::fs::remove_dir_all(root).unwrap();
    }
}
