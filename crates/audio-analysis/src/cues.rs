//! Source-owned named anchors. Slot arrays are adapters to the existing deck primitives.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Optional human-assigned arrangement role for a saved cue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueRole {
    Intro,
    BuildUp,
    Drop,
    Breakdown,
    Verse,
    Hook,
    Outro,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CuePoint {
    pub id: String,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub name: String,
    pub role: Option<CueRole>,
    pub origin: CueOrigin,
    pub modified: bool,
    pub confirmed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueOrigin {
    Manual,
    Imported,
}
impl CuePoint {
    pub fn manual(id: String, seconds: f64, name: String) -> Self {
        Self {
            id,
            start_seconds: seconds,
            end_seconds: None,
            name,
            role: None,
            origin: CueOrigin::Manual,
            modified: true,
            confirmed: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct CueBindings {
    pub main: Option<String>,
    pub hot: [Option<String>; 8],
    pub memory: [Option<String>; 10],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CueCollection {
    pub version: u32,
    pub next_id: u64,
    pub points: Vec<CuePoint>,
    pub bindings: CueBindings,
    /// Compatibility projections; persisted positions live in points after migration.
    pub cue_seconds: f64,
    pub hot_cues: [Option<f64>; 8],
    pub memory_cues: [Option<f64>; 10],
    pub annotations: BTreeMap<String, String>,
    /// Last derived labels distinguish explicit legacy renames from stale slot projections.
    #[serde(skip)]
    pub projected_annotations: BTreeMap<String, String>,
}
// The nonpersisted projection baseline is bookkeeping, not a source metadata edit.
impl PartialEq for CueCollection {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.next_id == other.next_id
            && self.points == other.points
            && self.bindings == other.bindings
            && self.cue_seconds == other.cue_seconds
            && self.hot_cues == other.hot_cues
            && self.memory_cues == other.memory_cues
            && self.annotations == other.annotations
    }
}
impl Default for CueCollection {
    fn default() -> Self {
        Self {
            version: 0,
            next_id: 1,
            points: vec![],
            bindings: CueBindings::default(),
            cue_seconds: 0.0,
            hot_cues: [None; 8],
            memory_cues: [None; 10],
            annotations: BTreeMap::new(),
            projected_annotations: BTreeMap::new(),
        }
    }
}
impl CueCollection {
    pub fn insert_manual(&mut self, time: f64, name: String) -> String {
        while self
            .points
            .iter()
            .any(|p| p.id == format!("user:{}", self.next_id))
        {
            self.next_id += 1;
        }
        let id = format!("user:{}", self.next_id);
        self.next_id += 1;
        self.points.push(CuePoint::manual(id.clone(), time, name));
        id
    }
    pub fn migrate(&mut self) {
        if self.version > 0 {
            self.project();
            return;
        }
        self.version = 2;
        if self.cue_seconds != 0.0 {
            self.bindings.main = Some(self.insert_manual(self.cue_seconds, "Main cue".into()));
        }
        for i in 0..8 {
            if let Some(t) = self.hot_cues[i] {
                let name = self
                    .annotations
                    .get(&format!("hot:{i}"))
                    .cloned()
                    .unwrap_or_else(|| format!("Hot cue {}", i + 1));
                self.bindings.hot[i] = Some(self.insert_manual(t, name));
            }
        }
        for i in 0..10 {
            if let Some(t) = self.memory_cues[i] {
                let name = self
                    .annotations
                    .get(&format!("memory:{i}"))
                    .cloned()
                    .unwrap_or_else(|| "Memory cue".into());
                self.bindings.memory[i] = Some(self.insert_manual(t, name));
            }
        }
        self.project();
    }
    pub fn project(&mut self) {
        for point in &self.points {
            if let Some(id) = point
                .id
                .strip_prefix("user:")
                .and_then(|id| id.parse::<u64>().ok())
            {
                self.next_id = self.next_id.max(id.saturating_add(1));
            }
        }
        let find = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| self.points.iter().find(|p| &p.id == id))
        };
        self.cue_seconds = find(&self.bindings.main).map_or(0., |p| p.start_seconds);
        self.hot_cues =
            std::array::from_fn(|i| find(&self.bindings.hot[i]).map(|p| p.start_seconds));
        let mut memory: Vec<String> = self
            .bindings
            .memory
            .iter()
            .flatten()
            .filter(|id| self.points.iter().any(|p| &p.id == *id))
            .cloned()
            .collect();
        memory.sort_by(|a, b| {
            self.points
                .iter()
                .find(|p| &p.id == a)
                .unwrap()
                .start_seconds
                .total_cmp(
                    &self
                        .points
                        .iter()
                        .find(|p| &p.id == b)
                        .unwrap()
                        .start_seconds,
                )
        });
        self.bindings.memory = std::array::from_fn(|i| memory.get(i).cloned());
        self.memory_cues = std::array::from_fn(|i| {
            self.bindings.memory[i]
                .as_ref()
                .and_then(|id| self.points.iter().find(|p| &p.id == id))
                .map(|p| p.start_seconds)
        });
        self.annotations
            .retain(|k, _| !k.starts_with("hot:") && !k.starts_with("memory:"));
        for (kind, ids) in [
            ("hot", self.bindings.hot.as_slice()),
            ("memory", self.bindings.memory.as_slice()),
        ] {
            for (i, id) in ids.iter().enumerate() {
                if let Some(p) = id
                    .as_ref()
                    .and_then(|id| self.points.iter().find(|p| &p.id == id))
                {
                    self.annotations
                        .insert(format!("{kind}:{i}"), p.name.clone());
                }
            }
        }
        self.projected_annotations = self.annotations.clone();
    }
    pub fn delete(&mut self, id: &str) {
        self.points.retain(|p| p.id != id);
        for b in self
            .bindings
            .hot
            .iter_mut()
            .chain(self.bindings.memory.iter_mut())
            .chain(std::iter::once(&mut self.bindings.main))
        {
            if b.as_deref() == Some(id) {
                *b = None;
            }
        }
        self.project();
    }
    /// Accept old deck/CLI slot setters by reconciling them into stable named records.
    pub fn reconcile_legacy(
        &mut self,
        hot: [Option<f64>; 8],
        memory: [Option<f64>; 10],
        main: f64,
        labels: &BTreeMap<String, String>,
    ) {
        let legacy = self.version == 0;
        let previous_labels = self.projected_annotations.clone();
        self.migrate();
        let previous_labels = if legacy {
            self.projected_annotations.clone()
        } else {
            previous_labels
        };
        let old_hot = self.hot_cues;
        let old_main = self.cue_seconds;
        let old_ids = self.bindings.memory.clone();
        let mut mapped: [Option<String>; 10] = Default::default();
        for (i, time) in memory.iter().enumerate() {
            if let Some(t) = time {
                mapped[i] = old_ids
                    .iter()
                    .flatten()
                    .find(|id| {
                        self.points
                            .iter()
                            .any(|p| &p.id == *id && (p.start_seconds - *t).abs() < 1e-6)
                    })
                    .cloned()
                    .or_else(|| {
                        Some(
                            self.insert_manual(
                                *t,
                                labels
                                    .get(&format!("memory:{i}"))
                                    .cloned()
                                    .unwrap_or_else(|| "Memory cue".into()),
                            ),
                        )
                    });
            }
        }
        for id in old_ids.iter().flatten() {
            if !mapped.iter().flatten().any(|v| v == id)
                && !self.bindings.hot.iter().flatten().any(|v| v == id)
            {
                self.delete(id);
            }
        }
        self.bindings.memory = mapped;
        // Memory slots can be sorted or compacted. An unchanged slot annotation must
        // not rename whichever stable point now occupies that numerical slot.
        for (i, id) in self.bindings.memory.iter().enumerate() {
            let key = format!("memory:{i}");
            if let (Some(id), Some(name)) = (id, labels.get(&key)) {
                if previous_labels.get(&key) != Some(name) {
                    if let Some(point) = self.points.iter_mut().find(|p| &p.id == id) {
                        if point.name != *name {
                            point.name = name.clone();
                            point.modified = true;
                            point.confirmed = true;
                        }
                    }
                }
            }
        }
        for kind in ["hot", "main"] {
            let new: Vec<Option<f64>> = match kind {
                "hot" => hot.to_vec(),
                _ => vec![Some(main)],
            };
            let old: Vec<Option<f64>> = match kind {
                "hot" => old_hot.to_vec(),
                _ => vec![Some(old_main)],
            };
            let ids: Vec<Option<String>> = match kind {
                "hot" => self.bindings.hot.to_vec(),
                _ => vec![self.bindings.main.clone()],
            };
            let mut result = ids.clone();
            for (i, time) in new.iter().enumerate() {
                let key = format!("{kind}:{i}");
                let name = labels
                    .get(&key)
                    .filter(|name| previous_labels.get(&key) != Some(*name))
                    .cloned();
                if *time == old[i] {
                    if let (Some(id), Some(name)) = (&ids[i], &name) {
                        if let Some(p) = self.points.iter_mut().find(|p| &p.id == id) {
                            if &p.name != name {
                                p.name = name.clone();
                                p.modified = true;
                                p.confirmed = true;
                            }
                        }
                    }
                    continue;
                }
                result[i] =
                    match time {
                        None => {
                            if let Some(id) = &ids[i] {
                                if kind == "hot" {
                                    self.bindings.hot[i] = None;
                                }
                                if !self.bindings.memory.iter().flatten().any(|v| v == id) {
                                    self.delete(id);
                                }
                            }
                            None
                        }
                        Some(t) => {
                            if let Some(id) = &ids[i] {
                                if let Some(p) = self.points.iter_mut().find(|p| &p.id == id) {
                                    p.start_seconds = *t;
                                    p.end_seconds = None;
                                    p.modified = true;
                                    p.confirmed = true;
                                    if let Some(name) = name {
                                        p.name = name;
                                    }
                                    Some(id.clone())
                                } else {
                                    Some(self.insert_manual(
                                        *t,
                                        name.unwrap_or_else(|| format!("{kind} cue")),
                                    ))
                                }
                            } else {
                                Some(self.insert_manual(
                                    *t,
                                    name.unwrap_or_else(|| format!("{kind} cue")),
                                ))
                            }
                        }
                    };
            }
            match kind {
                "hot" => self.bindings.hot = std::array::from_fn(|i| result[i].clone()),
                _ => self.bindings.main = result[0].clone(),
            };
        }
        self.project();
    }
    pub fn update_memory(&mut self, memory: [Option<f64>; 10]) {
        self.reconcile_legacy(
            self.hot_cues,
            memory,
            self.cue_seconds,
            &self.annotations.clone(),
        );
    }
    pub fn normalize_memory(&mut self) {
        self.migrate();
        self.project();
    }
    pub fn validate(&self, duration: f64) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for p in &self.points {
            if p.id.is_empty()
                || p.id.len() > 128
                || !ids.insert(&p.id)
                || p.name.len() > 256
                || !p.start_seconds.is_finite()
                || p.start_seconds < 0.
                || p.start_seconds > duration
                || p.end_seconds
                    .is_some_and(|t| !t.is_finite() || t < p.start_seconds || t > duration)
            {
                return Err("Invalid named cue identity, label or track-bounded span".into());
            }
        }
        for id in self
            .bindings
            .hot
            .iter()
            .chain(self.bindings.memory.iter())
            .chain(std::iter::once(&self.bindings.main))
            .flatten()
        {
            if !ids.contains(id) {
                return Err("Cue binding refers to a missing point".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_memory_rename_follows_point_identity_through_reorder_and_delete() {
        let mut cues = CueCollection::default();
        cues.memory_cues[0] = Some(10.);
        cues.memory_cues[1] = Some(20.);
        cues.annotations.insert("memory:0".into(), "Entry".into());
        cues.annotations.insert("memory:1".into(), "Exit".into());
        cues.migrate();
        let entry = cues.bindings.memory[0].clone().unwrap();
        let exit = cues.bindings.memory[1].clone().unwrap();
        for point in &mut cues.points {
            point.modified = false;
            point.confirmed = false;
        }
        let mut labels = cues.annotations.clone();
        labels.insert("memory:0".into(), "Named arrival".into());
        cues.reconcile_legacy(cues.hot_cues, cues.memory_cues, cues.cue_seconds, &labels);
        let point = cues.points.iter().find(|p| p.id == entry).unwrap();
        assert_eq!(point.name, "Named arrival");
        assert!(point.modified && point.confirmed);
        let other = cues.points.iter().find(|p| p.id == exit).unwrap();
        assert!(!other.modified && !other.confirmed);
        let mut memory = cues.memory_cues;
        memory.swap(0, 1);
        cues.reconcile_legacy(
            cues.hot_cues,
            memory,
            cues.cue_seconds,
            &cues.annotations.clone(),
        );
        assert_eq!(cues.bindings.memory[0].as_deref(), Some(entry.as_str()));
        assert_eq!(cues.annotations["memory:0"], "Named arrival");
        memory = [None; 10];
        memory[1] = Some(10.);
        cues.reconcile_legacy(
            cues.hot_cues,
            memory,
            cues.cue_seconds,
            &cues.annotations.clone(),
        );
        assert_eq!(cues.bindings.memory[0].as_deref(), Some(entry.as_str()));
        assert_eq!(cues.annotations["memory:0"], "Named arrival");
        assert!(!cues.points.iter().any(|p| p.id == exit));
    }
    #[test]
    fn unchanged_legacy_labels_do_not_undo_canonical_names() {
        let mut cues = CueCollection::default();
        cues.hot_cues[0] = Some(5.);
        cues.memory_cues[0] = Some(10.);
        cues.migrate();
        let labels = cues.annotations.clone();
        for id in [
            cues.bindings.hot[0].clone(),
            cues.bindings.memory[0].clone(),
        ]
        .into_iter()
        .flatten()
        {
            cues.points.iter_mut().find(|p| p.id == id).unwrap().name = "Canonical edit".into();
        }
        cues.reconcile_legacy(cues.hot_cues, cues.memory_cues, cues.cue_seconds, &labels);
        assert_eq!(cues.annotations["hot:0"], "Canonical edit");
        assert_eq!(cues.annotations["memory:0"], "Canonical edit");
        assert!(!serde_json::to_string(&cues)
            .unwrap()
            .contains("projected_annotations"));
    }
}
