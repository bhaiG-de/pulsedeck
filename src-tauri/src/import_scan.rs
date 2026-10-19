use std::path::{Path, PathBuf};

const MAX_TRACKS: usize = 10_000;

fn supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "wav" | "aiff" | "aif" | "mp3" | "flac" | "m4a" | "aac" | "ogg" | "alac"
            )
        })
}

pub fn audio_paths(folder: &Path) -> Result<Vec<String>, String> {
    let root = folder
        .canonicalize()
        .map_err(|error| format!("Cannot open {}: {error}", folder.display()))?;
    if !root.is_dir() {
        return Err(format!("Select a music folder: {}", folder.display()));
    }
    let mut pending = vec![root];
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("Cannot read {}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Cannot read folder entry: {error}"))?;
            let kind = entry
                .file_type()
                .map_err(|error| format!("Cannot inspect {}: {error}", entry.path().display()))?;
            let path: PathBuf = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && supported_audio(&path) {
                paths.push(path.to_string_lossy().into_owned());
                if paths.len() > MAX_TRACKS {
                    return Err(format!(
                        "Folder contains more than {MAX_TRACKS} audio tracks; choose a smaller folder"
                    ));
                }
            }
        }
    }
    paths.sort();
    if paths.is_empty() {
        Err(format!(
            "No supported audio files found in {}",
            folder.display()
        ))
    } else {
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::audio_paths;

    #[test]
    fn scans_nested_audio_without_other_files() {
        let root =
            std::env::temp_dir().join(format!("pulsedeck-import-scan-{}", std::process::id()));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("b.MP3"), []).unwrap();
        std::fs::write(nested.join("a.flac"), []).unwrap();
        std::fs::write(nested.join("cover.jpg"), []).unwrap();
        let paths = audio_paths(&root).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("b.MP3"));
        assert!(paths[1].ends_with("a.flac"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
