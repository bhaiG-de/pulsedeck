//! Lazy local cover thumbnails. No image bytes are sent with library pages.
use crate::AppState;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path, sync::Mutex};
use tauri::{http::Response, Manager};

// Bound concurrent image decoding and cache maintenance independently of audio.
static WORKER: Mutex<()> = Mutex::new(());
const CACHE_LIMIT: usize = 512;

pub fn response(app: &tauri::AppHandle, uri_path: &str) -> Response<Vec<u8>> {
    let result = (|| -> Result<Vec<u8>, String> {
        let id: i64 = uri_path
            .trim_start_matches('/')
            .parse()
            .map_err(|_| "Invalid track id")?;
        let state = app.state::<AppState>();
        let path = state
            .library
            .lock()
            .map_err(|_| "Library unavailable")?
            .path_for_id(id)?;
        let path = Path::new(&path);
        let file = path.metadata().map_err(|e| e.to_string())?;
        let mut digest = Sha256::new();
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update(file.len().to_le_bytes());
        digest.update(
            file.modified()
                .map_err(|e| e.to_string())?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
                .to_le_bytes(),
        );
        let key = format!("{:x}", digest.finalize());
        let cache = state.data_dir.join("cover-thumbnails-v1");
        std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let target = cache.join(format!("{key}.jpg"));
        if let Ok(bytes) = std::fs::read(&target) {
            return if bytes.is_empty() {
                Err("No embedded cover".into())
            } else {
                Ok(bytes)
            };
        }
        let _worker = WORKER.lock().map_err(|_| "Artwork unavailable")?;
        if let Ok(bytes) = std::fs::read(&target) {
            return if bytes.is_empty() {
                Err("No embedded cover".into())
            } else {
                Ok(bytes)
            };
        }
        let bytes = match deck_engine::embedded_artwork(path)? {
            Some(cover) => thumbnail(&cover).unwrap_or_default(),
            None => Vec::new(),
        };
        std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
        // Persistent cache is bounded, including negative lookups.
        let mut entries: Vec<_> = std::fs::read_dir(&cache)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        if entries.len() > CACHE_LIMIT {
            entries.sort_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()));
            let remove = entries.len() - CACHE_LIMIT;
            for entry in entries.into_iter().take(remove) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        if bytes.is_empty() {
            Err("No embedded cover".into())
        } else {
            Ok(bytes)
        }
    })();
    match result {
        Ok(bytes) => Response::builder()
            .header("Content-Type", "image/jpeg")
            .header("Cache-Control", "private, max-age=60")
            .body(bytes)
            .unwrap(),
        Err(_) => Response::builder()
            .status(404)
            .header("Cache-Control", "private, max-age=60")
            .body(Vec::new())
            .unwrap(),
    }
}
fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let cover = reader
        .decode()
        .map_err(|e| e.to_string())?
        .thumbnail(128, 128)
        .to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 80)
        .encode_image(&cover)
        .map_err(|e| e.to_string())?;
    Ok(output)
}
#[cfg(test)]
mod tests {
    #[test]
    fn cover_thumbnail_is_small_and_rejects_non_images() {
        let cover = image::RgbImage::from_pixel(1024, 512, image::Rgb([200, 40, 20]));
        let mut input = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut input)
            .encode_image(&cover)
            .unwrap();
        let bytes = super::thumbnail(&input).unwrap();
        let output = image::load_from_memory(&bytes).unwrap();
        assert_eq!((output.width(), output.height()), (128, 64));
        assert!(bytes.len() < 4096);
        assert!(super::thumbnail(b"not an image").is_err());
    }
}
