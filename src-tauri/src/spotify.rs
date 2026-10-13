//! Own-client OAuth, library browsing and Spotify Connect controls.
//! Native playback credentials remain private to the experimental worker.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use reqwest::{blocking::Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

const CALLBACK_PORT: u16 = 8765;
const NATIVE_CALLBACK_PORT: u16 = 8898;
const NATIVE_REDIRECT: &str = "http://127.0.0.1:8898/login";
const REGISTERED_REDIRECT: &str = "http://127.0.0.1:8765/callback";
const SCOPES: &str = "playlist-read-private playlist-read-collaborative user-library-read user-read-playback-state user-modify-playback-state";
const PLAYBACK_SCOPES: &str = "playlist-read-private playlist-read-collaborative user-library-read user-read-playback-state user-modify-playback-state streaming";
const PAGE_SIZE: u64 = 50;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Default, Deserialize, Serialize)]
struct Config {
    client_id: Option<String>,
}
#[derive(Clone)]
struct Token {
    access_token: String,
    refresh_token: String,
    scope: String,
    expires_at: u64,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: Option<String>,
}
struct LoginContext {
    generation: u64,
    requested_scopes: String,
}
#[derive(Default)]
struct Inner {
    config: Config,
    token: Option<Token>,
    account: Option<Account>,
    authorizing: bool,
    restoring: bool,
    last_error: Option<String>,
    rate_limited_until: Option<Instant>,
}

#[derive(Clone)]
pub struct SpotifyService {
    inner: Arc<Mutex<Inner>>,
    operation: Arc<Mutex<()>>,
    generation: Arc<AtomicU64>,
    client: Client,
    config_path: PathBuf,
    native_playback: bool,
    vault: Arc<crate::spotify_vault::Vault>,
}
pub struct NativeSpotifyService(pub SpotifyService);
#[derive(Clone, Debug, Serialize)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub external_url: String,
}
#[derive(Debug, Serialize)]
pub struct SpotifyStatus {
    pub client_id: Option<String>,
    pub configured: bool,
    pub connected: bool,
    pub authorizing: bool,
    pub restoring: bool,
    pub account: Option<Account>,
    pub last_error: Option<String>,
    pub registered_redirect_uri: String,
    pub playback_mode: String,
    pub dj_streaming_available: bool,
    pub playback_permission: bool,
    pub setup_notice: String,
}
#[derive(Debug, Serialize)]
pub struct LoginStart {
    pub authorization_url: String,
    pub redirect_uri: String,
    pub expires_in_seconds: u64,
}
#[derive(Debug, Serialize)]
pub struct SpotifyTrack {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: Option<String>,
    pub duration_ms: u64,
    pub artwork_url: Option<String>,
    pub external_url: String,
    pub is_local: bool,
    pub available: bool,
    pub playable_in_deck: bool,
}
#[derive(Debug, Serialize)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub total: Option<u64>,
    pub external_url: String,
    pub artwork_url: Option<String>,
    pub owner_id: String,
    pub collaborative: bool,
}
#[derive(Debug, Serialize)]
pub struct SpotifyPage<T> {
    pub items: Vec<T>,
    pub offset: u64,
    pub total: u64,
    pub next_offset: Option<u64>,
}
#[derive(Debug, Serialize)]
pub struct SpotifyDevice {
    pub id: Option<String>,
    pub name: String,
    pub device_type: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub volume_percent: Option<u64>,
}

fn lock_error() -> String {
    "Spotify state is unavailable; restart the application".into()
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn validate_client_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Enter the 32-character Client ID from your own Spotify developer application (no client secret)".into());
    }
    Ok(id.to_string())
}
impl SpotifyService {
    #[cfg(test)]
    pub fn new(config_path: PathBuf) -> Result<Self, String> {
        Self::new_mode(config_path, false, None)
    }
    pub fn new_pair(
        config_path: PathBuf,
        native_path: PathBuf,
    ) -> Result<(Self, NativeSpotifyService), String> {
        let library_id = match std::fs::read(&config_path) {
            Ok(raw) => {
                serde_json::from_slice::<Config>(&raw)
                    .map_err(|_| "Spotify settings are invalid")?
                    .client_id
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("Could not read Spotify settings".into()),
        };
        let mut legacy = Vec::new();
        if let Some(id) = library_id {
            validate_client_id(&id)?;
            legacy.push(id);
        }
        legacy.push(format!("native:{}", spotify_playback::native_client_id()));
        let vault = Arc::new(crate::spotify_vault::Vault::new(legacy));
        Ok((
            Self::new_mode(config_path, false, Some(vault.clone()))?,
            NativeSpotifyService(Self::new_mode(native_path, true, Some(vault))?),
        ))
    }
    fn credential_account(&self, id: &str) -> String {
        if self.native_playback {
            format!("native:{id}")
        } else {
            id.into()
        }
    }
    fn new_mode(
        config_path: PathBuf,
        native_playback: bool,
        vault: Option<Arc<crate::spotify_vault::Vault>>,
    ) -> Result<Self, String> {
        let config = if native_playback {
            Config {
                client_id: Some(spotify_playback::native_client_id()),
            }
        } else {
            match std::fs::read(&config_path) {
                Ok(bytes) => serde_json::from_slice::<Config>(&bytes)
                    .map_err(|_| "Spotify settings are invalid".to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
                Err(_) => return Err("Could not read Spotify settings".into()),
            }
        };
        if let Some(id) = &config.client_id {
            validate_client_id(id)?;
        }
        let restore_id = config.client_id.clone();
        let vault = vault.unwrap_or_else(|| {
            Arc::new(crate::spotify_vault::Vault::new(
                restore_id
                    .iter()
                    .map(|id| {
                        if native_playback {
                            format!("native:{id}")
                        } else {
                            id.clone()
                        }
                    })
                    .collect(),
            ))
        });
        let inner = Inner {
            config,
            restoring: restore_id.is_some(),
            ..Inner::default()
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("PulseDeck/0.1")
            .build()
            .map_err(|_| "Could not initialize Spotify network client".to_string())?;
        let service = Self {
            inner: Arc::new(Mutex::new(inner)),
            operation: Arc::new(Mutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
            client,
            config_path,
            native_playback,
            vault,
        };
        if let Some(id) = restore_id {
            let restore = service.clone();
            // Keychain may prompt after a rebuild. Never block the app's main/UI thread.
            std::thread::Builder::new()
                .name("pulsedeck-keychain-restore".into())
                .spawn(move || {
                    let result = (|| {
                        let saved = restore.vault.get(&restore.credential_account(&id))?;
                        Ok(saved.map(|saved| Token {
                            access_token: String::new(),
                            refresh_token: saved.refresh_token,
                            scope: saved.scope,
                            expires_at: 0,
                        }))
                    })();
                    if let Ok(mut inner) = restore.inner.lock() {
                        inner.restoring = false;
                        if restore.generation.load(Ordering::SeqCst) == 0
                            && !inner.authorizing
                            && inner.token.is_none()
                        {
                            match result {
                                Ok(token) => inner.token = token,
                                Err(error) => inner.last_error = Some(error),
                            }
                        }
                    }
                })
                .map_err(|_| "Could not start saved Spotify sign-in restore".to_string())?;
        }
        Ok(service)
    }

    pub fn status(&self) -> Result<SpotifyStatus, String> {
        let inner = self.inner.lock().map_err(|_| lock_error())?;
        Ok(SpotifyStatus {
            client_id: inner.config.client_id.clone(),
            configured: inner.config.client_id.is_some(),
            connected: inner.token.is_some(),
            authorizing: inner.authorizing,
            restoring: inner.restoring,
            account: inner.account.clone(),
            last_error: inner.last_error.clone(),
            registered_redirect_uri: if self.native_playback {
                NATIVE_REDIRECT
            } else {
                REGISTERED_REDIRECT
            }
            .into(),
            playback_mode: "spotify_connect_and_native_experiment".into(),
            dj_streaming_available: false,
            playback_permission: inner
                .token
                .as_ref()
                .is_some_and(|t| t.scope.split_whitespace().any(|scope| scope == "streaming")),
            setup_notice: if self.native_playback {
                "Native playback uses a separate librespot sign-in with its upstream registered native client. Only streaming permission is requested. Library credentials are kept separate; Premium is required.".into()
            } else {
                format!("Register {REGISTERED_REDIRECT} for your own Spotify Client ID and allow your account. This login supplies library and Connect access. Sign in to the native player separately for Load A/B.")
            },
        })
    }

    pub fn configure(&self, client_id: &str) -> Result<SpotifyStatus, String> {
        if self.native_playback {
            return Err("Native playback identity is managed by librespot".into());
        }
        let id = validate_client_id(client_id)?;
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        let old = self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .config
            .client_id
            .clone();
        if old.as_deref() == Some(&id) {
            return self.status();
        }
        // Never save a refresh/access token in the JSON settings file.
        let config = Config {
            client_id: Some(id),
        };
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Could not create Spotify settings directory".to_string())?;
        }
        let temp = self.config_path.with_extension("json.tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&config)
                .map_err(|_| "Could not encode Spotify settings".to_string())?,
        )
        .map_err(|_| "Could not save Spotify settings".to_string())?;
        if let Some(old) = old {
            self.vault.remove(&self.credential_account(&old))?;
        }
        std::fs::rename(temp, &self.config_path)
            .map_err(|_| "Could not replace Spotify settings".to_string())?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self.inner.lock().map_err(|_| lock_error())? = Inner {
            config,
            ..Inner::default()
        };
        self.status()
    }

    pub fn disconnect(&self) -> Result<SpotifyStatus, String> {
        // Cancellation precedes serialized network operations so a pending login cannot reconnect.
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        let id = self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .config
            .client_id
            .clone();
        {
            let mut inner = self.inner.lock().map_err(|_| lock_error())?;
            inner.token = None;
            inner.account = None;
            inner.authorizing = false;
            inner.last_error = None;
        }
        if let Some(id) = id {
            self.vault.remove(&self.credential_account(&id))?;
        }
        self.status()
    }

    fn cancel_login(&self, message: &str) -> Result<(), String> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut inner = self.inner.lock().map_err(|_| lock_error())?;
        inner.authorizing = false;
        inner.last_error = Some(message.into());
        Ok(())
    }
    pub fn begin_login(&self) -> Result<LoginStart, String> {
        if self.native_playback {
            self.begin_login_with_scopes(NATIVE_CALLBACK_PORT, "streaming")
        } else {
            self.begin_login_on_port(CALLBACK_PORT)
        }
    }
    fn begin_login_on_port(&self, port: u16) -> Result<LoginStart, String> {
        self.begin_login_with_scopes(port, SCOPES)
    }
    fn begin_login_with_scopes(&self, port: u16, scopes: &str) -> Result<LoginStart, String> {
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        if self.inner.lock().map_err(|_| lock_error())?.restoring {
            return Err(
                "Wait for saved Spotify sign-in restoration or approve the Keychain prompt".into(),
            );
        }
        if self.inner.lock().map_err(|_| lock_error())?.authorizing {
            return Err("Spotify sign-in is already waiting in your browser; finish it or disconnect to cancel".into());
        }
        let id = self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .config
            .client_id
            .clone()
            .ok_or("Configure your own Spotify developer Client ID first")?;
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|_| format!("Could not open Spotify callback port {port}. Close another PulseDeck instance or the app using this port, then retry."))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Could not configure Spotify callback".to_string())?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}{}",
            listener.local_addr().map_err(|_| lock_error())?.port(),
            if self.native_playback {
                "/login"
            } else {
                "/callback"
            }
        );
        let verifier = random_secret();
        let state = random_secret();
        let mut authorization =
            Url::parse("https://accounts.spotify.com/authorize").map_err(|_| lock_error())?;
        authorization.query_pairs_mut().extend_pairs([
            ("client_id", id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", scopes),
            ("state", state.as_str()),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge(&verifier).as_str()),
        ]);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut inner = self.inner.lock().map_err(|_| lock_error())?;
            inner.authorizing = true;
            inner.last_error = None;
        }
        let service = self.clone();
        let redirect = redirect_uri.clone();
        let context = LoginContext {
            generation,
            requested_scopes: scopes.into(),
        };
        std::thread::Builder::new()
            .name("pulsedeck-spotify-auth".into())
            .spawn(move || {
                let result =
                    service.await_login(listener, &redirect, &state, &verifier, &id, context);
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
                "Could not start the Spotify sign-in worker".to_string()
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
        id: &str,
        context: LoginContext,
    ) -> Result<(), String> {
        let generation = context.generation;
        let deadline = Instant::now() + LOGIN_TIMEOUT;
        while Instant::now() < deadline {
            if self.generation.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            match listener.accept() {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let expected_host = redirect
                        .strip_prefix("http://")
                        .and_then(|s| s.split('/').next())
                        .ok_or_else(lock_error)?;
                    let target = match read_request(&mut stream, expected_host) {
                        Ok(target) => target,
                        Err(_) => {
                            respond(&mut stream, 400, "Invalid callback request.");
                            continue;
                        }
                    };
                    match validate_callback(&target, redirect, state) {
                        Ok(Some(code)) => {
                            let _operation = self.operation.lock().map_err(|_| lock_error())?;
                            if self.generation.load(Ordering::SeqCst) != generation {
                                respond(&mut stream, 400, "Sign-in cancelled.");
                                return Ok(());
                            }
                            let result = self
                                .exchange_token(&[
                                    ("grant_type", "authorization_code"),
                                    ("code", &code),
                                    ("redirect_uri", redirect),
                                    ("client_id", id),
                                    ("code_verifier", verifier),
                                ])
                                .and_then(|mut token| {
                                    // OAuth permits omitting scope when it equals the request.
                                    // An explicitly returned reduced grant always takes precedence.
                                    if token.scope.is_none() {
                                        token.scope = Some(context.requested_scopes.clone());
                                    }
                                    self.install_token(id, token, None)
                                });
                            match result {
                                Ok(()) => {
                                    if self.generation.load(Ordering::SeqCst) != generation {
                                        // Disconnect requested during the token request: erase only our new credential.
                                        self.vault.remove(&self.credential_account(id))?;
                                        self.inner.lock().map_err(|_| lock_error())?.token = None;
                                        respond(&mut stream, 400, "Sign-in cancelled.");
                                        return Ok(());
                                    }
                                    respond(&mut stream, 200, "Spotify sign-in completed. Return to PulseDeck to try playback.");
                                    if !self.native_playback {
                                        if let Ok(value) =
                                            self.api_locked(Method::GET, "me", &[], None)
                                        {
                                            if let Ok(mut inner) = self.inner.lock() {
                                                inner.account = parse_account(&value);
                                            }
                                        }
                                    }
                                    return Ok(());
                                }
                                Err(e) => {
                                    respond(
                                        &mut stream,
                                        400,
                                        "Spotify sign-in failed. Return to PulseDeck for details.",
                                    );
                                    return Err(e);
                                }
                            }
                        }
                        Ok(None) => {
                            respond(&mut stream, 404, "Callback not found.");
                        }
                        Err(CallbackError::Invalid) => {
                            respond(&mut stream, 400, "Invalid sign-in callback.");
                        }
                        Err(CallbackError::Denied) => {
                            respond(
                                &mut stream,
                                400,
                                "Spotify sign-in was declined. Return to PulseDeck.",
                            );
                            return Err(
                                "Spotify permission was declined; sign in again when ready".into(),
                            );
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => return Err("Spotify sign-in callback could not be read".into()),
            }
        }
        Err("Spotify sign-in timed out after five minutes; sign in again".into())
    }

    fn exchange_token(&self, form: &[(&str, &str)]) -> Result<TokenResponse, String> {
        let response = self
            .client
            .post("https://accounts.spotify.com/api/token")
            .form(form)
            .send()
            .map_err(|_| {
                "Spotify authorization request failed; check the network connection".to_string()
            })?;
        if !response.status().is_success() {
            return Err(http_error(response.status().as_u16(), None, true));
        }
        let token: TokenResponse = response
            .json()
            .map_err(|_| "Spotify returned an invalid token response".to_string())?;
        if token.access_token.is_empty()
            || !token.token_type.eq_ignore_ascii_case("bearer")
            || token.expires_in == 0
        {
            return Err("Spotify returned an unusable authorization token".into());
        }
        Ok(token)
    }
    fn install_token(
        &self,
        id: &str,
        response: TokenResponse,
        previous: Option<Token>,
    ) -> Result<(), String> {
        let refresh_token = response
            .refresh_token
            .or_else(|| previous.as_ref().map(|t| t.refresh_token.clone()))
            .filter(|s| !s.is_empty())
            .ok_or("Spotify did not provide a refresh token; sign in again")?;
        let scope = response
            .scope
            .or_else(|| previous.as_ref().map(|t| t.scope.clone()))
            .unwrap_or_else(|| SCOPES.into());
        self.vault.set(
            &self.credential_account(id),
            crate::spotify_vault::Credential {
                refresh_token: refresh_token.clone(),
                scope: scope.clone(),
            },
        )?;
        let mut inner = self.inner.lock().map_err(|_| lock_error())?;
        inner.token = Some(Token {
            access_token: response.access_token,
            refresh_token,
            scope,
            expires_at: now_seconds().saturating_add(response.expires_in),
        });
        inner.last_error = None;
        Ok(())
    }
    fn access_token(&self, force_refresh: bool) -> Result<String, String> {
        let (id, token) = {
            let inner = self.inner.lock().map_err(|_| lock_error())?;
            (
                inner
                    .config
                    .client_id
                    .clone()
                    .ok_or("Configure a Spotify Client ID first")?,
                inner.token.clone().ok_or("Sign in to Spotify first")?,
            )
        };
        if !force_refresh
            && !token.access_token.is_empty()
            && token.expires_at > now_seconds().saturating_add(30)
        {
            return Ok(token.access_token);
        }
        let response = self.exchange_token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &token.refresh_token),
            ("client_id", &id),
        ]);
        match response {
            Ok(response) => self.install_token(&id, response, Some(token))?,
            Err(error) => {
                if error.starts_with("Spotify HTTP 400:") || error.starts_with("Spotify HTTP 401:")
                {
                    self.forget_invalid_session(&id, &error)?;
                } else {
                    self.inner.lock().map_err(|_| lock_error())?.last_error = Some(error.clone());
                }
                // Keep the refresh token on transient network/server failures.
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
    /// Native-only credentials for the experimental playback worker. Never
    /// serialized to the webview, logs, argv, environment or a plaintext file.
    pub(crate) fn playback_credentials(&self) -> Result<(String, String), String> {
        if !self.native_playback {
            return Err("Sign in to the native player separately; the library token cannot be used for librespot playback".into());
        }
        if self.status()?.restoring {
            return Err("Wait for native playback sign-in restoration".into());
        }
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        let token = self.access_token(false)?;
        if !self.status()?.playback_permission {
            return Err("Native player sign-in has no streaming permission. Open Spotify account settings and choose Sign in to native player, then retry Load A/B.".into());
        }
        let id = self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .config
            .client_id
            .clone()
            .ok_or("Configure your own Spotify Client ID first")?;
        Ok((id, token))
    }
    fn forget_invalid_session(&self, id: &str, error: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|_| lock_error())?;
        inner.token = None;
        inner.account = None;
        inner.last_error = Some(error.into());
        drop(inner);
        self.vault.remove(&self.credential_account(id))
    }
    fn api(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let _operation = self.operation.lock().map_err(|_| lock_error())?;
        self.api_locked(method, path, query, body)
    }
    fn api_locked(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        {
            let inner = self.inner.lock().map_err(|_| lock_error())?;
            if let Some(until) = inner
                .rate_limited_until
                .filter(|until| *until > Instant::now())
            {
                return Err(format!(
                    "Spotify rate limit: try again in {} seconds",
                    until
                        .duration_since(Instant::now())
                        .as_secs()
                        .saturating_add(1)
                ));
            }
        }
        let url = format!("https://api.spotify.com/v1/{path}");
        for attempt in 0..2 {
            let token = self.access_token(attempt != 0)?;
            let mut request = self
                .client
                .request(method.clone(), &url)
                .bearer_auth(token)
                .query(query);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request
                .send()
                .map_err(|_| "Spotify request failed; check the network connection".to_string())?;
            let status = response.status();
            if status == StatusCode::UNAUTHORIZED && attempt == 0 {
                continue;
            }
            if status == StatusCode::NO_CONTENT {
                return Ok(Value::Null);
            }
            if !status.is_success() {
                if status == StatusCode::UNAUTHORIZED {
                    let id = self
                        .inner
                        .lock()
                        .map_err(|_| lock_error())?
                        .config
                        .client_id
                        .clone();
                    if let Some(id) = id {
                        self.forget_invalid_session(&id, &http_error(401, None, false))?;
                    }
                }
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|v| v.min(86400));
                if status == StatusCode::TOO_MANY_REQUESTS {
                    self.inner
                        .lock()
                        .map_err(|_| lock_error())?
                        .rate_limited_until = Some(
                        Instant::now() + Duration::from_secs(retry_after.unwrap_or(30).max(1)),
                    );
                }
                return Err(http_error(status.as_u16(), retry_after, false));
            }
            return response
                .json()
                .map_err(|_| "Spotify returned an invalid API response".into());
        }
        Err("Spotify sign-in expired; disconnect and sign in again".into())
    }

    pub fn playlists(&self, offset: u64) -> Result<SpotifyPage<SpotifyPlaylist>, String> {
        let raw = self.api(Method::GET, "me/playlists", &page_query(offset), None)?;
        let items = raw["items"]
            .as_array()
            .ok_or("Spotify did not return a playlist page")?
            .iter()
            .filter_map(parse_playlist)
            .collect();
        Ok(page(&raw, items, offset))
    }
    pub fn liked_tracks(&self, offset: u64) -> Result<SpotifyPage<SpotifyTrack>, String> {
        let raw = self.api(Method::GET, "me/tracks", &page_query(offset), None)?;
        let items = raw["items"]
            .as_array()
            .ok_or("Spotify did not return a liked-song page")?
            .iter()
            .filter_map(|v| parse_track(&v["track"]))
            .collect();
        Ok(page(&raw, items, offset))
    }
    pub fn playlist_tracks(
        &self,
        id: &str,
        offset: u64,
    ) -> Result<SpotifyPage<SpotifyTrack>, String> {
        if !valid_id(id) {
            return Err("Invalid Spotify playlist ID".into());
        }
        // 2026 Development Mode uses /items; older Extended Quota apps keep /tracks.
        let raw = match self.api(
            Method::GET,
            &format!("playlists/{id}/items"),
            &page_query(offset),
            None,
        ) {
            Err(e) if e.starts_with("Spotify HTTP 404") => self.api(
                Method::GET,
                &format!("playlists/{id}/tracks"),
                &page_query(offset),
                None,
            )?,
            result => result?,
        };
        let values = raw["items"].as_array().ok_or("Playlist contents are unavailable; development apps can read only playlists you own or collaborate on")?;
        let items = values
            .iter()
            .filter_map(|v| parse_track(v.get("item").unwrap_or(&v["track"])))
            .collect();
        Ok(page(&raw, items, offset))
    }
    pub fn album_tracks(
        &self,
        id: &str,
        name: &str,
        offset: u64,
    ) -> Result<SpotifyPage<SpotifyTrack>, String> {
        if !valid_id(id) {
            return Err("Invalid Spotify album ID".into());
        }
        let raw = self.api(
            Method::GET,
            &format!("albums/{id}/tracks"),
            &page_query(offset),
            None,
        )?;
        let items = raw["items"]
            .as_array()
            .ok_or("Spotify did not return an album page")?
            .iter()
            .filter_map(|value| {
                let mut value = value.clone();
                value["album"] = serde_json::json!({"id": id, "name": name});
                parse_track(&value)
            })
            .collect();
        Ok(page(&raw, items, offset))
    }
    pub fn devices(&self) -> Result<Vec<SpotifyDevice>, String> {
        let raw = self.api(Method::GET, "me/player/devices", &[], None)?;
        let devices = raw["devices"]
            .as_array()
            .ok_or("Spotify did not return a device list")?;
        Ok(devices
            .iter()
            .map(|v| SpotifyDevice {
                id: v["id"].as_str().map(str::to_owned),
                name: text(v, "name"),
                device_type: text(v, "type"),
                is_active: v["is_active"].as_bool().unwrap_or(false),
                is_restricted: v["is_restricted"].as_bool().unwrap_or(true),
                volume_percent: v["volume_percent"].as_u64(),
            })
            .collect())
    }
    pub fn play(&self, uri: &str, device_id: Option<&str>) -> Result<(), String> {
        let id = uri
            .strip_prefix("spotify:track:")
            .filter(|id| valid_id(id))
            .ok_or(
                "Select a Spotify catalog track; local files cannot be played through this API",
            )?;
        self.api(
            Method::PUT,
            "me/player/play",
            &device_query(device_id)?,
            Some(json!({"uris": [format!("spotify:track:{id}")]})),
        )?;
        Ok(())
    }
    pub fn pause(&self, device_id: Option<&str>) -> Result<(), String> {
        self.api(
            Method::PUT,
            "me/player/pause",
            &device_query(device_id)?,
            None,
        )?;
        Ok(())
    }
}
fn device_query(id: Option<&str>) -> Result<Vec<(&'static str, String)>, String> {
    match id {
        None => Ok(vec![]),
        Some(id) if valid_id(id) => Ok(vec![("device_id", id.into())]),
        _ => Err("Invalid Spotify Connect device ID".into()),
    }
}
fn page_query(offset: u64) -> Vec<(&'static str, String)> {
    vec![
        ("limit", PAGE_SIZE.to_string()),
        ("offset", offset.to_string()),
    ]
}
fn page<T>(raw: &Value, items: Vec<T>, offset: u64) -> SpotifyPage<T> {
    // Advance by raw items, including unavailable/episode entries we filtered out.
    let count = raw["items"].as_array().map_or(0, |a| a.len()) as u64;
    let total = raw["total"]
        .as_u64()
        .unwrap_or(offset.saturating_add(count));
    let next_offset =
        (raw["next"].as_str().is_some() && count > 0).then_some(offset.saturating_add(count));
    SpotifyPage {
        items,
        offset,
        total,
        next_offset,
    }
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}
fn external_url(v: &Value, kind: &str, id: &str) -> String {
    v["external_urls"]["spotify"]
        .as_str()
        .filter(|s| {
            Url::parse(s)
                .is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some("open.spotify.com"))
        })
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://open.spotify.com/{kind}/{id}"))
}
fn artwork(v: &Value) -> Option<String> {
    v["images"].as_array()?.first()?["url"]
        .as_str()
        .filter(|s| Url::parse(s).is_ok_and(|u| u.scheme() == "https"))
        .map(str::to_owned)
}
fn parse_account(v: &Value) -> Option<Account> {
    let id = v["id"].as_str()?.to_string();
    Some(Account {
        display_name: v["display_name"].as_str().unwrap_or(&id).into(),
        external_url: external_url(v, "user", &id),
        id,
    })
}
fn parse_playlist(v: &Value) -> Option<SpotifyPlaylist> {
    let id = v["id"].as_str()?.to_string();
    if !valid_id(&id) {
        return None;
    }
    Some(SpotifyPlaylist {
        name: text(v, "name"),
        description: text(v, "description"),
        total: v["items"]["total"]
            .as_u64()
            .or_else(|| v["tracks"]["total"].as_u64()),
        external_url: external_url(v, "playlist", &id),
        artwork_url: artwork(v),
        owner_id: text(&v["owner"], "id"),
        collaborative: v["collaborative"].as_bool().unwrap_or(false),
        id,
    })
}
fn parse_track(v: &Value) -> Option<SpotifyTrack> {
    if v["type"].as_str().is_some_and(|kind| kind != "track") {
        return None;
    }
    let id = v["id"].as_str()?.to_string();
    if !valid_id(&id) {
        return None;
    }
    let is_local = v["is_local"].as_bool().unwrap_or(false);
    Some(SpotifyTrack {
        uri: format!("spotify:track:{id}"),
        title: text(v, "name"),
        artists: v["artists"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|a| a["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        album: text(&v["album"], "name"),
        album_id: v["album"]["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .map(str::to_owned),
        duration_ms: v["duration_ms"].as_u64().unwrap_or(0),
        artwork_url: artwork(&v["album"]),
        external_url: external_url(v, "track", &id),
        is_local,
        available: !is_local
            && v["is_playable"].as_bool().unwrap_or(true)
            && v["restrictions"]["reason"].is_null(),
        playable_in_deck: false,
        id,
    })
}
fn http_error(status: u16, retry_after: Option<u64>, token: bool) -> String {
    let message = match status {
        400 if token => "Spotify authorization was rejected; check your Client ID and registered redirect URI, then sign in again".into(),
        401 => "Spotify sign-in expired or was revoked; disconnect and sign in again".into(),
        403 => "Spotify denied access: check Premium, developer account allowlist, scopes and playlist ownership/collaboration".into(),
        404 => "No Spotify Connect device or accessible item was found; open Spotify on the desired device".into(),
        429 => format!("Rate limited; try again in {} seconds", retry_after.unwrap_or(30)),
        500..=599 => "Spotify is temporarily unavailable; try again later".into(),
        _ => "Spotify could not complete this request".into(),
    };
    format!("Spotify HTTP {status}: {message}")
}
#[derive(Debug, PartialEq)]
enum CallbackError {
    Invalid,
    Denied,
}
fn validate_callback(
    target: &str,
    redirect: &str,
    expected_state: &str,
) -> Result<Option<String>, CallbackError> {
    if !target.starts_with('/') || target.starts_with("//") || target.contains('#') {
        return Err(CallbackError::Invalid);
    }
    let base = Url::parse(redirect).map_err(|_| CallbackError::Invalid)?;
    if target.split('?').next() != Some(base.path()) {
        return Ok(None);
    }
    let url = base.join(target).map_err(|_| CallbackError::Invalid)?;
    if url.origin() != base.origin() || url.path() != base.path() {
        return Ok(None);
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref())
            .collect::<Vec<_>>()
    };
    let states = values("state");
    if states.len() != 1 || states[0] != expected_state {
        return Err(CallbackError::Invalid);
    }
    let errors = values("error");
    let codes = values("code");
    if !errors.is_empty() {
        if errors.len() == 1 && codes.is_empty() {
            return Err(CallbackError::Denied);
        }
        return Err(CallbackError::Invalid);
    }
    if codes.len() != 1 || codes[0].is_empty() || codes[0].len() > 4096 {
        return Err(CallbackError::Invalid);
    }
    Ok(Some(codes[0].into()))
}
fn read_request(stream: &mut TcpStream, expected_host: &str) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|_| lock_error())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::with_capacity(2048);
    let mut buffer = [0u8; 1024];
    loop {
        if Instant::now() >= deadline || bytes.len() >= 8192 {
            return Err("Callback request exceeded limits".into());
        }
        let n = stream
            .read(&mut buffer)
            .map_err(|_| "Callback request could not be read".to_string())?;
        if n == 0 {
            return Err("Incomplete callback request".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid callback encoding".to_string())?;
    let hosts = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("host").then_some(value.trim())
        })
        .collect::<Vec<_>>();
    if hosts.len() != 1 || hosts[0] != expected_host {
        return Err("Callback Host did not match this sign-in listener".into());
    }
    let mut parts = text.lines().next().unwrap_or_default().split_whitespace();
    if parts.next() != Some("GET") {
        return Err("Callback must use GET".into());
    }
    let target = parts.next().ok_or("Missing callback target")?;
    if !matches!(parts.next(), Some("HTTP/1.1" | "HTTP/1.0")) || parts.next().is_some() {
        return Err("Invalid callback protocol".into());
    }
    Ok(target.into())
}
fn respond(stream: &mut TcpStream, code: u16, message: &str) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let response = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{message}", message.len());
    let _ = stream.write_all(response.as_bytes());
}

async fn background<T: Send + 'static>(
    service: SpotifyService,
    job: impl FnOnce(SpotifyService) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || job(service))
        .await
        .map_err(|_| "Spotify background task failed".to_string())?
}
#[tauri::command]
pub async fn spotify_native_status(
    service: tauri::State<'_, NativeSpotifyService>,
) -> Result<SpotifyStatus, String> {
    service.0.status()
}
#[tauri::command]
pub async fn spotify_native_begin_login(
    service: tauri::State<'_, NativeSpotifyService>,
) -> Result<LoginStart, String> {
    background(service.0.clone(), |s| {
        let start = s.begin_login()?;
        if open::that(&start.authorization_url).is_err() {
            s.cancel_login("Could not open native playback sign-in in your browser")?;
            return Err("Could not open native playback sign-in in your browser".into());
        }
        Ok(start)
    })
    .await
}
#[tauri::command]
pub async fn spotify_native_disconnect(
    app: tauri::AppHandle,
    service: tauri::State<'_, NativeSpotifyService>,
) -> Result<SpotifyStatus, String> {
    crate::spotify_decks::clear_account_sources(&app)?;
    background(service.0.clone(), |s| s.disconnect()).await
}
#[tauri::command]
pub async fn spotify_status(
    service: tauri::State<'_, SpotifyService>,
) -> Result<SpotifyStatus, String> {
    service.status()
}
#[tauri::command]
pub async fn spotify_configure(
    app: tauri::AppHandle,
    service: tauri::State<'_, SpotifyService>,
    client_id: String,
) -> Result<SpotifyStatus, String> {
    let previous = service.status()?.client_id;
    let status = background(service.inner().clone(), move |s| s.configure(&client_id)).await?;
    if status.client_id != previous {
        crate::spotify_decks::clear_account_sources(&app)?;
    }
    Ok(status)
}
#[tauri::command]
pub async fn spotify_begin_login(
    service: tauri::State<'_, SpotifyService>,
    playback: Option<bool>,
) -> Result<LoginStart, String> {
    background(service.inner().clone(), move |s| {
        let start = if playback.unwrap_or(false) {
            s.begin_login_with_scopes(CALLBACK_PORT, PLAYBACK_SCOPES)?
        } else {
            s.begin_login()?
        };
        #[cfg(target_os = "macos")]
        {
            if open::that(&start.authorization_url).is_err() {
                let message = "Could not open the default browser; retry Spotify sign-in";
                s.cancel_login(message)?;
                return Err(message.into());
            }
        }
        Ok(start)
    })
    .await
}
#[tauri::command]
pub async fn spotify_disconnect(
    app: tauri::AppHandle,
    service: tauri::State<'_, SpotifyService>,
) -> Result<SpotifyStatus, String> {
    crate::spotify_decks::clear_account_sources(&app)?;
    background(service.inner().clone(), |s| s.disconnect()).await
}
#[tauri::command]
pub async fn spotify_playlists(
    service: tauri::State<'_, SpotifyService>,
    offset: u64,
) -> Result<SpotifyPage<SpotifyPlaylist>, String> {
    background(service.inner().clone(), move |s| s.playlists(offset)).await
}
#[tauri::command]
pub async fn spotify_liked_tracks(
    service: tauri::State<'_, SpotifyService>,
    offset: u64,
) -> Result<SpotifyPage<SpotifyTrack>, String> {
    background(service.inner().clone(), move |s| s.liked_tracks(offset)).await
}
#[tauri::command]
pub async fn spotify_playlist_tracks(
    service: tauri::State<'_, SpotifyService>,
    playlist_id: String,
    offset: u64,
) -> Result<SpotifyPage<SpotifyTrack>, String> {
    background(service.inner().clone(), move |s| {
        s.playlist_tracks(&playlist_id, offset)
    })
    .await
}
#[tauri::command]
pub async fn spotify_album_tracks(
    service: tauri::State<'_, SpotifyService>,
    album_id: String,
    album_name: String,
    offset: u64,
) -> Result<SpotifyPage<SpotifyTrack>, String> {
    background(service.inner().clone(), move |s| {
        s.album_tracks(&album_id, &album_name, offset)
    })
    .await
}
#[tauri::command]
pub async fn spotify_devices(
    service: tauri::State<'_, SpotifyService>,
) -> Result<Vec<SpotifyDevice>, String> {
    background(service.inner().clone(), |s| s.devices()).await
}
#[tauri::command]
pub async fn spotify_play(
    service: tauri::State<'_, SpotifyService>,
    uri: String,
    device_id: Option<String>,
) -> Result<(), String> {
    background(service.inner().clone(), move |s| {
        s.play(&uri, device_id.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn spotify_pause(
    service: tauri::State<'_, SpotifyService>,
    device_id: Option<String>,
) -> Result<(), String> {
    background(service.inner().clone(), move |s| {
        s.pause(device_id.as_deref())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_matches_rfc_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = random_secret();
        assert_eq!(v.len(), 43);
        assert!(v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }
    #[test]
    fn callback_binds_state_and_rejects_ambiguous_input() {
        let redirect = "http://127.0.0.1:34567/callback";
        assert_eq!(
            validate_callback("/callback?code=abc&state=nonce", redirect, "nonce"),
            Ok(Some("abc".into()))
        );
        for target in [
            "/callback?code=abc",
            "/callback?code=abc&state=wrong",
            "/callback?code=abc&state=nonce&state=nonce",
            "/callback?code=a&code=b&state=nonce",
            "//evil.test/callback?code=a&state=nonce",
            "https://evil.test/callback?code=a&state=nonce",
            "/callback?code=a&state=nonce#fragment",
            "/callback?error=denied&code=a&state=nonce",
        ] {
            assert_eq!(
                validate_callback(target, redirect, "nonce"),
                Err(CallbackError::Invalid),
                "{target}"
            );
        }
        assert_eq!(
            validate_callback(
                "/callback?error=access_denied&state=nonce",
                redirect,
                "nonce"
            ),
            Err(CallbackError::Denied)
        );
        assert_eq!(
            validate_callback("/favicon.ico", redirect, "nonce"),
            Ok(None)
        );
    }
    #[test]
    fn remote_tracks_never_advertise_native_audio() {
        let v = json!({"id":"abc123","type":"track","name":"Track","artists":[{"name":"Artist"}],"album":{"name":"Album","images":[]},"duration_ms":1234,"preview_url":"https://example.test/preview.mp3"});
        let track = parse_track(&v).unwrap();
        assert!(!track.playable_in_deck);
        assert_eq!(track.uri, "spotify:track:abc123");
        let encoded = serde_json::to_string(&track).unwrap();
        assert!(!encoded.contains("preview"));
        assert!(!encoded.contains("path"));
        assert!(parse_track(&json!({"id":"abc","type":"episode"})).is_none());
        assert!(parse_track(&Value::Null).is_none());
    }
    #[test]
    fn page_preserves_progress_when_entries_are_filtered() {
        let raw = json!({"items":[null,{"track":null},{"item":{"type":"episode"}}],"total":100,"next":"https://api.spotify.com/v1/me/tracks?offset=3"});
        let p: SpotifyPage<SpotifyTrack> = page(&raw, vec![], 0);
        assert_eq!(p.next_offset, Some(3));
        assert_eq!(p.total, 100);
        let p: SpotifyPage<SpotifyTrack> =
            page(&json!({"items":[],"total":100,"next":"bad"}), vec![], 0);
        assert_eq!(p.next_offset, None);
    }
    #[test]
    fn supports_current_and_legacy_playlist_metadata() {
        assert_eq!(
            parse_playlist(&json!({"id":"abc","items":{"total":12}}))
                .unwrap()
                .total,
            Some(12)
        );
        assert_eq!(
            parse_playlist(&json!({"id":"abc","tracks":{"total":9}}))
                .unwrap()
                .total,
            Some(9)
        );
        assert_eq!(parse_playlist(&json!({"id":"abc"})).unwrap().total, None);
        assert!(valid_id("abc123"));
        assert!(!valid_id("../me"));
        assert!(!valid_id("abc?token=secret"));
    }
    #[test]
    fn api_errors_explain_required_user_actions() {
        assert!(http_error(403, None, false).contains("allowlist"));
        assert!(http_error(429, Some(17), false).contains("17 seconds"));
        assert!(http_error(404, None, false).contains("Connect device"));
    }
    #[test]
    fn local_callback_reads_fragmented_http_headers() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let sender = std::thread::spawn(move || {
            let mut client = TcpStream::connect(addr).unwrap();
            client
                .write_all(b"GET /callback?code=test&state=x HTTP/1.1\r\nHost: 127.0.0.1\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(20));
            client.write_all(b"\r\n").unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(
            read_request(&mut stream, "127.0.0.1").unwrap(),
            "/callback?code=test&state=x"
        );
        sender.join().unwrap();
    }
    #[test]
    fn loopback_login_survives_spoofed_state_and_handles_denial() {
        let path =
            std::env::temp_dir().join(format!("pulsedeck-spotify-test-{}.json", random_secret()));
        let service = SpotifyService::new(path).unwrap();
        service.inner.lock().unwrap().config.client_id =
            Some("0123456789abcdef0123456789abcdef".into());
        let start = service.begin_login().unwrap();
        assert_eq!(start.redirect_uri, REGISTERED_REDIRECT);
        let auth = Url::parse(&start.authorization_url).unwrap();
        assert!(auth
            .query_pairs()
            .any(|(key, value)| key == "redirect_uri" && value == REGISTERED_REDIRECT));
        let state = auth
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let redirect = Url::parse(&start.redirect_uri).unwrap();
        assert_eq!(redirect.host_str(), Some("127.0.0.1"));
        assert!(redirect.port().is_some());
        assert_eq!(redirect.path(), "/callback");
        assert!(auth
            .query_pairs()
            .any(|(k, v)| k == "code_challenge_method" && v == "S256"));
        assert!(!start.authorization_url.contains("client_secret"));
        let addr = format!("127.0.0.1:{}", redirect.port().unwrap());
        for (nonce, denied) in [("wrong", false), (state.as_str(), true)] {
            let mut client = TcpStream::connect(&addr).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            write!(
                client,
                "GET /callback?error=access_denied&state={nonce} HTTP/1.1\r\nHost: {addr}\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 400"));
            assert!(!response.contains(nonce));
            if !denied {
                assert!(service.status().unwrap().authorizing);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while service.status().unwrap().authorizing && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = service.status().unwrap();
        assert!(!status.authorizing);
        assert!(!status.connected);
        assert!(status.last_error.unwrap().contains("declined"));
    }
    #[test]
    fn cancelling_pending_login_clears_readiness_without_touching_existing_tokens() {
        let path = std::env::temp_dir().join(format!(
            "pulsedeck-spotify-cancel-test-{}.json",
            random_secret()
        ));
        let service = SpotifyService::new(path).unwrap();
        service.inner.lock().unwrap().config.client_id =
            Some("0123456789abcdef0123456789abcdef".into());
        service.inner.lock().unwrap().token = Some(Token {
            access_token: "not-a-real-access-token".into(),
            refresh_token: "not-a-real-refresh-token".into(),
            scope: SCOPES.into(),
            expires_at: 0,
        });
        let start = service.begin_login_on_port(0).unwrap();
        assert!(service.begin_login_on_port(0).is_err());
        service.cancel_login("Browser could not open").unwrap();
        let status = service.status().unwrap();
        assert!(!status.authorizing);
        assert!(status.connected);
        let encoded = serde_json::to_string(&status).unwrap();
        assert!(!encoded.contains("not-a-real"));
        assert!(!encoded.contains("refresh_token"));
        let redirect = Url::parse(&start.redirect_uri).unwrap();
        let addr = format!("127.0.0.1:{}", redirect.port().unwrap());
        let deadline = Instant::now() + Duration::from_secs(1);
        while TcpStream::connect(&addr).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(TcpStream::connect(&addr).is_err());
    }
    #[test]
    fn playback_consent_requests_streaming_and_preserves_existing_library_session() {
        let service = SpotifyService::new(std::env::temp_dir().join(format!(
            "pulsedeck-playback-consent-{}.json",
            random_secret()
        )))
        .unwrap();
        {
            let mut inner = service.inner.lock().unwrap();
            inner.config.client_id = Some("0123456789abcdef0123456789abcdef".into());
            inner.token = Some(Token {
                access_token: "fake-library-token".into(),
                refresh_token: "fake-refresh-token".into(),
                scope: SCOPES.into(),
                expires_at: now_seconds() + 3600,
            });
        }
        assert!(!service.status().unwrap().playback_permission);
        assert!(service
            .playback_credentials()
            .unwrap_err()
            .contains("library token cannot be used"));
        let start = service.begin_login_with_scopes(0, PLAYBACK_SCOPES).unwrap();
        let auth = Url::parse(&start.authorization_url).unwrap();
        let scopes = auth
            .query_pairs()
            .find(|(key, _)| key == "scope")
            .unwrap()
            .1
            .into_owned();
        assert!(scopes.split_whitespace().any(|s| s == "streaming"));
        for original in SCOPES.split_whitespace() {
            assert!(scopes.split_whitespace().any(|s| s == original));
        }
        service.cancel_login("Consent cancelled").unwrap();
        assert!(service.status().unwrap().connected);
        assert!(!service.status().unwrap().playback_permission);
        service.inner.lock().unwrap().token.as_mut().unwrap().scope = PLAYBACK_SCOPES.into();
        assert!(service.status().unwrap().playback_permission);
        assert!(service
            .playback_credentials()
            .unwrap_err()
            .contains("library token cannot be used"));
        let encoded = serde_json::to_string(&service.status().unwrap()).unwrap();
        assert!(!encoded.contains("fake-library-token"));
    }
    #[test]
    fn native_login_has_separate_identity_storage_and_state_checked_callback() {
        // Build an isolated fixture without reading an actual native Keychain entry.
        let mut service = SpotifyService::new(
            std::env::temp_dir().join(format!("pulsedeck-native-fixture-{}.json", random_secret())),
        )
        .unwrap();
        service.native_playback = true;
        let id = spotify_playback::native_client_id();
        {
            let mut inner = service.inner.lock().unwrap();
            inner.config.client_id = Some(id.clone());
            inner.token = Some(Token {
                access_token: "fake-native-token".into(),
                refresh_token: "fake-native-refresh".into(),
                scope: "streaming".into(),
                expires_at: now_seconds() + 3600,
            });
        }
        assert_eq!(service.credential_account(&id), format!("native:{id}"));
        assert!(service
            .configure("0123456789abcdef0123456789abcdef")
            .is_err());
        let start = service.begin_login_with_scopes(0, "streaming").unwrap();
        let redirect = Url::parse(&start.redirect_uri).unwrap();
        assert_eq!(redirect.path(), "/login");
        let auth = Url::parse(&start.authorization_url).unwrap();
        let query: std::collections::HashMap<_, _> = auth.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], id);
        assert_eq!(query["scope"], "streaming");
        assert_eq!(
            validate_callback(
                &format!("/login?code=fixture&state={}", query["state"]),
                &start.redirect_uri,
                &query["state"]
            )
            .unwrap(),
            Some("fixture".into())
        );
        assert!(validate_callback(
            "/login?code=fixture&state=wrong",
            &start.redirect_uri,
            &query["state"]
        )
        .is_err());
        assert_eq!(
            validate_callback(
                "/callback?code=fixture",
                &start.redirect_uri,
                &query["state"]
            )
            .unwrap(),
            None
        );
        service.cancel_login("Fixture consent cancelled").unwrap();
        assert!(service.status().unwrap().connected);
        assert_eq!(
            service.playback_credentials().unwrap().1,
            "fake-native-token"
        );
        assert!(!serde_json::to_string(&service.status().unwrap())
            .unwrap()
            .contains("fake-native"));
    }
    #[test]
    fn occupied_callback_port_leaves_login_retryable() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let path = std::env::temp_dir().join(format!(
            "pulsedeck-spotify-port-test-{}.json",
            random_secret()
        ));
        let service = SpotifyService::new(path).unwrap();
        service.inner.lock().unwrap().config.client_id =
            Some("0123456789abcdef0123456789abcdef".into());
        let error = service.begin_login_on_port(port).err().unwrap();
        assert!(error.contains(&port.to_string()));
        assert!(!service.status().unwrap().authorizing);
        drop(listener);
        let start = service.begin_login_on_port(port).unwrap();
        assert_eq!(Url::parse(&start.redirect_uri).unwrap().port(), Some(port));
        service.cancel_login("Test complete").unwrap();
    }
}
