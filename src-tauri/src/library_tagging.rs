use super::*;
use std::collections::{BTreeMap, HashSet};

type Fields = BTreeMap<String, String>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LibraryFilters {
    pub my_tags: Vec<i64>,
    pub tag_mode: String,
    pub rating_min: Option<u8>,
    pub rating_max: Option<u8>,
    pub colors: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MyTag {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub count: usize,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TagCategory {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub tags: Vec<MyTag>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TrackDetails {
    pub id: i64,
    pub revision: i64,
    pub title: String,
    pub artist: String,
    pub fields: Fields,
    pub rating: u8,
    pub color: String,
    pub my_tags: Vec<i64>,
    pub file_metadata: Vec<deck_engine::EmbeddedTag>,
    pub has_file_edits: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailsPatch {
    pub revision: i64,
    pub fields: Fields,
    pub rating: u8,
    pub color: String,
    pub my_tags: Vec<i64>,
}
fn valid_color(color: &str) -> bool {
    color.is_empty()
        || (color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
}
const XML_FIELDS: [(&str, &str); 16] = [
    ("Name", "title"),
    ("Artist", "artist"),
    ("Album", "AlbumTitle"),
    ("AlbumArtist", "AlbumArtist"),
    ("Genre", "Genre"),
    ("Comments", "Comment"),
    ("Label", "Label"),
    ("Grouping", "ContentGroup"),
    ("Composer", "Composer"),
    ("Remixer", "Remixer"),
    ("Year", "Year"),
    ("TrackNumber", "TrackNumber"),
    ("DiscNumber", "DiscNumber"),
    ("Mix", "MixArtist"),
    ("ISRC", "Isrc"),
    ("Copyright", "CopyrightMessage"),
];
fn fields(raw: &[deck_engine::EmbeddedTag]) -> Fields {
    let mut result = Fields::new();
    for f in raw {
        let key = f.standard_key.as_deref().unwrap_or(&f.key);
        let key = match key {
            "TrackTitle" => "title",
            "TrackArtist" => "artist",
            _ => key,
        };
        result
            .entry(key.into())
            .and_modify(|v| {
                if !v.split('\0').any(|x| x == f.value) {
                    v.push('\0');
                    v.push_str(&f.value);
                }
            })
            .or_insert_with(|| f.value.clone());
    }
    result
}
fn validate_fields(fields: &Fields, preserved: bool) -> Result<(), String> {
    let limit = if preserved {
        32 * 1024 * 1024
    } else {
        1024 * 1024
    };
    if (!preserved
        && (fields.len() > 512
            || fields.iter().any(|(k, v)| {
                k.trim().is_empty()
                    || k.len() > 256
                    || k.chars().any(char::is_control)
                    || v.len() > 128 * 1024
            })))
        || serde_json::to_vec(fields).map_err(|e| e.to_string())?.len() > limit
    {
        return Err("Invalid metadata fields: exceeds the editable or XML size limit".into());
    }
    Ok(())
}
impl Library {
    pub(super) fn initialize_tagging(&self) -> Result<(), String> {
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS track_info(track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,revision INTEGER NOT NULL DEFAULT 0,fields TEXT NOT NULL DEFAULT '{}',overrides TEXT NOT NULL DEFAULT '{}',xml_attributes TEXT NOT NULL DEFAULT '{}',xml_fields TEXT NOT NULL DEFAULT '{}',rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5),color TEXT NOT NULL DEFAULT '',rating_edited INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX IF NOT EXISTS track_info_rating ON track_info(rating,track_id);
        CREATE INDEX IF NOT EXISTS track_info_color_rating ON track_info(color,rating,track_id);
        DROP INDEX IF EXISTS track_info_color;
        CREATE TABLE IF NOT EXISTS tag_categories(id INTEGER PRIMARY KEY,name TEXT NOT NULL,search_name TEXT NOT NULL UNIQUE,position INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS my_tags(id INTEGER PRIMARY KEY,category_id INTEGER NOT NULL REFERENCES tag_categories(id) ON DELETE CASCADE,name TEXT NOT NULL,search_name TEXT NOT NULL,position INTEGER NOT NULL,UNIQUE(category_id,search_name));
        CREATE TABLE IF NOT EXISTS track_my_tags(track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,tag_id INTEGER NOT NULL REFERENCES my_tags(id) ON DELETE CASCADE,PRIMARY KEY(track_id,tag_id));
        CREATE INDEX IF NOT EXISTS track_my_tags_tag ON track_my_tags(tag_id,track_id);").map_err(|e|e.to_string())?;
        let has_xml_fields:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('track_info') WHERE name='xml_fields')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !has_xml_fields {
            self.db
                .execute(
                    "ALTER TABLE track_info ADD COLUMN xml_fields TEXT NOT NULL DEFAULT '{}'",
                    [],
                )
                .map_err(|e| e.to_string())?;
        }
        let ids = self
            .db
            .prepare("SELECT id FROM tracks WHERE id NOT IN(SELECT track_id FROM track_info)")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        for id in ids {
            self.refresh_details(id)?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub(super) fn refresh_details(&self, id: i64) -> Result<(), String> {
        let (title, artist, raw): (String, String, String) = self
            .db
            .query_row(
                "SELECT title,artist,file_metadata FROM tracks WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        self.db
            .execute(
                "INSERT OR IGNORE INTO track_info(track_id) VALUES(?1)",
                [id],
            )
            .map_err(|e| e.to_string())?;
        let (overrides, attrs, xml_fields): (String, String, String) = self
            .db
            .query_row(
                "SELECT overrides,xml_attributes,xml_fields FROM track_info WHERE track_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let raw: Vec<deck_engine::EmbeddedTag> =
            serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let mut effective = fields(&raw);
        effective.insert("title".into(), title.clone());
        effective.insert("artist".into(), artist.clone());
        let attrs: Fields = serde_json::from_str(&attrs).map_err(|e| e.to_string())?;
        for (xml, key) in XML_FIELDS {
            if let Some(v) = attrs.get(xml) {
                effective.insert(key.into(), v.clone());
            }
        }
        effective.extend(serde_json::from_str::<Fields>(&xml_fields).map_err(|e| e.to_string())?);
        let overrides: Fields = serde_json::from_str(&overrides).map_err(|e| e.to_string())?;
        effective.extend(overrides.clone());
        if let Some(raw_rating) = raw
            .iter()
            .find(|f| f.format == "Id3v2" && f.standard_key.as_deref() == Some("Popularimeter"))
            .and_then(|f| f.value.parse::<u8>().ok())
        {
            let stars = match raw_rating {
                0 => 0,
                1..=63 => 1,
                64..=127 => 2,
                128..=195 => 3,
                196..=254 => 4,
                255 => 5,
            };
            self.db
                .execute(
                    "UPDATE track_info SET rating=?1 WHERE track_id=?2 AND rating_edited=0",
                    params![stars, id],
                )
                .map_err(|e| e.to_string())?;
        }
        let search = raw
            .iter()
            .flat_map(|f| [f.key.as_str(), f.description.as_str()])
            .chain(effective.iter().flat_map(|(k, v)| [k.as_str(), v.as_str()]))
            .collect::<Vec<_>>()
            .join(" ")
            .replace('\0', " ")
            .to_lowercase();
        self.db.execute("UPDATE track_info SET revision=revision+(fields IS NOT ?1),fields=?1 WHERE track_id=?2",params![serde_json::to_string(&effective).map_err(|e|e.to_string())?,id]).map_err(|e|e.to_string())?;
        // Only explicit name edits replace the main display fields.
        self.db
            .execute(
                "UPDATE tracks SET title=?1,artist=?2,metadata_search=?3 WHERE id=?4",
                params![
                    overrides.get("title").unwrap_or(&title),
                    overrides.get("artist").unwrap_or(&artist),
                    search,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn track_details(&self, id: i64) -> Result<TrackDetails, String> {
        let mut d=self.db.query_row("SELECT t.id,i.revision,t.title,t.artist,i.fields,i.rating,i.color,t.file_metadata,i.overrides<>'{}' FROM tracks t JOIN track_info i ON i.track_id=t.id WHERE t.id=?1",[id],|r|Ok(TrackDetails{id:r.get(0)?,revision:r.get(1)?,title:r.get(2)?,artist:r.get(3)?,fields:serde_json::from_str(&r.get::<_,String>(4)?).unwrap_or_default(),rating:r.get(5)?,color:r.get(6)?,my_tags:vec![],file_metadata:serde_json::from_str(&r.get::<_,String>(7)?).unwrap_or_default(),has_file_edits:r.get(8)?})).map_err(|e|e.to_string())?;
        d.my_tags = self
            .db
            .prepare("SELECT tag_id FROM track_my_tags WHERE track_id=?1 ORDER BY tag_id")
            .map_err(|e| e.to_string())?
            .query_map([id], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?;
        Ok(d)
    }
    pub fn edit_details(&self, id: i64, patch: DetailsPatch) -> Result<TrackDetails, String> {
        validate_fields(&patch.fields, false)?;
        if patch.rating > 5 || !valid_color(&patch.color) || patch.my_tags.len() > 512 {
            return Err("Invalid rating, color or My Tags".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let old = self.track_details(id)?;
        if old.revision != patch.revision {
            return Err("Metadata revision conflict; reload before saving".into());
        }
        let prior: String = tx
            .query_row(
                "SELECT overrides FROM track_info WHERE track_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut overrides: Fields = serde_json::from_str(&prior).map_err(|e| e.to_string())?;
        // The UI sends a patch; unrelated custom fields and multi-value originals survive.
        for (key, value) in patch.fields {
            if old.fields.get(&key) != Some(&value) {
                overrides.insert(key, value);
            }
        }
        tx.execute("UPDATE track_info SET revision=revision+1,rating_edited=CASE WHEN rating<>?2 THEN 1 ELSE rating_edited END,overrides=?1,rating=?2,color=?3 WHERE track_id=?4",params![serde_json::to_string(&overrides).map_err(|e|e.to_string())?,patch.rating,patch.color.to_uppercase(),id]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM track_my_tags WHERE track_id=?1", [id])
            .map_err(|e| e.to_string())?;
        for tag in patch.my_tags.into_iter().collect::<HashSet<_>>() {
            tx.execute("INSERT INTO track_my_tags VALUES(?1,?2)", params![id, tag])
                .map_err(|e| e.to_string())?;
        }
        self.refresh_details(id)?;
        tx.execute("UPDATE suggest_features SET title=(SELECT title FROM tracks WHERE id=?1),artist=(SELECT artist FROM tracks WHERE id=?1) WHERE track_id=?1",[id]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.features.borrow_mut().take();
        self.track_details(id)
    }
    pub fn tag_categories(&self) -> Result<Vec<TagCategory>, String> {
        let mut categories = self
            .db
            .prepare("SELECT id,name,position FROM tag_categories ORDER BY position,id")
            .map_err(|e| e.to_string())?
            .query_map([], |r| {
                Ok(TagCategory {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    position: r.get(2)?,
                    tags: vec![],
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let rows=self.db.prepare("SELECT m.category_id,m.id,m.name,m.position,(SELECT count(*) FROM track_my_tags x WHERE x.tag_id=m.id) FROM my_tags m ORDER BY m.position,m.id").map_err(|e|e.to_string())?.query_map([],|r|Ok((r.get::<_,i64>(0)?,MyTag{id:r.get(1)?,name:r.get(2)?,position:r.get(3)?,count:r.get(4)?}))).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
        for (category, tag) in rows {
            if let Some(c) = categories.iter_mut().find(|c| c.id == category) {
                c.tags.push(tag);
            }
        }
        Ok(categories)
    }
    pub fn edit_taxonomy(
        &self,
        kind: &str,
        id: Option<i64>,
        category: Option<i64>,
        name: &str,
        position: Option<i64>,
        delete: bool,
    ) -> Result<Vec<TagCategory>, String> {
        let table = match kind {
            "category" => "tag_categories",
            "tag" => "my_tags",
            _ => return Err("Invalid taxonomy kind".into()),
        };
        let name = name.trim();
        if !delete
            && (name.is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control))
        {
            return Err("Choose a tag name between 1 and 128 characters".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        if let Some(id) = id {
            let changed=if delete {tx.execute(&format!("DELETE FROM {table} WHERE id=?1"),[id])} else {tx.execute(&format!("UPDATE {table} SET name=?1,search_name=?2,position=coalesce(?3,position) WHERE id=?4"),params![name,name.to_lowercase(),position,id])}.map_err(|e|e.to_string())?;
            if changed == 0 {
                return Err("Tag or category not found".into());
            }
        } else if delete {
            return Err("Invalid delete: choose an ID".into());
        } else if kind == "category" {
            tx.execute("INSERT INTO tag_categories(name,search_name,position) VALUES(?1,?2,coalesce(?3,(SELECT coalesce(max(position),-1)+1 FROM tag_categories)))",params![name,name.to_lowercase(),position]).map_err(|e|e.to_string())?;
        } else {
            let category = category.ok_or("Choose a category")?;
            tx.execute("INSERT INTO my_tags(category_id,name,search_name,position) VALUES(?1,?2,?3,coalesce(?4,(SELECT coalesce(max(position),-1)+1 FROM my_tags WHERE category_id=?1)))",params![category,name,name.to_lowercase(),position]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        self.tag_categories()
    }
    pub(super) fn filter_sql(
        &self,
        filters: &LibraryFilters,
        values: &mut Vec<rusqlite::types::Value>,
    ) -> Result<Option<String>, String> {
        if filters.my_tags.len() > 512
            || !matches!(filters.tag_mode.as_str(), "" | "and" | "or")
            || filters.rating_min.is_some_and(|x| x > 5)
            || filters.rating_max.is_some_and(|x| x > 5)
            || filters.colors.iter().any(|s| !valid_color(s))
            || filters.colors.len() > 64
        {
            return Err("Invalid library filters".into());
        }
        if let (Some(min), Some(max)) = (filters.rating_min, filters.rating_max) {
            if min > max {
                return Err("Invalid rating range".into());
            }
        }
        let mut conditions = Vec::new();
        for (op, value) in [(">=", filters.rating_min), ("<=", filters.rating_max)] {
            if let Some(v) = value {
                values.push(i64::from(v).into());
                conditions.push(format!("rating{op}?{}", values.len()));
            }
        }
        if !filters.colors.is_empty() {
            let mut p = vec![];
            for color in &filters.colors {
                values.push(color.to_uppercase().into());
                p.push(format!("?{}", values.len()));
            }
            conditions.push(format!("color IN({})", p.join(",")));
        }
        let mut source = "track_info i".to_owned();
        let id = "i.track_id";
        let mut tags = filters.my_tags.clone();
        tags.sort_unstable();
        tags.dedup();
        if !tags.is_empty() {
            let mut p = Vec::new();
            for tag in tags {
                values.push(tag.into());
                p.push(format!("?{}", values.len()));
            }
            // Start with a compact indexed classification table, not full tracks.
            if conditions.is_empty() {
                source = "track_my_tags i".into();
                if filters.tag_mode == "or" {
                    return Ok(Some(format!(
                        "SELECT DISTINCT track_id AS id FROM {source} WHERE tag_id IN({})",
                        p.join(",")
                    )));
                }
                conditions.push(format!("i.tag_id={}", p.remove(0)));
            }
            if !p.is_empty() {
                if filters.tag_mode == "or" {
                    conditions.push(format!(
                        "EXISTS(SELECT 1 FROM track_my_tags WHERE track_id={id} AND tag_id IN({}))",
                        p.join(",")
                    ));
                } else if p.len() <= 62 {
                    // Stay below SQLite's 64-table join limit, including BPM's tracks join.
                    for (index, tag) in p.into_iter().enumerate() {
                        source.push_str(&format!(" JOIN track_my_tags m{index} ON m{index}.track_id={id} AND m{index}.tag_id={tag}"));
                    }
                } else {
                    for tag in p {
                        conditions.push(format!("EXISTS(SELECT 1 FROM track_my_tags WHERE track_id={id} AND tag_id={tag})"));
                    }
                }
            }
        }
        Ok((!conditions.is_empty()).then(|| {
            format!(
                "SELECT {id} AS id FROM {source} WHERE {}",
                conditions.join(" AND ")
            )
        }))
    }

    pub fn import_xml_cues(&self, record: &TrackRecord, entry: &XmlTrack) -> Result<(), String> {
        let (revision, mut cues) = self.source_cues(&record.path)?;
        let original = cues.clone();
        if let Some(json) = entry.attributes.get("PulseDeckCues") {
            let mut imported: SourceCues = serde_json::from_str(json)
                .map_err(|e| format!("Invalid PulseDeck XML cues: {e}"))?;
            imported.migrate();
            imported.validate(record.duration_seconds)?;
            let mut ids = HashMap::new();
            for point in &imported.points {
                let existing = cues
                    .points
                    .iter()
                    .find(|p| {
                        p.start_seconds == point.start_seconds
                            && p.end_seconds == point.end_seconds
                            && p.name == point.name
                    })
                    .map(|p| p.id.clone());
                let id = existing.unwrap_or_else(|| {
                    let mut point = point.clone();
                    if cues.points.iter().any(|p| p.id == point.id) {
                        point.id = cues.insert_manual(point.start_seconds, point.name.clone());
                        if let Some(p) = cues.points.iter_mut().find(|p| p.id == point.id) {
                            *p = point.clone();
                        }
                    } else {
                        cues.points.push(point.clone());
                    }
                    point.id
                });
                ids.insert(point.id.clone(), id);
            }
            for (old, new) in cues
                .bindings
                .hot
                .iter_mut()
                .zip(imported.bindings.hot.iter())
            {
                if old.is_none() {
                    *old = new.as_ref().and_then(|id| ids.get(id).cloned());
                }
            }
            for id in imported
                .bindings
                .memory
                .iter()
                .flatten()
                .filter_map(|id| ids.get(id))
            {
                if !cues.bindings.memory.iter().flatten().any(|v| v == id) {
                    if let Some(slot) = cues.bindings.memory.iter_mut().find(|v| v.is_none()) {
                        *slot = Some(id.clone());
                    }
                }
            }
            if cues.bindings.main.is_none() {
                cues.bindings.main = imported
                    .bindings
                    .main
                    .as_ref()
                    .and_then(|id| ids.get(id).cloned());
            }
            cues.next_id = cues.next_id.max(imported.next_id);
        }
        for mark in &entry.marks {
            if mark.kind != 0
                || !mark.start.is_finite()
                || mark.start < 0.
                || mark.start > record.duration_seconds
            {
                continue;
            }
            let existing = cues
                .points
                .iter()
                .find(|p| {
                    (p.start_seconds - mark.start).abs() < 1e-6
                        && (mark.name.is_empty() || p.name == mark.name)
                })
                .map(|p| p.id.clone());
            let id = existing.unwrap_or_else(|| {
                let id = cues.insert_manual(
                    mark.start,
                    if mark.name.is_empty() {
                        "Imported cue".into()
                    } else {
                        mark.name.clone()
                    },
                );
                if let Some(point) = cues.points.iter_mut().find(|p| p.id == id) {
                    point.origin = audio_analysis::CueOrigin::Imported;
                }
                id
            });
            if (0..8).contains(&mark.number) {
                let binding = &mut cues.bindings.hot[mark.number as usize];
                if binding.is_none() {
                    *binding = Some(id);
                }
            } else if mark.number == -1 && !cues.bindings.memory.iter().flatten().any(|v| *v == id)
            {
                if let Some(slot) = cues.bindings.memory.iter_mut().find(|x| x.is_none()) {
                    *slot = Some(id);
                }
            }
        }
        if cues != original {
            cues.project();
            self.save_source_cues(&record.path, &cues, Some(revision))?;
        }
        let bpm = entry
            .bpm
            .or_else(|| entry.tempos.first().map(|t| t.bpm))
            .filter(|b| b.is_finite() && (30.0..=300.0).contains(b));
        let offset = entry
            .tempos
            .first()
            .filter(|t| t.start.is_finite() && t.start >= 0.)
            .map(|t| t.start);
        let bpm = bpm.filter(|value| *value != record.bpm);
        let offset = offset.filter(|value| *value != record.beatgrid_offset_seconds);
        if bpm.is_some() || offset.is_some() {
            self.metadata(&record.path, bpm, None, offset)?;
        }
        Ok(())
    }
    pub fn metadata_write_edits(&self, id: i64, revision: i64) -> Result<(String, Fields), String> {
        let d = self.track_details(id)?;
        if d.revision != revision {
            return Err("Metadata revision conflict; reload before writing".into());
        }
        let (path,edits):(String,String)=self.db.query_row("SELECT t.path,i.overrides FROM tracks t JOIN track_info i ON i.track_id=t.id WHERE t.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
        let edits: Fields = serde_json::from_str(&edits).map_err(|e| e.to_string())?;
        if edits.is_empty() {
            return Err("No text metadata edits to write".into());
        }
        // Classification stays in the library/XML; text writeback changes explicit edits only.
        Ok((path, edits))
    }
    pub fn import_xml_details(&self, id: i64, attrs: &Fields) -> Result<(), String> {
        validate_fields(attrs, true)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let current: String = tx
            .query_row(
                "SELECT xml_attributes FROM track_info WHERE track_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut merged: Fields = serde_json::from_str(&current).map_err(|e| e.to_string())?;
        merged.extend(attrs.clone());
        let rating = attrs
            .get("Rating")
            .map(|v| v.parse::<u16>().map(|x| ((x.min(255) + 25) / 51) as u8))
            .transpose()
            .map_err(|_| "Invalid XML rating")?;
        let color = attrs
            .get("Colour")
            .map(|s| {
                let s = s.trim().to_uppercase();
                if let Some(v) = s.strip_prefix("0X") {
                    format!("#{v}")
                } else if s.len() == 6 {
                    format!("#{s}")
                } else {
                    s
                }
            })
            .filter(|s| valid_color(s));
        tx.execute("UPDATE track_info SET xml_attributes=?1,rating_edited=CASE WHEN ?2 IS NULL THEN rating_edited ELSE 1 END,rating=coalesce(?2,rating),color=coalesce(?3,color),revision=revision+1 WHERE track_id=?4",params![serde_json::to_string(&merged).map_err(|e|e.to_string())?,rating,color,id]).map_err(|e|e.to_string())?;
        if let Some(extension) = attrs.get("PulseDeckTags") {
            #[derive(Deserialize)]
            struct Extension {
                #[serde(default)]
                labels: Vec<String>,
                #[serde(default)]
                categories: Vec<ImportedCategory>,
                #[serde(default)]
                fields: Fields,
            }
            #[derive(Deserialize)]
            struct ImportedCategory {
                name: String,
                tags: Vec<String>,
            }
            let extension: Extension = serde_json::from_str(extension)
                .map_err(|e| format!("Invalid PulseDeck XML tags: {e}"))?;
            validate_fields(&extension.fields, true)?;
            let previous: String = tx
                .query_row(
                    "SELECT xml_fields FROM track_info WHERE track_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let mut edits: Fields = serde_json::from_str(&previous).map_err(|e| e.to_string())?;
            for (key, value) in extension.fields {
                edits.insert(key, value);
            }
            tx.execute(
                "UPDATE track_info SET xml_fields=?1 WHERE track_id=?2",
                params![
                    serde_json::to_string(&edits).map_err(|e| e.to_string())?,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
            let mut labels: Vec<String> = serde_json::from_str(
                &tx.query_row("SELECT tags FROM tracks WHERE id=?1", [id], |r| {
                    r.get::<_, String>(0)
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let previous_labels = labels.clone();
            for label in extension.labels {
                if !labels
                    .iter()
                    .any(|v| v.to_lowercase() == label.to_lowercase())
                {
                    labels.push(label);
                }
            }
            if labels != previous_labels {
                self.set_tags(id, labels)?;
            }
            if extension.categories.len() > 128 {
                return Err("Invalid category count".into());
            }
            for c in extension.categories {
                if c.tags.len() > 512 {
                    return Err("Invalid tag count".into());
                }
                let category = tx
                    .query_row(
                        "SELECT id FROM tag_categories WHERE search_name=?1",
                        [c.name.trim().to_lowercase()],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                let category = match category {
                    Some(v) => v,
                    None => {
                        self.edit_taxonomy_in_import("category", None, &c.name)?;
                        tx.last_insert_rowid()
                    }
                };
                for tag in c.tags {
                    let tag_id = tx
                        .query_row(
                            "SELECT id FROM my_tags WHERE category_id=?1 AND search_name=?2",
                            params![category, tag.trim().to_lowercase()],
                            |r| r.get::<_, i64>(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    let tag_id = match tag_id {
                        Some(v) => v,
                        None => {
                            self.edit_taxonomy_in_import("tag", Some(category), &tag)?;
                            tx.last_insert_rowid()
                        }
                    };
                    tx.execute(
                        "INSERT OR IGNORE INTO track_my_tags VALUES(?1,?2)",
                        params![id, tag_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        self.refresh_details(id)?;
        tx.commit().map_err(|e| e.to_string())
    }
    fn edit_taxonomy_in_import(
        &self,
        kind: &str,
        category: Option<i64>,
        name: &str,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control) {
            return Err("Invalid XML tag name".into());
        }
        let sql = if kind == "category" {
            "INSERT INTO tag_categories(name,search_name,position) VALUES(?1,?2,(SELECT coalesce(max(position),-1)+1 FROM tag_categories))"
        } else {
            "INSERT INTO my_tags(name,search_name,position,category_id) VALUES(?1,?2,(SELECT coalesce(max(position),-1)+1 FROM my_tags WHERE category_id=?3),?3)"
        };
        if kind == "category" {
            self.db.execute(sql, params![name, name.to_lowercase()])
        } else {
            self.db
                .execute(sql, params![name, name.to_lowercase(), category])
        }
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn import_taxonomy_json(&self, json: &str) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Category {
            name: String,
            tags: Vec<Tag>,
        }
        #[derive(Deserialize)]
        struct Tag {
            name: String,
        }
        let categories: Vec<Category> =
            serde_json::from_str(json).map_err(|e| format!("Invalid taxonomy XML: {e}"))?;
        if categories.len() > 128 || categories.iter().any(|c| c.tags.len() > 512) {
            return Err("Invalid XML taxonomy size".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        for c in categories {
            let id = tx
                .query_row(
                    "SELECT id FROM tag_categories WHERE search_name=?1",
                    [c.name.trim().to_lowercase()],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let id = match id {
                Some(id) => id,
                None => {
                    self.edit_taxonomy_in_import("category", None, &c.name)?;
                    tx.last_insert_rowid()
                }
            };
            for tag in c.tags {
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM my_tags WHERE category_id=?1 AND search_name=?2)",params![id,tag.name.trim().to_lowercase()],|r|r.get(0)).map_err(|e|e.to_string())?;
                if !exists {
                    self.edit_taxonomy_in_import("tag", Some(id), &tag.name)?;
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub(super) fn all_xml_details(&self) -> Result<HashMap<i64, Fields>, String> {
        let mut assignments: HashMap<i64, BTreeMap<String, Vec<String>>> = HashMap::new();
        let mut stmt=self.db.prepare("SELECT x.track_id,c.name,m.name FROM track_my_tags x JOIN my_tags m ON m.id=x.tag_id JOIN tag_categories c ON c.id=m.category_id ORDER BY c.position,c.id,m.position,m.id").map_err(|e|e.to_string())?;
        for row in stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
        {
            let (track, category, tag) = row.map_err(|e| e.to_string())?;
            assignments
                .entry(track)
                .or_default()
                .entry(category)
                .or_default()
                .push(tag);
        }
        let mut stmt=self.db.prepare("SELECT i.track_id,i.fields,i.xml_attributes,i.rating,i.color,t.tags FROM track_info i JOIN tracks t ON t.id=i.track_id").map_err(|e|e.to_string())?;
        let mut result = HashMap::new();
        for row in stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u8>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?
        {
            let (id, fields, raw, rating, color, labels) = row.map_err(|e| e.to_string())?;
            let fields: Fields = serde_json::from_str(&fields).map_err(|e| e.to_string())?;
            let mut attrs: Fields = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            for (xml, key) in XML_FIELDS {
                if let Some(value) = fields.get(key) {
                    attrs.insert(xml.into(), value.replace('\0', "; "));
                }
            }
            attrs.insert("Rating".into(), (u16::from(rating) * 51).to_string());
            attrs.insert("Colour".into(), color);
            let categories: Vec<_> = assignments
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, tags)| serde_json::json!({"name":name,"tags":tags}))
                .collect();
            attrs.insert("PulseDeckTags".into(),serde_json::json!({"labels":serde_json::from_str::<Vec<String>>(&labels).map_err(|e|e.to_string())?,"categories":categories,"fields":fields}).to_string());
            result.insert(id, attrs);
        }
        Ok(result)
    }
}
