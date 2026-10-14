//! Convert upstream loader logs to fixed, credential-safe failure categories.
//! Raw messages (which may contain signed URLs) are never emitted or retained.
use std::sync::atomic::{AtomicU8, Ordering};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Stage {
    Unknown,
    Metadata,
    Availability,
    Format,
    Delivery,
    Key,
    Decoder,
    Seek,
    KeyTimeout,
}

fn classify(target: &str, message: &str) -> Stage {
    if target != "librespot_playback::player" {
        return Stage::Unknown;
    }
    if message.starts_with("Unable to load audio item:") {
        Stage::Metadata
    } else if message.starts_with("Track is unavailable:")
        || message == "Track should be available, but no alternatives found."
    {
        Stage::Availability
    } else if message.ends_with("is not available in any supported format") {
        Stage::Format
    } else if message.starts_with("Unable to load encrypted file:") {
        Stage::Delivery
    } else if message.starts_with("Unable to load key, continuing without decryption:") {
        if message.contains("audio key response timeout") {
            Stage::KeyTimeout
        } else {
            Stage::Key
        }
    } else if message.starts_with("Unable to read audio file:")
        || message.starts_with("PlayerTrackLoader::load_track error opening subfile:")
    {
        Stage::Decoder
    } else if message
        .starts_with("PlayerTrackLoader::load_track error seeking to starting position")
    {
        Stage::Seek
    } else {
        Stage::Unknown
    }
}

struct Diagnostics(AtomicU8);
static DIAGNOSTICS: Diagnostics = Diagnostics(AtomicU8::new(0));
impl log::Log for Diagnostics {
    fn enabled(&self, meta: &log::Metadata<'_>) -> bool {
        meta.level() <= log::Level::Warn && meta.target() == "librespot_playback::player"
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = Zeroizing::new(record.args().to_string());
        let stage = classify(record.target(), &message);
        if stage == Stage::Unknown {
            return;
        }
        // A rejected key often causes a subsequent decoder error; retain its cause.
        let _ = self
            .0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(
                    if old == Stage::Key as u8 || old == Stage::KeyTimeout as u8 {
                        old
                    } else {
                        stage as u8
                    },
                )
            });
    }
    fn flush(&self) {}
}
pub fn install() {
    if log::set_logger(&DIAGNOSTICS).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}
pub fn retryable_before_audio(frames: u64, attempts: u8) -> bool {
    let stage = DIAGNOSTICS.0.load(Ordering::Acquire);
    retryable(stage, frames, attempts)
}
fn retryable(stage: u8, frames: u64, attempts: u8) -> bool {
    frames == 0
        && attempts == 0
        && (stage == Stage::Delivery as u8
            || stage == Stage::Metadata as u8
            || stage == Stage::KeyTimeout as u8)
}
pub fn reset() {
    DIAGNOSTICS.0.store(0, Ordering::Release);
}
pub fn unavailable_error() -> String {
    let detail = match DIAGNOSTICS.0.load(Ordering::Acquire) {
        x if x == Stage::Metadata as u8 => "track metadata request failed",
        x if x == Stage::Availability as u8 => {
            "this catalog version is unavailable for the account or market"
        }
        x if x == Stage::Format as u8 => {
            "no audio format supported by the native player was supplied"
        }
        x if x == Stage::Delivery as u8 => "the audio delivery request failed",
        x if x == Stage::KeyTimeout as u8 => "the playback key response timed out after a retry",
        x if x == Stage::Key as u8 => {
            "the provider rejected or could not complete the playback key request"
        }
        x if x == Stage::Decoder as u8 => "the supplied audio could not be decoded",
        x if x == Stage::Seek as u8 => "the supplied audio could not be positioned for playback",
        _ => "the native player reported an unavailable track without a detailed cause",
    };
    format!(
        "Spotify track loading failed after native sign-in: {detail}. No partial audio was loaded."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoder_failures_do_not_get_misreported_as_an_account_problem() {
        assert_eq!(
            classify(
                "librespot_playback::player",
                "Unable to read audio file: unsupported codec"
            ),
            Stage::Decoder
        );
        assert_eq!(
            classify(
                "librespot_playback::player",
                "Unable to load encrypted file: request failed"
            ),
            Stage::Delivery
        );
        assert_eq!(
            classify(
                "librespot_playback::player",
                "<Some title> is not available in any supported format"
            ),
            Stage::Format
        );
    }
    #[test]
    fn retries_are_bounded_and_do_not_repeat_restrictions_or_append_partial_audio() {
        assert!(retryable(Stage::Delivery as u8, 0, 0));
        assert!(retryable(Stage::Metadata as u8, 0, 0));
        assert!(retryable(Stage::KeyTimeout as u8, 0, 0));
        assert_eq!(classify("librespot_playback::player", "Unable to load key, continuing without decryption: aborted: audio key response timeout"), Stage::KeyTimeout);
        assert!(!retryable(Stage::Delivery as u8, 1, 0));
        assert!(!retryable(Stage::Delivery as u8, 0, 1));
        for stage in [
            Stage::Key,
            Stage::Decoder,
            Stage::Availability,
            Stage::Format,
            Stage::Seek,
            Stage::Unknown,
        ] {
            assert!(!retryable(stage as u8, 0, 0));
        }
    }
    #[test]
    fn unrelated_and_sensitive_messages_are_not_retained() {
        assert_eq!(
            classify("librespot_core::login5", "access_token=secret"),
            Stage::Unknown
        );
        assert_eq!(
            classify(
                "librespot_playback::player",
                "Loading signed https://example.test/?token=secret"
            ),
            Stage::Unknown
        );
    }
}
