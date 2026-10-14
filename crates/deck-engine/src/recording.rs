use rtrb::{Consumer, Producer, RingBuffer};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub active: bool,
    pub path: Option<String>,
    pub sample_rate: u32,
    pub frames_written: u64,
    pub dropped_frames: u64,
    pub error: Option<String>,
}
pub(crate) struct RecordingStats {
    pub active: AtomicBool,
    pub written: AtomicU64,
    pub dropped: AtomicU64,
    pub error: Mutex<Option<String>>,
    pub path: String,
    pub rate: u32,
}
impl RecordingStats {
    pub fn status(&self) -> RecordingStatus {
        RecordingStatus {
            active: self.active.load(Ordering::Acquire),
            path: Some(self.path.clone()),
            sample_rate: self.rate,
            frames_written: self.written.load(Ordering::Relaxed),
            dropped_frames: self.dropped.load(Ordering::Relaxed),
            error: self.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }
}
pub(crate) struct Recorder {
    pub producer: Producer<[f32; 2]>,
    pub stats: Arc<RecordingStats>,
}
pub(crate) struct RecordingJob {
    pub stats: Arc<RecordingStats>,
    pub worker: Option<JoinHandle<()>>,
}
pub(crate) fn create_with_policy(
    path: &Path,
    rate: u32,
    overwrite: bool,
) -> Result<(Recorder, RecordingJob), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(!overwrite)
        .create(overwrite)
        .truncate(overwrite)
        .open(path)
        .map_err(|e| format!("Cannot create recording (choose a new .wav filename): {e}"))?;
    header(&mut file, rate, 0).map_err(|e| format!("Cannot initialize WAV: {e}"))?;
    let (producer, consumer) = RingBuffer::new((rate as usize * 4).max(65536));
    let stats = Arc::new(RecordingStats {
        active: AtomicBool::new(true),
        written: AtomicU64::new(0),
        dropped: AtomicU64::new(0),
        error: Mutex::new(None),
        path: path.to_string_lossy().into(),
        rate,
    });
    let writer_stats = stats.clone();
    let worker = thread::Builder::new()
        .name("dj-recording-writer".into())
        .spawn(move || write_recording(file, consumer, writer_stats))
        .map_err(|e| format!("Cannot start recording writer: {e}"))?;
    Ok((
        Recorder {
            producer,
            stats: stats.clone(),
        },
        RecordingJob {
            stats,
            worker: Some(worker),
        },
    ))
}
fn header(file: &mut File, rate: u32, frames: u64) -> std::io::Result<()> {
    let bytes = frames
        .checked_mul(8)
        .filter(|x| *x <= u32::MAX as u64 - 36)
        .ok_or_else(|| std::io::Error::other("WAV exceeds 4 GiB; stop recording sooner"))?
        as u32;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 8).to_le_bytes())?;
    file.write_all(&8_u16.to_le_bytes())?;
    file.write_all(&32_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    Ok(())
}
fn write_recording(mut file: File, mut consumer: Consumer<[f32; 2]>, stats: Arc<RecordingStats>) {
    let mut bytes = Vec::with_capacity(32768);
    let mut frames = 0_u64;
    let result = (|| -> std::io::Result<()> {
        loop {
            bytes.clear();
            let mut batch = 0_u64;
            while batch < 4096 {
                let Ok(frame) = consumer.pop() else {
                    break;
                };
                for sample in frame {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                batch += 1;
            }
            if batch > 0 {
                file.write_all(&bytes)?;
                frames += batch;
                stats.written.store(frames, Ordering::Relaxed);
                if frames > (u32::MAX as u64 - 36) / 8 - 4096 {
                    return Err(std::io::Error::other(
                        "Recording reached the WAV 4 GiB limit",
                    ));
                }
            }
            if !stats.active.load(Ordering::Acquire) && consumer.is_empty() {
                break;
            }
            if batch == 0 {
                thread::sleep(Duration::from_millis(5));
            }
        }
        header(&mut file, stats.rate, frames)?;
        file.flush()?;
        Ok(())
    })();
    if let Err(error) = result {
        *stats.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.to_string());
        let _ = header(&mut file, stats.rate, frames);
        let _ = file.flush();
    }
    stats.active.store(false, Ordering::Release);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_writes_decodable_stereo_float_wav() {
        let path = std::env::temp_dir().join(format!("deck-recording-{}.wav", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (mut recorder, mut job) = create_with_policy(&path, 48000, false).unwrap();
        for _ in 0..4800 {
            recorder.producer.push([0.2, -0.3]).unwrap();
        }
        recorder.stats.active.store(false, Ordering::Release);
        drop(recorder);
        job.worker.take().unwrap().join().unwrap();
        let track = crate::decode::decode_file(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(track.samples.len(), 4800);
        assert!((track.samples[1024][0] - 0.2).abs() < 1e-6);
        assert!((track.samples[1024][1] + 0.3).abs() < 1e-6);
        assert_eq!(job.stats.status().frames_written, 4800);
        assert_eq!(job.stats.status().dropped_frames, 0);
    }
}
