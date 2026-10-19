use lofty::{
    config::{ParseOptions, ParsingMode},
    file::TaggedFileExt,
    id3::v2::{Frame, Id3v2Tag},
    probe::Probe,
    tag::{ItemKey, ItemValue, TagType},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EmbeddedTag {
    pub format: String,
    pub key: String,
    pub standard_key: Option<String>,
    pub value: String,
    pub description: String,
    pub language: String,
}

/// Read text/URL metadata only. Neither PCM, audio properties nor artwork is decoded.
pub fn read_file_metadata(path: &Path) -> Result<Vec<EmbeddedTag>, String> {
    let options = ParseOptions::new()
        .read_properties(false)
        .parsing_mode(ParsingMode::Relaxed)
        .read_cover_art(false);
    let parsed = Probe::open(path)
        .map_err(|e| e.to_string())
        .and_then(|probe| probe.guess_file_type().map_err(|e| e.to_string()))
        .and_then(|probe| probe.options(options).read().map_err(|e| e.to_string()));
    match parsed {
        Ok(file) => Ok(collect_metadata(file)),
        Err(error) => {
            let mut fields = crate::decode::recover_file_metadata(path).unwrap_or_default();
            if let Some(file) = readable_id3_prefix(path, options) {
                fields.extend(collect_metadata(file));
            }
            if let Some(source) = crate::decode::audio_after_id3(path) {
                if let Ok(file) = Probe::new(source)
                    .guess_file_type()
                    .map_err(|e| e.to_string())
                    .and_then(|probe| probe.options(options).read().map_err(|e| e.to_string()))
                {
                    fields.extend(collect_metadata(file));
                }
            }
            if fields.is_empty() {
                return Err(error);
            }
            fields.sort();
            fields.dedup();
            Ok(fields)
        }
    }
}

/// A later damaged stacked tag must not discard a valid first tag.
fn readable_id3_prefix(path: &Path, options: ParseOptions) -> Option<lofty::file::TaggedFile> {
    use std::io::{Cursor, Read};
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0_u8; 10];
    file.read_exact(&mut header).ok()?;
    if &header[..3] != b"ID3"
        || !(2..=4).contains(&header[3])
        || header[6..].iter().any(|b| *b >= 128)
    {
        return None;
    }
    let size = header[6..]
        .iter()
        .fold(0_usize, |n, b| (n << 7) | usize::from(*b));
    if size > 16 * 1024 * 1024 {
        return None;
    }
    let mut bytes = vec![0; size + 10];
    bytes[..10].copy_from_slice(&header);
    file.read_exact(&mut bytes[10..]).ok()?;
    Probe::with_file_type(Cursor::new(bytes), lofty::file::FileType::Mpeg)
        .options(options)
        .read()
        .ok()
}

fn collect_metadata(mut file: lofty::file::TaggedFile) -> Vec<EmbeddedTag> {
    let mut fields = BTreeSet::new();
    let kinds: Vec<_> = file.tags().iter().map(|tag| tag.tag_type()).collect();
    for kind in kinds {
        let tag = file.remove(kind).expect("Tag type came from this file");
        for item in tag.items() {
            let value = match item.value() {
                ItemValue::Text(value) | ItemValue::Locator(value) => value.trim(),
                ItemValue::Binary(_) => continue,
            };
            let standard_key = match item.key() {
                ItemKey::Unknown(_) => None,
                key => Some(format!("{key:?}")),
            };
            for value in value.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                fields.insert(EmbeddedTag {
                    format: format!("{:?}", tag.tag_type()),
                    key: item
                        .key()
                        .map_key(tag.tag_type(), true)
                        .unwrap_or("")
                        .into(),
                    standard_key: standard_key.clone(),
                    value: value.into(),
                    description: item.description().into(),
                    language: String::from_utf8_lossy(item.lang())
                        .trim_matches('\0')
                        .into(),
                });
            }
        }
        if kind == TagType::Id3v2 && tag.has_format_specific_items() {
            // Generic ID3 conversion retains four-character TXXX/WXXX names
            // in its concrete remainder. Consume it without copying binary blobs.
            let raw: Id3v2Tag = tag.into();
            for frame in raw {
                let (key, value) = match frame {
                    Frame::UserText(frame) => (frame.description, frame.content),
                    Frame::UserUrl(frame) => (frame.description, frame.content),
                    Frame::Popularimeter(frame) => {
                        fields.insert(EmbeddedTag {
                            format: "Id3v2".into(),
                            key: "POPM".into(),
                            standard_key: Some("Popularimeter".into()),
                            value: frame.rating.to_string(),
                            description: frame.email,
                            language: String::new(),
                        });
                        continue;
                    }
                    _ => continue,
                };
                for value in value.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                    fields.insert(EmbeddedTag {
                        format: "Id3v2".into(),
                        key: key.clone(),
                        standard_key: None,
                        value: value.into(),
                        description: String::new(),
                        language: String::new(),
                    });
                }
            }
        }
    }
    fields.into_iter().collect()
}

/// Write explicit text edits to an atomic replacement, retaining an exact backup.
/// Reject malformed tags rather than rewriting a partially recovered container.
pub fn write_file_metadata(
    path: &Path,
    edits: &std::collections::BTreeMap<String, String>,
) -> Result<std::path::PathBuf, String> {
    use lofty::{
        file::AudioFile,
        tag::{Tag, TagItem},
    };
    use std::io::Write;
    let before = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut file = Probe::open(path)
        .map_err(|e| e.to_string())?
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .options(
            ParseOptions::new()
                .read_properties(false)
                .parsing_mode(ParsingMode::Strict),
        )
        .read()
        .map_err(|e| format!("Cannot safely write malformed metadata: {e}"))?;
    let kind = file.primary_tag_type();
    if file.tag(kind).is_none() {
        file.insert_tag(Tag::new(kind));
    }
    let tag = file
        .tag_mut(kind)
        .ok_or("File does not support editable tags")?;
    let mut custom_edits = Vec::new();
    for (name, value) in edits {
        let standard = match name.as_str() {
            "title" => Some(ItemKey::TrackTitle),
            "artist" => Some(ItemKey::TrackArtist),
            "Genre" => Some(ItemKey::Genre),
            "Comment" => Some(ItemKey::Comment),
            "AlbumTitle" => Some(ItemKey::AlbumTitle),
            "AlbumArtist" => Some(ItemKey::AlbumArtist),
            "Composer" => Some(ItemKey::Composer),
            "Label" => Some(ItemKey::Label),
            "ContentGroup" => Some(ItemKey::ContentGroup),
            "Remixer" => Some(ItemKey::Remixer),
            "Year" => Some(ItemKey::Year),
            "TrackNumber" => Some(ItemKey::TrackNumber),
            "DiscNumber" => Some(ItemKey::DiscNumber),
            "Isrc" => Some(ItemKey::Isrc),
            "CopyrightMessage" => Some(ItemKey::CopyrightMessage),
            _ => None,
        };
        let key = standard
            .or_else(|| {
                tag.items()
                    .find(|i| {
                        format!("{:?}", i.key()) == *name
                            || i.key().map_key(kind, true) == Some(name.as_str())
                    })
                    .map(|i| i.key().clone())
            })
            .unwrap_or_else(|| ItemKey::from_key(kind, name));
        // Every edit must be representable; never silently drop a custom field.
        if key.map_key(kind, true).is_none() {
            return Err(format!(
                "Metadata field {name} is unsupported by this format"
            ));
        }
        let locator = tag
            .items()
            .any(|item| item.key() == &key && matches!(item.value(), ItemValue::Locator(_)));
        if kind == TagType::Id3v2 && matches!(key, ItemKey::Unknown(_)) {
            custom_edits.push((name.clone(), value.clone(), locator));
            tag.remove_key(&key);
            continue;
        }
        if key == ItemKey::Popularimeter {
            return Err("Binary ratings cannot be written as text metadata".into());
        }
        tag.remove_key(&key);
        for value in value.split('\0').filter(|v| !v.is_empty()) {
            tag.push_unchecked(TagItem::new(
                key.clone(),
                if locator {
                    ItemValue::Locator(value.into())
                } else {
                    ItemValue::Text(value.into())
                },
            ));
        }
    }
    if !custom_edits.is_empty() {
        let mut raw: Id3v2Tag = file.remove(kind).ok_or("Missing ID3 tag")?.into();
        for (name, value, locator) in custom_edits {
            let url = locator
                || (&raw)
                    .into_iter()
                    .any(|f| matches!(f,Frame::UserUrl(f) if f.description==name));
            raw.retain(|frame| match frame {
                Frame::UserText(f) => f.description != name,
                Frame::UserUrl(f) => f.description != name,
                _ => true,
            });
            if !value.is_empty() {
                if url {
                    raw.insert(Frame::UserUrl(lofty::id3::v2::ExtendedUrlFrame::new(
                        lofty::TextEncoding::UTF8,
                        name,
                        value,
                    )));
                } else {
                    raw.insert_user_text(name, value);
                }
            }
        }
        file.insert_tag(raw.into());
    }
    let parent = path.parent().ok_or("File must have a parent folder")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = parent.join(format!(".pulsedeck-tags-{}-{stamp}", std::process::id()));
    let backup = parent.join(format!(
        "{}.pulsedeck-{stamp}.bak",
        path.file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
    ));
    let result = (|| {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let mut source = std::fs::File::open(path).map_err(|e| e.to_string())?;
        std::io::copy(&mut source, &mut output).map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        std::fs::set_permissions(&temp, before.permissions()).map_err(|e| e.to_string())?;
        file.save_to_path(&temp, lofty::config::WriteOptions::default())
            .map_err(|e| e.to_string())?;
        std::fs::File::open(&temp)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        let now = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if now.len() != before.len() || now.modified().ok() != before.modified().ok() {
            return Err("Source changed while editing metadata; retry".into());
        }
        // Hard-link the unchanged source before the atomic replacement.
        std::fs::hard_link(path, &backup).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(backup)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::{
        config::WriteOptions,
        tag::{Tag, TagExt, TagItem, TagType},
    };

    #[test]
    fn reads_standard_metadata_and_custom_fields_across_real_containers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metadata");
        for extension in ["mp3", "flac", "ogg", "m4a", "wav", "aiff"] {
            let path = root.join(format!("tags.{extension}"));
            let before = std::fs::read(&path).unwrap();
            let tags = read_file_metadata(&path).unwrap();
            assert!(
                tags.iter().any(|t| t.value == "Afro House"),
                "{extension}: {tags:?}"
            );
            assert!(
                tags.iter().any(|t| t.value == "Warm sunset café"),
                "{extension}: {tags:?}"
            );
            if ["mp3", "flac", "ogg"].contains(&extension) {
                assert!(
                    tags.iter()
                        .any(|t| t.key == "DJ_ENERGY" && t.value == "Peak 9"),
                    "{extension}: {tags:?}"
                );
            }
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn damaged_stacked_tag_retains_valid_first_tag_without_rewriting_source() {
        let path =
            std::env::temp_dir().join(format!("metadata-stacked-{}.mp3", std::process::id()));
        let mut bytes = include_bytes!("../tests/fixtures/metadata/tags.mp3").to_vec();
        let tag_end = 10
            + bytes[6..10]
                .iter()
                .fold(0_usize, |n, b| (n << 7) | usize::from(*b));
        bytes.splice(
            tag_end..tag_end,
            b"ID3\x04\0\x40\0\0\0\x0c\xff\xff\xff\xff\0\0\0\0\0\0\0\0"
                .iter()
                .copied(),
        );
        std::fs::write(&path, &bytes).unwrap();
        let fields = read_file_metadata(&path).unwrap();
        assert!(fields.iter().any(|f| f.value == "Afro House"));
        assert!(fields.iter().any(|f| f.value == "Peak 9"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_date_does_not_discard_valid_metadata() {
        let path =
            std::env::temp_dir().join(format!("metadata-bad-date-{}.mp3", std::process::id()));
        let mut bytes = include_bytes!("../tests/fixtures/metadata/tags.mp3").to_vec();
        assert_eq!(bytes[3], 4);
        let size = bytes[6..10]
            .iter()
            .fold(0_u32, |size, b| (size << 7) | u32::from(*b));
        let frame = b"TDRC\0\0\0\x0b\0\0\x03not-a-date";
        bytes.splice(10..10, frame.iter().copied());
        let size = size + frame.len() as u32;
        for (index, shift) in [21, 14, 7, 0].into_iter().enumerate() {
            bytes[6 + index] = ((size >> shift) & 127) as u8;
        }
        std::fs::write(&path, bytes).unwrap();
        let fields = read_file_metadata(&path).unwrap();
        assert!(fields.iter().any(|f| f.value == "Afro House"));
        assert!(fields
            .iter()
            .any(|f| f.key == "DJ_ENERGY" && f.value == "Peak 9"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn reads_footer_tags_multivalue_custom_id3_and_comment_descriptions() {
        let path = std::env::temp_dir().join(format!("metadata-footer-{}.mp3", std::process::id()));
        std::fs::write(&path, include_bytes!("../tests/fixtures/metadata/tags.mp3")).unwrap();
        for (kind, key, value) in [
            (TagType::Id3v1, ItemKey::Comment, "Vinyl crate"),
            (
                TagType::Ape,
                ItemKey::Unknown("DJ_CRATE".into()),
                "Afterhours",
            ),
        ] {
            let mut tag = Tag::new(kind);
            tag.push_unchecked(TagItem::new(key, ItemValue::Text(value.into())));
            tag.save_to_path(&path, WriteOptions::default()).unwrap();
        }
        let mut tag = Tag::new(TagType::Id3v2);
        let mut comment = TagItem::new(ItemKey::Comment, ItemValue::Text("Long mix out".into()));
        comment.set_description("Mixing notes".into());
        tag.push(comment);
        tag.push_unchecked(TagItem::new(
            ItemKey::Unknown("DJ_SLOT".into()),
            ItemValue::Text("Warmup\0Closing".into()),
        ));
        let mut tag: lofty::id3::v2::Id3v2Tag = tag.into();
        tag.insert(Frame::UserText(lofty::id3::v2::ExtendedTextFrame::new(
            lofty::TextEncoding::UTF8,
            "VIBE".into(),
            "Deep groove".into(),
        )));
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let fields = read_file_metadata(&path).unwrap();
        assert!(
            fields
                .iter()
                .any(|t| t.key == "VIBE" && t.value == "Deep groove"),
            "{fields:?}"
        );
        assert!(
            fields
                .iter()
                .any(|t| t.format == "Id3v1" && t.value == "Vinyl crate"),
            "{fields:?}"
        );
        assert!(
            fields
                .iter()
                .any(|t| t.format == "Ape" && t.key == "DJ_CRATE" && t.value == "Afterhours"),
            "{fields:?}"
        );
        assert!(fields
            .iter()
            .any(|t| t.description == "Mixing notes" && t.value == "Long mix out"));
        for value in ["Warmup", "Closing"] {
            assert!(
                fields
                    .iter()
                    .any(|t| t.key == "DJ_SLOT" && t.value == value),
                "{fields:?}"
            );
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn explicit_file_write_keeps_audio_private_tags_and_exact_original_backup() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metadata");
        for extension in ["mp3", "flac", "ogg", "m4a", "wav", "aiff"] {
            let path =
                std::env::temp_dir().join(format!("pd-write-{}.{}", std::process::id(), extension));
            std::fs::copy(root.join(format!("tags.{extension}")), &path).unwrap();
            if extension == "mp3" {
                let file = lofty::read_from_path(&path).unwrap();
                let mut tag: Id3v2Tag = file.primary_tag().unwrap().clone().into();
                tag.insert_user_text("VIBE".into(), "Old groove".into());
                tag.insert(Frame::Popularimeter(
                    lofty::id3::v2::PopularimeterFrame::new("rating-owner".into(), 196, 17),
                ));
                tag.insert(Frame::Private(lofty::id3::v2::PrivateFrame::new(
                    "private-owner".into(),
                    vec![0, 1, 2, 255, 0, 44],
                )));
                tag.save_to_path(&path, WriteOptions::default()).unwrap();
            }
            let before = std::fs::read(&path).unwrap();
            let audio = crate::decode::decode_file_playable(&path, None).unwrap();
            let mut edits = std::collections::BTreeMap::from([
                ("Genre".into(), "Deep House".into()),
                ("Comment".into(), "New comment café".into()),
            ]);
            if extension == "mp3" {
                edits.insert("VIBE".into(), "New groove".into());
            }
            let backup = write_file_metadata(&path, &edits).unwrap();
            assert_eq!(std::fs::read(&backup).unwrap(), before);
            let fields = read_file_metadata(&path).unwrap();
            assert!(
                fields.iter().any(|f| f.value == "Deep House"),
                "{extension}: {fields:?}"
            );
            assert!(fields.iter().any(|f| f.value == "New comment café"));
            if ["mp3", "flac", "ogg"].contains(&extension) {
                assert!(fields.iter().any(|f| f.value == "Peak 9"));
            }
            let after = crate::decode::decode_file_playable(&path, None).unwrap();
            assert!(
                audio.samples == after.samples,
                "{extension} audio changed: {} -> {} frames",
                audio.samples.len(),
                after.samples.len()
            );
            assert_eq!(audio.sample_rate, after.sample_rate);
            if extension == "mp3" {
                assert!(fields.iter().any(
                    |f| f.standard_key.as_deref() == Some("Popularimeter") && f.value == "196"
                ));
                assert!(fields
                    .iter()
                    .any(|f| f.key == "VIBE" && f.value == "New groove"));
                assert!(!fields
                    .iter()
                    .any(|f| f.key == "VIBE" && f.value == "Old groove"));
                let file = lofty::read_from_path(&path).unwrap();
                let tag: Id3v2Tag = file.primary_tag().unwrap().clone().into();
                assert!(tag.into_iter().any(|frame|matches!(frame,Frame::Private(f) if f.owner=="private-owner"&&f.private_data==[0,1,2,255,0,44])));
            }
            std::fs::remove_file(path).unwrap();
            std::fs::remove_file(backup).unwrap();
        }
    }
    #[test]
    fn edited_custom_urls_keep_their_id3_frame_type() {
        let path = std::env::temp_dir().join(format!("pd-url-write-{}.mp3", std::process::id()));
        std::fs::write(&path, include_bytes!("../tests/fixtures/metadata/tags.mp3")).unwrap();
        let file = lofty::read_from_path(&path).unwrap();
        let mut tag: Id3v2Tag = file.primary_tag().unwrap().clone().into();
        for key in ["LINK", "DJ_CUSTOM_LINK"] {
            tag.insert(Frame::UserUrl(lofty::id3::v2::ExtendedUrlFrame::new(
                lofty::TextEncoding::UTF8,
                key.into(),
                "https://example.test/original".into(),
            )));
        }
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let edits = std::collections::BTreeMap::from([
            ("LINK".into(), "https://example.test/new-short".into()),
            (
                "DJ_CUSTOM_LINK".into(),
                "https://example.test/new-long".into(),
            ),
        ]);
        let backup = write_file_metadata(&path, &edits).unwrap();
        let file = lofty::read_from_path(&path).unwrap();
        let tag: Id3v2Tag = file.primary_tag().unwrap().clone().into();
        for (key, value) in edits {
            assert!((&tag).into_iter().any(
                |frame| matches!(frame,Frame::UserUrl(f) if f.description==key&&f.content==value)
            ));
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(backup).unwrap();
    }
}
