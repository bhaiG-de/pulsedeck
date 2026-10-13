use deck_engine::{AudioDeviceInfo, AudioRouting, AudioStatus};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Deserialize, Serialize)]
pub struct Preferences {
    pub device_name: String,
    pub buffer_frames: u32,
    pub routing: AudioRouting,
}
impl Preferences {
    pub fn from_status(status: &AudioStatus) -> Self {
        Self {
            device_name: status.device_name.clone(),
            buffer_frames: status.buffer_frames,
            routing: status.routing.clone(),
        }
    }
}
pub fn load(path: &Path) -> Result<Option<Preferences>, String> {
    match std::fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw)
            .map(Some)
            .map_err(|_| "Saved audio settings are invalid".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Could not read saved audio settings".into()),
    }
}
pub fn save(path: &Path, settings: &Preferences) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "Could not create audio settings folder")?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(settings).map_err(|_| "Could not encode audio settings")?,
    )
    .map_err(|_| "Could not save audio settings")?;
    std::fs::rename(temporary, path).map_err(|_| "Could not replace audio settings".into())
}
/// Audio choice is independent of the MIDI controller. A missing saved output
/// falls back to laptop/default speakers with stereo routing, never to FLX4.
pub fn choose(
    devices: &[AudioDeviceInfo],
    saved: Option<Preferences>,
) -> (Option<String>, u32, AudioRouting) {
    if let Some(saved) = saved {
        if devices
            .iter()
            .any(|device| device.name == saved.device_name)
        {
            return (Some(saved.device_name), saved.buffer_frames, saved.routing);
        }
    }
    let device = devices
        .iter()
        .find(|d| {
            d.name.to_lowercase().contains("macbook") && d.name.to_lowercase().contains("speaker")
        })
        .or_else(|| {
            devices
                .iter()
                .find(|d| d.is_default && !d.name.to_uppercase().contains("DDJ-FLX4"))
        })
        .or_else(|| {
            devices
                .iter()
                .find(|d| !d.name.to_uppercase().contains("DDJ-FLX4"))
        });
    (device.map(|d| d.name.clone()), 512, AudioRouting::default())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn devices() -> Vec<AudioDeviceInfo> {
        vec![
            AudioDeviceInfo {
                name: "DDJ-FLX4".into(),
                is_default: true,
                channels: 4,
                sample_rates: vec![48000],
            },
            AudioDeviceInfo {
                name: "MacBook Pro Speakers".into(),
                is_default: false,
                channels: 2,
                sample_rates: vec![44100, 48000],
            },
        ]
    }
    #[test]
    fn midi_connection_does_not_select_controller_audio_and_saved_speakers_keep_stereo_routing() {
        let (device, _, route) = choose(&devices(), None);
        assert_eq!(device.as_deref(), Some("MacBook Pro Speakers"));
        assert_eq!(route, AudioRouting::default());
        let saved = Preferences {
            device_name: "MacBook Pro Speakers".into(),
            buffer_frames: 256,
            routing: AudioRouting::default(),
        };
        let (device, buffer, route) = choose(&devices(), Some(saved));
        assert_eq!(device.as_deref(), Some("MacBook Pro Speakers"));
        assert_eq!(buffer, 256);
        assert_eq!(route.cue_left, None);
        let saved = Preferences {
            device_name: "Disconnected output".into(),
            buffer_frames: 256,
            routing: AudioRouting {
                cue_left: Some(2),
                cue_right: Some(3),
                ..Default::default()
            },
        };
        assert_eq!(choose(&devices(), Some(saved)).2, AudioRouting::default());
    }
    #[test]
    fn output_and_buffer_preferences_survive_restart_without_changing_recording_or_controller_settings(
    ) {
        let path =
            std::env::temp_dir().join(format!("pulsedeck-audio-pref-{}.json", std::process::id()));
        let saved = Preferences {
            device_name: "MacBook Pro Speakers".into(),
            buffer_frames: 512,
            routing: AudioRouting::default(),
        };
        save(&path, &saved).unwrap();
        let loaded = load(&path).unwrap().unwrap();
        assert_eq!(loaded.device_name, saved.device_name);
        assert_eq!(loaded.routing, saved.routing);
        assert_eq!(loaded.buffer_frames, 512);
        let _ = std::fs::remove_file(path);
    }
}
