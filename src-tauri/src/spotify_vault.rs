//! One shared Keychain read per application session, with durable-change writes.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Mutex};

const SERVICE: &str = "com.pulsedeck.spotify";
const ACCOUNT: &str = "session-v1";
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Credential {
    pub refresh_token: String,
    pub scope: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct Bundle {
    accounts: BTreeMap<String, Credential>,
}
trait Backend: Send + Sync {
    fn read(&self, account: &str) -> Result<Option<String>, String>;
    fn write(&self, account: &str, value: &str) -> Result<(), String>;
}
#[derive(Default)]
struct Keychain {
    #[cfg(target_os = "macos")]
    items: Mutex<BTreeMap<String, security_framework::os::macos::keychain_item::SecKeychainItem>>,
}
#[cfg(target_os = "macos")]
impl Backend for Keychain {
    fn read(&self, account: &str) -> Result<Option<String>, String> {
        use security_framework::os::macos::passwords::find_generic_password;
        match find_generic_password(None, SERVICE, account) {
            Ok((password, item)) => {
                let value = String::from_utf8(password.to_owned()).map_err(|_| "Invalid saved Spotify credential")?;
                self.items.lock().map_err(|_| "Keychain item cache unavailable")?.insert(account.into(), item);
                Ok(Some(value))
            },
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err("Keychain access was not granted; saved Spotify sign-in is unavailable for this session".into()),
        }
    }
    fn write(&self, account: &str, value: &str) -> Result<(), String> {
        use security_framework::{
            item::{ItemClass, ItemSearchOptions, Reference, SearchResult},
            os::macos::keychain::SecKeychain,
        };
        let mut items = self
            .items
            .lock()
            .map_err(|_| "Keychain item cache unavailable")?;
        if let Some(item) = items.get_mut(account) {
            // Modify the authorized item in place, without reading its secret
            // again as keyring::set_password would do on every token refresh.
            return item.set_password(value.as_bytes()).map_err(|_| {
                "Could not save changed Spotify credentials securely in Keychain".into()
            });
        }
        SecKeychain::default()
            .and_then(|keychain| keychain.add_generic_password(SERVICE, account, value.as_bytes()))
            .map_err(|_| "Could not create secure Spotify credential bundle")?;
        // Retrieve only the item reference; no password data or extra secret read.
        if let Ok(results) = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(account)
            .load_refs(true)
            .search()
        {
            for result in results {
                if let SearchResult::Ref(Reference::KeychainItem(item)) = result {
                    items.insert(account.into(), item);
                    break;
                }
            }
        }
        Ok(())
    }
}
#[cfg(not(target_os = "macos"))]
impl Backend for Keychain {
    fn read(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, account)
            .map_err(|_| "Could not open PulseDeck credential entry")?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Saved Spotify credentials are unavailable".into()),
        }
    }
    fn write(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, account)
            .map_err(|_| "Could not open credential entry")?
            .set_password(value)
            .map_err(|_| "Could not save changed Spotify credentials securely".into())
    }
}
/// Errors are cached too: denying startup access never causes another prompt on
/// playlist/deck requests. A restart or explicit new sign-in is required.
pub struct Vault {
    cache: Mutex<Option<Result<Bundle, String>>>,
    legacy: Vec<String>,
    backend: Box<dyn Backend>,
}
impl Vault {
    pub fn new(legacy: Vec<String>) -> Self {
        Self {
            cache: Mutex::new(None),
            legacy,
            backend: Box::<Keychain>::default(),
        }
    }
    fn restore(&self) -> Result<Bundle, String> {
        if let Some(raw) = self.backend.read(ACCOUNT)? {
            return serde_json::from_str(&raw)
                .map_err(|_| "Saved Spotify credential bundle is invalid; sign in again".into());
        }
        let mut bundle = Bundle::default();
        // One-time migration preserves both existing OAuth grants. Future
        // sessions read only ACCOUNT. Old entries are retained for recovery.
        for account in &self.legacy {
            if let Some(raw) = self.backend.read(account)? {
                let saved: Credential = serde_json::from_str(&raw)
                    .map_err(|_| "Legacy Spotify credential is invalid".to_string())?;
                if saved.refresh_token.is_empty() {
                    return Err("Saved Spotify refresh token is empty".into());
                }
                bundle.accounts.insert(account.clone(), saved);
            }
        }
        if !bundle.accounts.is_empty() {
            self.backend.write(
                ACCOUNT,
                &serde_json::to_string(&bundle)
                    .map_err(|_| "Could not encode saved Spotify sign-in")?,
            )?;
        }
        Ok(bundle)
    }
    pub fn get(&self, account: &str) -> Result<Option<Credential>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "Spotify credential cache is unavailable")?;
        if cache.is_none() {
            *cache = Some(self.restore());
        }
        cache
            .as_ref()
            .unwrap()
            .as_ref()
            .map(|b| b.accounts.get(account).cloned())
            .map_err(Clone::clone)
    }
    pub fn set(&self, account: &str, credential: Credential) -> Result<(), String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "Spotify credential cache is unavailable")?;
        if cache.is_none() || cache.as_ref().is_some_and(Result::is_err) {
            *cache = Some(self.restore());
        }
        // Explicit new sign-in can retry a denied startup read. It updates
        // only this account; an unread bundle must never be overwritten.
        let bundle = cache.as_ref().unwrap().as_ref().map_err(Clone::clone)?;
        if bundle.accounts.get(account) == Some(&credential) {
            return Ok(());
        }
        let mut next = bundle.clone();
        next.accounts.insert(account.into(), credential);
        self.backend.write(
            ACCOUNT,
            &serde_json::to_string(&next).map_err(|_| "Could not encode Spotify sign-in")?,
        )?;
        *cache = Some(Ok(next));
        Ok(())
    }
    pub fn remove(&self, account: &str) -> Result<(), String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "Spotify credential cache is unavailable")?;
        if cache.is_none() {
            *cache = Some(self.restore());
        }
        let bundle = cache.as_ref().unwrap().as_ref().map_err(Clone::clone)?;
        if !bundle.accounts.contains_key(account) {
            return Ok(());
        }
        let mut next = bundle.clone();
        next.accounts.remove(account);
        // Keep the empty bundle, so legacy recovery entries cannot resurrect a
        // deliberately disconnected session on the next startup.
        self.backend.write(
            ACCOUNT,
            &serde_json::to_string(&next).map_err(|_| "Could not encode Spotify sign-in")?,
        )?;
        *cache = Some(Ok(next));
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Memory {
        values: Arc<Mutex<BTreeMap<String, String>>>,
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
        fail_write: bool,
        fail_read: bool,
    }
    impl Backend for Memory {
        fn read(&self, account: &str) -> Result<Option<String>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_read {
                return Err("denied".into());
            }
            Ok(self.values.lock().unwrap().get(account).cloned())
        }
        fn write(&self, account: &str, value: &str) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_write {
                return Err("write failed".into());
            }
            self.values
                .lock()
                .unwrap()
                .insert(account.into(), value.into());
            Ok(())
        }
    }
    fn credential() -> Credential {
        Credential {
            refresh_token: "fixture-refresh".into(),
            scope: "streaming".into(),
        }
    }
    fn fixture(
        values: BTreeMap<String, String>,
        fail_read: bool,
        fail_write: bool,
    ) -> (Arc<Vault>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        (
            Arc::new(Vault {
                cache: Mutex::new(None),
                legacy: vec!["library".into(), "native".into()],
                backend: Box::new(Memory {
                    values: Arc::new(Mutex::new(values)),
                    reads: reads.clone(),
                    writes: writes.clone(),
                    fail_read,
                    fail_write,
                }),
            }),
            reads,
            writes,
        )
    }
    #[test]
    fn concurrent_consumers_restore_one_bundle_and_refresh_without_writing_unchanged_tokens() {
        let bundle = Bundle {
            accounts: BTreeMap::from([
                ("library".into(), credential()),
                ("native".into(), credential()),
            ]),
        };
        let (vault, reads, writes) = fixture(
            BTreeMap::from([(ACCOUNT.into(), serde_json::to_string(&bundle).unwrap())]),
            false,
            false,
        );
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let vault = vault.clone();
                std::thread::spawn(move || vault.get("native").unwrap())
            })
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap().is_some());
        }
        vault.set("native", credential()).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        let mut changed = credential();
        changed.refresh_token = "rotated".into();
        vault.set("native", changed).unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert!(vault.get("library").unwrap() == Some(credential()));
    }
    #[test]
    fn migration_reads_legacy_once_then_disconnect_keeps_other_grant_and_never_rereads() {
        let (vault, reads, writes) = fixture(
            BTreeMap::from([
                (
                    "library".into(),
                    serde_json::to_string(&credential()).unwrap(),
                ),
                (
                    "native".into(),
                    serde_json::to_string(&credential()).unwrap(),
                ),
            ]),
            false,
            false,
        );
        assert!(vault.get("library").unwrap().is_some());
        assert!(vault.get("native").unwrap().is_some());
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        vault.remove("native").unwrap();
        assert!(vault.get("native").unwrap().is_none());
        assert!(vault.get("library").unwrap().is_some());
        assert_eq!(reads.load(Ordering::SeqCst), 3);
    }
    #[test]
    fn denied_read_is_not_retried_by_other_layers_and_failed_rotation_does_not_replace_cache() {
        let (vault, reads, _) = fixture(BTreeMap::new(), true, false);
        for _ in 0..10 {
            assert!(vault.get("library").is_err());
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let bundle = Bundle {
            accounts: BTreeMap::from([("native".into(), credential())]),
        };
        let (vault, _, _) = fixture(
            BTreeMap::from([(ACCOUNT.into(), serde_json::to_string(&bundle).unwrap())]),
            false,
            true,
        );
        let before = vault.get("native").unwrap();
        let mut next = credential();
        next.refresh_token = "changed".into();
        assert!(vault.set("native", next).is_err());
        assert!(vault.get("native").unwrap() == before);
    }
}
