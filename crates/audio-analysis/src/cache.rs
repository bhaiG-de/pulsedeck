//! Derived-analysis identity and bounded singleflight; never used by the audio callback.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOutcome {
    Ready,
    Unavailable,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageStamp {
    pub version: u32,
    pub dependency: String,
    pub outcome: StageOutcome,
    pub reason: Option<String>,
}
impl StageStamp {
    pub fn current(&self, version: u32, dependency: &str) -> bool {
        self.version == version && self.dependency == dependency
    }
    pub fn ready(version: u32, dependency: String) -> Self {
        Self {
            version,
            dependency,
            outcome: StageOutcome::Ready,
            reason: None,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisStages {
    /// Verified encoded local file identity; independent of derived algorithms.
    pub source_file: Option<StageStamp>,
    pub rhythm: Option<StageStamp>,
    pub waveform: Option<StageStamp>,
    pub key: Option<StageStamp>,
    pub neural: Option<StageStamp>,
}
/// Encoded local identity is cheap to verify before decoding. Derived PCM
/// fingerprints remain authoritative if tags/container bytes have changed.
pub fn encoded_file_fingerprint(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("encoded-sha256-v1:{:x}", digest.finalize()))
}
/// Full decoded-content identity. Rate, layout and actual resolved release are dependencies.
pub fn source_fingerprint(samples: &[[f32; 2]], rate: u32, release: Option<&str>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"pulsedeck-decoded-stereo-v1");
    digest.update(rate.to_le_bytes());
    digest.update((samples.len() as u64).to_le_bytes());
    if let Some(release) = release {
        digest.update((release.len() as u64).to_le_bytes());
        digest.update(release.as_bytes());
    }
    let mut bytes = Vec::with_capacity(16_384);
    for chunk in samples.chunks(2048) {
        bytes.clear();
        for frame in chunk {
            for sample in frame {
                let sample = if sample.is_finite() {
                    sample.clamp(-8., 8.)
                } else {
                    0.
                };
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        digest.update(&bytes);
    }
    format!("decoded-sha256-v1:{:x}", digest.finalize())
}
struct Flight<T> {
    result: OnceLock<Result<Arc<T>, String>>,
    started: AtomicBool,
    signal: Condvar,
    waiting: Mutex<()>,
}
impl<T> Flight<T> {
    fn new() -> Self {
        Self {
            result: OnceLock::new(),
            started: AtomicBool::new(false),
            signal: Condvar::new(),
            waiting: Mutex::new(()),
        }
    }
}
type Entry<T> = Arc<Flight<T>>;
/// Completed results (including failures) are bounded; in-flight entries cannot be evicted.
pub struct StageMemo<T> {
    entries: Mutex<VecDeque<(String, Entry<T>)>>,
    capacity: usize,
}
impl<T> StageMemo<T> {
    pub const fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            capacity,
        }
    }
    pub fn get(
        &self,
        key: String,
        compute: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        self.get_cancellable(key, &|| false, compute)
    }
    pub fn get_cancellable(
        &self,
        key: String,
        cancel: &dyn Fn() -> bool,
        compute: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        if cancel() {
            return Err("Analysis waiter cancelled".into());
        }
        let entry = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Analysis stage cache poisoned")?;
            if let Some(i) = entries.iter().position(|(k, _)| k == &key) {
                let item = entries.remove(i).unwrap();
                let entry = item.1.clone();
                entries.push_back(item);
                entry
            } else {
                while entries.len() >= self.capacity {
                    if let Some(i) = entries.iter().position(|(_, v)| v.result.get().is_some()) {
                        entries.remove(i);
                    } else {
                        break;
                    }
                }
                let entry = Arc::new(Flight::new());
                entries.push_back((key, entry.clone()));
                entry
            }
        };
        if entry
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            // Computation belongs to this source/dependency, not to the originating deck.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(compute))
                .map_err(|_| "Analysis worker panicked".to_string())
                .and_then(|r| r)
                .map(Arc::new);
            let _ = entry.result.set(result);
            entry.signal.notify_all();
            if let Ok(mut entries) = self.entries.lock() {
                while entries.len() > self.capacity {
                    if let Some(i) = entries.iter().position(|(_, v)| v.result.get().is_some()) {
                        entries.remove(i);
                    } else {
                        break;
                    }
                }
            }
        } else {
            let mut wait = entry
                .waiting
                .lock()
                .map_err(|_| "Analysis wait state poisoned")?;
            while entry.result.get().is_none() {
                if cancel() {
                    return Err("Analysis waiter cancelled".into());
                }
                let result = entry
                    .signal
                    .wait_timeout(wait, std::time::Duration::from_millis(20))
                    .map_err(|_| "Analysis wait state poisoned")?;
                wait = result.0;
            }
        }
        if cancel() {
            return Err("Analysis waiter cancelled".into());
        }
        entry.result.get().expect("Completed shared stage").clone()
    }
    pub fn forget(&self, key: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(k, v)| k != key || v.result.get().is_none());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::Duration,
    };
    #[test]
    fn identity_detects_same_length_edits_and_release_change_but_ignores_path() {
        let a = vec![[0.1, 0.2]; 4096];
        let mut b = a.clone();
        b[3000][0] = 0.3;
        assert_ne!(
            source_fingerprint(&a, 48000, None),
            source_fingerprint(&b, 48000, None)
        );
        assert_ne!(
            source_fingerprint(&a, 48000, Some("spotify:track:A")),
            source_fingerprint(&a, 48000, Some("spotify:track:B"))
        );
        assert_eq!(
            source_fingerprint(&a, 48000, None),
            source_fingerprint(&a, 48000, None)
        );
    }
    #[test]
    fn concurrent_stage_is_computed_once_and_failure_retries_only_after_dependency_changes() {
        let memo = Arc::new(StageMemo::<usize>::new(4));
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (memo, calls, barrier) = (memo.clone(), calls.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    memo.get("source:grid:v1".into(), || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(20));
                        Err("unsupported".into())
                    })
                })
            })
            .collect();
        for t in threads {
            assert!(t.join().unwrap().is_err());
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(memo.get("source:grid:v1".into(), || Ok(1)).is_err());
        assert_eq!(*memo.get("source:grid:v2".into(), || Ok(2)).unwrap(), 2);
        memo.forget("source:grid:v1");
        assert_eq!(*memo.get("source:grid:v1".into(), || Ok(3)).unwrap(), 3);
    }
    #[test]
    fn cancelled_waiter_cannot_poison_shared_stage_or_strand_healthy_waiter() {
        let memo = Arc::new(StageMemo::<usize>::new(4));
        let running = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let owner = {
            let (memo, running, release, cancel) = (
                memo.clone(),
                running.clone(),
                release.clone(),
                cancel.clone(),
            );
            thread::spawn(move || {
                memo.get_cancellable("source".into(), &|| cancel.load(Ordering::Acquire), || {
                    running.store(true, Ordering::Release);
                    while !release.load(Ordering::Acquire) {
                        thread::yield_now();
                    }
                    Ok(7)
                })
            })
        };
        while !running.load(Ordering::Acquire) {
            thread::yield_now();
        }
        let healthy = {
            let memo = memo.clone();
            thread::spawn(move || memo.get("source".into(), || panic!("duplicate compute")))
        };
        cancel.store(true, Ordering::Release);
        assert!(memo
            .get_cancellable("source".into(), &|| true, || panic!(
                "cancelled caller must not compute"
            ))
            .is_err());
        release.store(true, Ordering::Release);
        assert!(owner.join().unwrap().is_err());
        assert_eq!(*healthy.join().unwrap().unwrap(), 7);
        assert_eq!(
            *memo
                .get("source".into(), || panic!("ready stage reran"))
                .unwrap(),
            7
        );
        assert!(memo.get("panic".into(), || panic!("stage panic")).is_err());
        assert!(memo.get("panic".into(), || Ok(9)).is_err());
        memo.forget("panic");
        assert_eq!(*memo.get("panic".into(), || Ok(9)).unwrap(), 9);
    }
}
