//! Song sections on the beat grid, their labels, and the automatic cue plan.
//!
//! Each full bar from the grid's downbeat gets a feature vector from the beat
//! model's log-mel spectrogram: 16 timbre bands plus overall, bass and treble
//! level and bass onset strength. Dynamic programming then splits the bars into
//! sections that are internally uniform, favouring the 8- and 16-bar phrase
//! lengths dance music uses. Labels come from each section's energy, groove
//! (bass and kick) and repetition, with a dance or hip-hop vocabulary.

use serde::{Deserialize, Serialize};

use crate::beats::Spectrogram;

/// Version of the stored section stage; bump when detection or labels change.
pub const SECTIONS_VERSION: u32 = 1;
const TIMBRE_GROUPS: usize = 16;
const MIN_BARS: usize = 4;
const MAX_BARS: usize = 64;
/// Cue times closer than this are the same cue.
const SAME_CUE: f64 = 0.002;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionLabel {
    Intro,
    BuildUp,
    Drop,
    Breakdown,
    Verse,
    Hook,
    Outro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vocabulary {
    Dance,
    HipHop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub start_seconds: f64,
    pub bars: u32,
    pub label: Option<SectionLabel>,
    /// Loudness relative to the track's sections, 0 (quietest) to 1.
    pub energy: f32,
    /// Bass level and how far bass stands out from the mix, relative to the
    /// track's sections, 0 to 1. High in drops, low in breakdowns and risers.
    pub groove: f32,
    /// Loudness change across the section, in fractions of the track's range.
    pub rise: f32,
    /// Sections with the same sound share a group.
    pub group: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionStage {
    pub version: u32,
    /// The grid the bars were cut from.
    pub bpm: f64,
    pub downbeat_offset_seconds: f64,
    /// Vocabulary chosen from the track's structure.
    pub vocabulary: Vocabulary,
    /// The DJ's choice, when it differs from the detected one.
    #[serde(default)]
    pub vocabulary_override: Option<Vocabulary>,
    pub sections: Vec<Section>,
}

impl SectionStage {
    pub fn effective_vocabulary(&self) -> Vocabulary {
        self.vocabulary_override.unwrap_or(self.vocabulary)
    }

    /// Whether the sections were cut from this grid.
    pub fn matches_grid(&self, bpm: f64, offset_seconds: f64) -> bool {
        let bar = 240.0 / bpm;
        let phase = (offset_seconds - self.downbeat_offset_seconds).rem_euclid(bar);
        (self.bpm - bpm).abs() < 0.01 && phase.min(bar - phase) < 0.002
    }

    /// Sections as a DJ sees them: neighbours with the same label (three drop
    /// phrases in a row) become one. Stored sections stay fine-grained so a
    /// vocabulary change can relabel them.
    pub fn merged(&self) -> Vec<Section> {
        let mut out: Vec<Section> = Vec::new();
        for s in &self.sections {
            match out.last_mut() {
                Some(last) if last.label.is_some() && last.label == s.label => {
                    let (a, b) = (last.bars as f32, s.bars as f32);
                    last.energy = (last.energy * a + s.energy * b) / (a + b);
                    last.groove = (last.groove * a + s.groove * b) / (a + b);
                    last.rise += s.rise;
                    last.bars += s.bars;
                }
                _ => out.push(s.clone()),
            }
        }
        out
    }

    /// Re-apply labels after a vocabulary change; no audio is needed.
    pub fn relabel(&mut self) {
        match self.effective_vocabulary() {
            Vocabulary::Dance => label_dance(&mut self.sections),
            Vocabulary::HipHop => label_hip_hop(&mut self.sections),
        }
    }
}

struct Bar {
    start: f64,
    level: f32,
    low: f32,
    high: f32,
    kick: f32,
    timbre: [f32; TIMBRE_GROUPS],
}

/// Cut a track into sections on a constant grid. Returns `None` when fewer
/// than eight audible bars exist or the grid is unusable.
pub fn detect_sections(
    spect: &Spectrogram,
    duration_seconds: f64,
    bpm: f64,
    downbeat_offset_seconds: f64,
    vocabulary_override: Option<Vocabulary>,
) -> Option<SectionStage> {
    if !bpm.is_finite() || !(40.0..=250.0).contains(&bpm) || !downbeat_offset_seconds.is_finite() {
        return None;
    }
    let bar = 240.0 / bpm;
    let offset = downbeat_offset_seconds.rem_euclid(bar);
    let bars = bar_features(spect, duration_seconds, bar, offset);
    if bars.len() < 8 {
        return None;
    }
    let boundaries = segment(&normalised_features(&bars));
    let mut sections = summarise(&bars, &boundaries);
    group_by_sound(&bars, &boundaries, &mut sections);
    let vocabulary = detect_vocabulary(&sections);
    let mut stage = SectionStage {
        version: SECTIONS_VERSION,
        bpm,
        downbeat_offset_seconds: offset,
        vocabulary,
        vocabulary_override,
        sections,
    };
    stage.relabel();
    Some(stage)
}

/// Features for every full bar from the first downbeat, without the silent
/// lead-in and tail.
fn bar_features(spect: &Spectrogram, duration: f64, bar: f64, offset: f64) -> Vec<Bar> {
    let fps = Spectrogram::FRAMES_PER_SECOND;
    let centres = Spectrogram::band_centres();
    let low: Vec<usize> = (0..centres.len()).filter(|&b| centres[b] < 150.0).collect();
    let high: Vec<usize> = (0..centres.len())
        .filter(|&b| centres[b] >= 2000.0)
        .collect();
    let mean_of = |frame: &[f32], bands: &[usize]| {
        bands.iter().map(|&b| frame[b]).sum::<f32>() / bands.len().max(1) as f32
    };
    let frames = spect.frame_count();
    let mut bars = Vec::new();
    let mut start = offset;
    while start + bar <= duration + 1e-6 {
        let f0 = (start * fps).round() as usize;
        let f1 = (((start + bar) * fps).round() as usize).min(frames);
        if f1 <= f0 + 1 {
            break;
        }
        let n = (f1 - f0) as f32;
        let (mut level, mut lo, mut hi, mut kick) = (0.0, 0.0, 0.0, 0.0);
        let mut timbre = [0f32; TIMBRE_GROUPS];
        let width = Spectrogram::BANDS / TIMBRE_GROUPS;
        let mut previous_low = mean_of(spect.frame(f0), &low);
        for f in f0..f1 {
            let frame = spect.frame(f);
            level += frame.iter().sum::<f32>() / frame.len() as f32;
            let l = mean_of(frame, &low);
            lo += l;
            hi += mean_of(frame, &high);
            kick += (l - previous_low).max(0.0);
            previous_low = l;
            for (g, value) in timbre.iter_mut().enumerate() {
                *value += frame[g * width..(g + 1) * width].iter().sum::<f32>() / width as f32;
            }
        }
        timbre.iter_mut().for_each(|v| *v /= n);
        bars.push(Bar {
            start,
            level: level / n,
            low: lo / n,
            high: hi / n,
            kick: kick / n,
            timbre,
        });
        start += bar;
    }
    // Drop silent bars at either end (below a tenth of the median loudness).
    let mut levels: Vec<f32> = bars.iter().map(|b| b.level).collect();
    levels.sort_by(f32::total_cmp);
    let floor = levels.get(levels.len() / 2).copied().unwrap_or(0.0) * 0.1;
    let first = bars
        .iter()
        .position(|b| b.level > floor)
        .unwrap_or(bars.len());
    let last = bars
        .iter()
        .rposition(|b| b.level > floor)
        .map_or(0, |i| i + 1);
    if first >= last {
        return Vec::new();
    }
    bars.truncate(last);
    bars.drain(..first);
    bars
}

/// Z-scored per-bar vectors; energy features count more than timbre.
fn normalised_features(bars: &[Bar]) -> Vec<Vec<f64>> {
    let raw: Vec<Vec<f64>> = bars
        .iter()
        .map(|b| {
            let mut v: Vec<f64> = b.timbre.iter().map(|&x| x as f64).collect();
            v.extend([b.level, b.low, b.high, b.kick].map(|x| x as f64));
            v
        })
        .collect();
    let dims = raw[0].len();
    let n = raw.len() as f64;
    let mut out = raw.clone();
    for d in 0..dims {
        let mean = raw.iter().map(|v| v[d]).sum::<f64>() / n;
        let std = (raw.iter().map(|v| (v[d] - mean).powi(2)).sum::<f64>() / n)
            .sqrt()
            .max(1e-3);
        let weight = if d < TIMBRE_GROUPS { 1.0 } else { 1.5 };
        for (row, value) in out.iter_mut().zip(&raw) {
            row[d] = (value[d] - mean) / std * weight;
        }
    }
    out
}

/// Section boundaries (bar indices, starting with 0) minimising within-section
/// variance plus a per-section cost, with lengths in whole phrases preferred.
fn segment(x: &[Vec<f64>]) -> Vec<usize> {
    let k = x.len();
    let dims = x[0].len();
    let mut s1 = vec![vec![0.0; dims]; k + 1];
    let mut s2 = vec![0.0; k + 1];
    for i in 0..k {
        for d in 0..dims {
            s1[i + 1][d] = s1[i][d] + x[i][d];
        }
        s2[i + 1] = s2[i] + x[i].iter().map(|v| v * v).sum::<f64>();
    }
    let cost = |i: usize, j: usize| {
        let n = (j - i) as f64;
        let mean_sq: f64 = (0..dims)
            .map(|d| (s1[j][d] - s1[i][d]).powi(2))
            .sum::<f64>()
            / n;
        s2[j] - s2[i] - mean_sq
    };
    let lambda = 4.0 * dims as f64;
    let length_cost = |len: usize, edge: bool| {
        // Whole 8-bar phrases are free; a lone 4-bar section needs clear evidence.
        let short = if !edge && len < 8 { 0.25 * lambda } else { 0.0 };
        short
            + if len.is_multiple_of(8) {
                0.0
            } else if len.is_multiple_of(4) {
                0.15 * lambda
            } else if edge {
                0.3 * lambda
            } else {
                0.6 * lambda
            }
    };
    let mut best = vec![f64::INFINITY; k + 1];
    let mut from = vec![0usize; k + 1];
    best[0] = 0.0;
    for j in 1..=k {
        for len in 1..=MAX_BARS.min(j) {
            let i = j - len;
            let edge = i == 0 || j == k;
            if len < MIN_BARS && !edge {
                continue;
            }
            let total = best[i] + cost(i, j) + lambda + length_cost(len, edge);
            if total < best[j] {
                best[j] = total;
                from[j] = i;
            }
        }
    }
    let mut starts = Vec::new();
    let mut j = k;
    while j > 0 {
        starts.push(from[j]);
        j = from[j];
    }
    starts.reverse();
    // Fragments under four bars at either end (a pickup, a fading tail) join
    // their neighbour rather than becoming sections of their own.
    if starts.len() >= 2 && starts[1] < MIN_BARS {
        starts.remove(1);
    }
    if starts.len() >= 2 && k - starts[starts.len() - 1] < MIN_BARS {
        starts.pop();
    }
    starts
}

fn summarise(bars: &[Bar], starts: &[usize]) -> Vec<Section> {
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([bars.len()]).collect();
    let spans: Vec<(usize, usize)> = starts.iter().copied().zip(ends).collect();
    let mean = |&(i, j): &(usize, usize), f: fn(&Bar) -> f32| {
        bars[i..j].iter().map(f).sum::<f32>() / (j - i) as f32
    };
    let level: Vec<f32> = spans.iter().map(|s| mean(s, |b| b.level)).collect();
    let low: Vec<f32> = spans.iter().map(|s| mean(s, |b| b.low)).collect();
    // Bass prominence: broadband risers raise the bass bands but not above the mix.
    let prominence: Vec<f32> = spans.iter().map(|s| mean(s, |b| b.low - b.level)).collect();
    let relative = |v: &[f32]| -> Vec<f32> {
        let lo = v.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        v.iter()
            .map(|&x| {
                if hi - lo > 1e-6 {
                    (x - lo) / (hi - lo)
                } else {
                    0.5
                }
            })
            .collect()
    };
    let (energy, low_rel, prominence_rel) =
        (relative(&level), relative(&low), relative(&prominence));
    let loudest = bars
        .iter()
        .map(|b| b.level)
        .fold(f32::NEG_INFINITY, f32::max);
    let quietest = bars.iter().map(|b| b.level).fold(f32::INFINITY, f32::min);
    let range = loudest - quietest;
    spans
        .iter()
        .enumerate()
        .map(|(s, &(i, j))| {
            // Least-squares loudness slope over the section, as a share of the
            // track's loudness range from its first bar to its last.
            let n = (j - i) as f32;
            let centre = (n - 1.0) / 2.0;
            let (num, den) = bars[i..j]
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(a, b), (t, bar)| {
                    let dx = t as f32 - centre;
                    (a + dx * (bar.level - level[s]), b + dx * dx)
                });
            let slope = if den > 0.0 { num / den } else { 0.0 };
            Section {
                start_seconds: (bars[i].start * 1e4).round() / 1e4,
                bars: (j - i) as u32,
                label: None,
                energy: energy[s],
                groove: 0.5 * low_rel[s] + 0.5 * prominence_rel[s],
                rise: if range > 1e-6 {
                    slope * (n - 1.0).max(1.0) / range
                } else {
                    0.0
                },
                group: 0,
            }
        })
        .collect()
}

/// Sections whose mean timbre and levels are close share a group id.
fn group_by_sound(bars: &[Bar], starts: &[usize], sections: &mut [Section]) {
    let x = normalised_features(bars);
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([bars.len()]).collect();
    let means: Vec<Vec<f64>> = starts
        .iter()
        .zip(&ends)
        .map(|(&i, &j)| {
            let n = (j - i) as f64;
            (0..x[0].len())
                .map(|d| x[i..j].iter().map(|r| r[d]).sum::<f64>() / n)
                .collect()
        })
        .collect();
    let dims = x[0].len() as f64;
    let mut representatives: Vec<usize> = Vec::new();
    for s in 0..sections.len() {
        let close = representatives.iter().position(|&r| {
            let d = means[s]
                .iter()
                .zip(&means[r])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>();
            (d / dims).sqrt() < 0.6
        });
        sections[s].group = match close {
            Some(g) => g as u16,
            None => {
                representatives.push(s);
                (representatives.len() - 1) as u16
            }
        };
    }
}

/// Dance music drops its bass for a breakdown and brings it back for a drop.
fn detect_vocabulary(sections: &[Section]) -> Vocabulary {
    let n = sections.len();
    if n < 3 {
        return Vocabulary::Dance;
    }
    let middle = &sections[1..n - 1];
    let dance = middle.iter().enumerate().any(|(i, s)| {
        s.groove <= 0.35
            && s.bars >= 8
            && middle[i + 1..]
                .iter()
                .chain(&sections[n - 1..])
                .any(|later| later.groove >= 0.65 && later.energy >= 0.55 && later.bars >= 8)
    });
    if dance {
        Vocabulary::Dance
    } else {
        Vocabulary::HipHop
    }
}

fn label_dance(sections: &mut [Section]) {
    let n = sections.len();
    for (i, s) in sections.iter_mut().enumerate() {
        let middle = i > 0 && i + 1 < n;
        s.label = if !middle {
            None
        } else if s.groove >= 0.6 && s.energy >= 0.55 {
            Some(SectionLabel::Drop)
        } else if s.groove <= 0.35 {
            Some(SectionLabel::Breakdown)
        } else {
            None
        };
    }
    // A build-up leads into a drop and either rises or is a short, lighter
    // lead-in; a flat 16-bar break before a drop stays a breakdown.
    for i in 1..n.saturating_sub(1) {
        let next_is_drop = sections[i + 1].label == Some(SectionLabel::Drop)
            || (i + 1 == n - 1 && sections[i + 1].groove >= 0.6);
        let s = &sections[i];
        if next_is_drop
            && s.label != Some(SectionLabel::Drop)
            && s.bars <= 16
            && (s.rise > 0.12 || (s.bars <= 8 && s.groove < sections[i + 1].groove - 0.2))
        {
            sections[i].label = Some(SectionLabel::BuildUp);
        }
    }
    // The intro runs until the first labelled section, the outro from the last
    // one: DJ intros and outros are often several plain phrases long.
    for (i, s) in sections.iter_mut().enumerate() {
        if i > 0 && s.label.is_some() {
            break;
        }
        s.label = Some(SectionLabel::Intro);
    }
    if n >= 2 {
        for i in (1..n).rev() {
            if i < n - 1 && sections[i].label.is_some() {
                break;
            }
            sections[i].label = Some(SectionLabel::Outro);
        }
    }
}

fn label_hip_hop(sections: &mut [Section]) {
    let n = sections.len();
    for s in sections.iter_mut() {
        s.label = None;
    }
    if n == 0 {
        return;
    }
    let edge = |s: &Section| s.bars <= 8 || s.energy < 0.5;
    let intro = edge(&sections[0]);
    let outro = n >= 2 && edge(&sections[n - 1]);
    let lo = usize::from(intro);
    let hi = if outro { n - 1 } else { n };
    let middle_bars: u32 = sections[lo..hi].iter().map(|s| s.bars).sum();
    // The hook is the repeating group of short (about eight-bar) sections; a
    // group that is most of the song is the beat under the verses instead.
    let mut groups: Vec<(u16, usize, u32, f32)> = Vec::new();
    for s in &sections[lo..hi] {
        match groups.iter_mut().find(|g| g.0 == s.group) {
            Some(g) => {
                g.1 += 1;
                g.2 += s.bars;
                g.3 += s.energy;
            }
            None => groups.push((s.group, 1, s.bars, s.energy)),
        }
    }
    let hook = groups
        .iter()
        .filter(|g| g.1 >= 2 && (g.2 as f32 / g.1 as f32) <= 12.0)
        .chain(
            groups
                .iter()
                .filter(|g| g.1 >= 2 && (g.2 as f32) < 0.7 * middle_bars as f32),
        )
        .max_by(|a, b| (a.3 / a.1 as f32).total_cmp(&(b.3 / b.1 as f32)))
        .map(|g| g.0);
    for s in &mut sections[lo..hi] {
        s.label = if Some(s.group) == hook {
            Some(SectionLabel::Hook)
        } else if s.bars >= 8 {
            Some(SectionLabel::Verse)
        } else {
            None
        };
    }
    if intro {
        sections[0].label = Some(SectionLabel::Intro);
    }
    if outro {
        sections[n - 1].label = Some(SectionLabel::Outro);
    }
}

/// Which cues PulseDeck placed itself, so later passes can tell them apart from
/// the DJ's own and never bring back one the DJ removed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoCues {
    /// Hot cue pads filled automatically, with the time placed.
    pub hot: Vec<(usize, f64)>,
    /// Auto hot cues the DJ cleared (pad and time): the pad stays empty and
    /// the cue is not placed elsewhere.
    pub declined_hot: Vec<(usize, f64)>,
    /// Memory cue times placed automatically.
    pub memory: Vec<f64>,
    /// Auto memory cues the DJ deleted.
    pub declined_memory: Vec<f64>,
}

pub struct CuePlan {
    pub hot: [Option<f64>; 8],
    pub memory: [Option<f64>; 10],
    pub record: AutoCues,
}

/// Fill empty cue slots from the sections. Cues the DJ set or moved are kept;
/// auto cues still where they were placed move with new sections; auto cues the
/// DJ removed stay removed. Memory cues come back sorted, as the engine keeps them.
pub fn plan_auto_cues(
    stage: &SectionStage,
    hot: [Option<f64>; 8],
    memory: [Option<f64>; 10],
    previous: Option<&AutoCues>,
    fill_hot: bool,
    fill_memory: bool,
) -> CuePlan {
    let previous = previous.cloned().unwrap_or_default();
    let beat = 60.0 / stage.bpm;
    let mut record = AutoCues::default();

    let mut hot = hot;
    if fill_hot {
        record.declined_hot = previous
            .declined_hot
            .iter()
            .copied()
            .filter(|&(pad, _)| pad < 8 && hot[pad].is_none())
            .collect();
        for &(pad, time) in previous.hot.iter().filter(|(pad, _)| *pad < 8) {
            match hot[pad] {
                Some(v) if (v - time).abs() < SAME_CUE => hot[pad] = None,
                None => record.declined_hot.push((pad, time)),
                Some(_) => {}
            }
        }
        let free: Vec<usize> = (0..8)
            .filter(|p| hot[*p].is_none() && record.declined_hot.iter().all(|(d, _)| d != p))
            .collect();
        let avoid: Vec<f64> = hot
            .iter()
            .flatten()
            .copied()
            .chain(record.declined_hot.iter().map(|&(_, t)| t))
            .collect();
        let mut picks: Vec<f64> = hot_candidates(stage)
            .into_iter()
            .filter(|t| avoid.iter().all(|v| (v - t).abs() >= beat))
            .take(free.len())
            .collect();
        picks.sort_by(f64::total_cmp);
        for (&pad, time) in free.iter().zip(picks) {
            hot[pad] = Some(time);
            record.hot.push((pad, time));
        }
    } else {
        record.hot = previous.hot.clone();
        record.declined_hot = previous.declined_hot.clone();
    }

    let mut kept: Vec<f64> = memory.iter().flatten().copied().collect();
    if fill_memory {
        record.declined_memory = previous.declined_memory.clone();
        for &time in &previous.memory {
            match kept.iter().position(|v| (v - time).abs() < SAME_CUE) {
                Some(i) => {
                    kept.remove(i);
                }
                None => record.declined_memory.push(time),
            }
        }
        let room = 10usize.saturating_sub(kept.len());
        let picks: Vec<f64> = stage
            .merged()
            .iter()
            .map(|s| s.start_seconds)
            .filter(|t| {
                kept.iter()
                    .chain(&record.declined_memory)
                    .all(|v| (v - t).abs() >= beat)
            })
            .take(room)
            .collect();
        kept.extend(&picks);
        record.memory = picks;
    } else {
        record.memory = previous.memory.clone();
        record.declined_memory = previous.declined_memory.clone();
    }
    kept.sort_by(f64::total_cmp);
    let mut memory = [None; 10];
    for (slot, time) in memory.iter_mut().zip(kept) {
        *slot = Some(time);
    }
    CuePlan {
        hot,
        memory,
        record,
    }
}

/// Section starts in hot-cue priority order: the first downbeat, drops and
/// hooks, breakdowns and verses, build-ups, the outro, then the rest.
fn hot_candidates(stage: &SectionStage) -> Vec<f64> {
    let rank = |label: Option<SectionLabel>| match label {
        Some(SectionLabel::Drop | SectionLabel::Hook) => 1,
        Some(SectionLabel::Breakdown | SectionLabel::Verse) => 2,
        Some(SectionLabel::BuildUp) => 3,
        Some(SectionLabel::Outro) => 4,
        Some(SectionLabel::Intro) | None => 5,
    };
    let mut ordered: Vec<(usize, f64)> = stage
        .merged()
        .iter()
        .enumerate()
        .map(|(i, s)| (if i == 0 { 0 } else { rank(s.label) }, s.start_seconds))
        .collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    ordered.into_iter().map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beats::spectrogram;

    const RATE: u32 = 22_050;

    /// Deterministic noise in -1..1.
    struct Noise(u32);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
        }
    }

    #[derive(Clone, Copy, Default)]
    struct Parts {
        kick: bool,
        bass: bool,
        hats: bool,
        snare: bool,
        lead: bool,
        pad: bool,
        riser: bool,
        vocal: bool,
    }

    /// Render bars of simple instruments at `bpm`, each section with its parts.
    fn render(bpm: f64, plan: &[(u32, Parts)]) -> Vec<[f32; 2]> {
        let beat = 60.0 / bpm;
        let mut noise = Noise(0x1234_5678);
        let mut out = Vec::new();
        let mut t0 = 0.0;
        for &(bars, p) in plan {
            let length = bars as f64 * 4.0 * beat;
            let n = (length * RATE as f64) as usize;
            for i in 0..n {
                let local = i as f64 / RATE as f64;
                let t = t0 + local;
                let in_beat = t % beat;
                let beat_no = (t / beat).floor() as i64;
                let progress = local / length;
                let mut v = 0.0f64;
                let tau = std::f64::consts::TAU;
                if p.kick {
                    v += 0.7
                        * (tau * (45.0 * in_beat + 6.0 * (1.0 - (-in_beat * 30.0).exp()))).sin()
                        * (-in_beat * 14.0).exp();
                }
                if p.bass {
                    let eighth = t % (beat / 2.0);
                    v += 0.35 * (tau * 55.0 * t).sin() * (-eighth * 4.0).exp();
                }
                let white = noise.next() as f64;
                if p.hats {
                    let off = (t + beat / 2.0) % beat;
                    v += 0.12 * white * (-off * 90.0).exp();
                }
                if p.snare && beat_no % 2 == 1 {
                    v += 0.3 * white * (-in_beat * 20.0).exp();
                }
                if p.lead {
                    for (k, f) in [440.0, 554.4, 659.3].iter().enumerate() {
                        v += 0.06 * (tau * f * t).sin().signum() / (k + 1) as f64;
                    }
                }
                if p.pad {
                    for f in [220.0, 277.2, 329.6] {
                        v += 0.08 * (tau * f * t).sin();
                    }
                }
                if p.riser {
                    let roll = t % (beat / 4.0);
                    v += (0.05 + 0.3 * progress) * white * (-roll * 30.0).exp();
                    v += 0.1 * progress * (tau * (300.0 + 900.0 * progress) * t).sin();
                }
                if p.vocal {
                    let syllable = 0.5 + 0.5 * (tau * 3.0 * t).sin();
                    v += 0.15
                        * syllable
                        * ((tau * 700.0 * t).sin() + 0.5 * (tau * 1400.0 * t).sin());
                }
                out.push([v as f32; 2]);
            }
            t0 += length;
        }
        out
    }

    fn labels(stage: &SectionStage) -> Vec<(u32, Option<SectionLabel>)> {
        stage.sections.iter().map(|s| (s.bars, s.label)).collect()
    }

    #[test]
    fn edm_structure_is_cut_on_phrases_and_labelled() {
        use SectionLabel::*;
        let groove = Parts {
            kick: true,
            hats: true,
            ..Default::default()
        };
        let build = Parts {
            riser: true,
            snare: true,
            ..Default::default()
        };
        let drop = Parts {
            kick: true,
            bass: true,
            hats: true,
            lead: true,
            ..Default::default()
        };
        let breakdown = Parts {
            pad: true,
            ..Default::default()
        };
        let audio = render(
            128.0,
            &[
                (16, groove),
                (8, build),
                (16, drop),
                (16, breakdown),
                (8, build),
                (16, drop),
                (16, groove),
            ],
        );
        let spect = spectrogram(&audio, RATE).unwrap();
        let duration = audio.len() as f64 / RATE as f64;
        let stage = detect_sections(&spect, duration, 128.0, 0.0, None).expect("sections");
        assert_eq!(stage.vocabulary, Vocabulary::Dance);
        assert_eq!(
            labels(&stage),
            [
                (16, Some(Intro)),
                (8, Some(BuildUp)),
                (16, Some(Drop)),
                (16, Some(Breakdown)),
                (8, Some(BuildUp)),
                (16, Some(Drop)),
                (16, Some(Outro)),
            ]
        );
        assert!((stage.sections[2].start_seconds - 24.0 * 1.875).abs() < 1e-3);
    }

    #[test]
    fn hip_hop_structure_finds_repeating_hooks() {
        use SectionLabel::*;
        let keys = Parts {
            pad: true,
            ..Default::default()
        };
        let verse = Parts {
            kick: true,
            snare: true,
            hats: true,
            bass: true,
            vocal: true,
            ..Default::default()
        };
        let hook = Parts {
            kick: true,
            snare: true,
            hats: true,
            bass: true,
            lead: true,
            ..Default::default()
        };
        let audio = render(
            90.0,
            &[
                (4, keys),
                (16, verse),
                (8, hook),
                (16, verse),
                (8, hook),
                (4, keys),
            ],
        );
        let spect = spectrogram(&audio, RATE).unwrap();
        let duration = audio.len() as f64 / RATE as f64;
        let mut stage = detect_sections(&spect, duration, 90.0, 0.0, None).expect("sections");
        assert_eq!(stage.vocabulary, Vocabulary::HipHop);
        assert_eq!(
            labels(&stage),
            [
                (4, Some(Intro)),
                (16, Some(Verse)),
                (8, Some(Hook)),
                (16, Some(Verse)),
                (8, Some(Hook)),
                (4, Some(Outro)),
            ]
        );
        // The DJ can switch vocabulary without the audio.
        stage.vocabulary_override = Some(Vocabulary::Dance);
        stage.relabel();
        assert_eq!(stage.sections[0].label, Some(Intro));
        assert!(stage.sections.iter().all(|s| s.label != Some(Hook)));
    }

    #[test]
    fn short_or_silent_audio_has_no_sections() {
        let silence = vec![[0f32; 2]; RATE as usize * 40];
        let spect = spectrogram(&silence, RATE).unwrap();
        assert!(detect_sections(&spect, 40.0, 120.0, 0.0, None).is_none());
        assert!(detect_sections(&spect, 40.0, f64::NAN, 0.0, None).is_none());
    }

    #[test]
    fn grid_match_tolerates_whole_bar_offsets() {
        let stage = SectionStage {
            version: SECTIONS_VERSION,
            bpm: 120.0,
            downbeat_offset_seconds: 0.25,
            vocabulary: Vocabulary::Dance,
            vocabulary_override: None,
            sections: Vec::new(),
        };
        assert!(stage.matches_grid(120.0, 2.25));
        assert!(!stage.matches_grid(120.0, 0.75));
        assert!(!stage.matches_grid(121.0, 0.25));
    }

    fn stage_with(starts: &[(f64, Option<SectionLabel>)]) -> SectionStage {
        SectionStage {
            version: SECTIONS_VERSION,
            bpm: 120.0,
            downbeat_offset_seconds: 0.0,
            vocabulary: Vocabulary::Dance,
            vocabulary_override: None,
            sections: starts
                .iter()
                .map(|&(t, label)| Section {
                    start_seconds: t,
                    bars: 16,
                    label,
                    energy: 0.5,
                    groove: 0.5,
                    rise: 0.0,
                    group: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn plain_lead_in_phrases_join_the_intro_and_stacked_drops_merge() {
        use SectionLabel::*;
        let section = |bars: u32, energy: f32, groove: f32, rise: f32| Section {
            start_seconds: 0.0,
            bars,
            label: None,
            energy,
            groove,
            rise,
            group: 0,
        };
        let mut sections = vec![
            section(16, 0.1, 0.5, 0.0),  // drums only
            section(16, 0.45, 0.5, 0.0), // drums and bass, still building
            section(16, 0.9, 0.9, 0.0),  // drop
            section(16, 0.9, 0.85, 0.0), // drop continues with a new melody
            section(16, 0.4, 0.1, 0.0),  // flat breakdown before the drop
            section(16, 1.0, 0.95, 0.0), // drop
            section(16, 0.45, 0.5, 0.0), // mix-out groove
            section(8, 0.1, 0.4, 0.0),   // fading drums
        ];
        for (i, s) in sections.iter_mut().enumerate() {
            s.start_seconds = i as f64 * 30.0;
        }
        label_dance(&mut sections);
        let labels: Vec<_> = sections.iter().map(|s| s.label).collect();
        assert_eq!(
            labels,
            [
                Some(Intro),
                Some(Intro),
                Some(Drop),
                Some(Drop),
                Some(Breakdown),
                Some(Drop),
                Some(Outro),
                Some(Outro)
            ]
        );
        let stage = SectionStage {
            version: SECTIONS_VERSION,
            bpm: 128.0,
            downbeat_offset_seconds: 0.0,
            vocabulary: Vocabulary::Dance,
            vocabulary_override: None,
            sections,
        };
        let merged: Vec<_> = stage
            .merged()
            .iter()
            .map(|s| (s.start_seconds, s.bars, s.label))
            .collect();
        assert_eq!(
            merged,
            [
                (0.0, 32, Some(Intro)),
                (60.0, 32, Some(Drop)),
                (120.0, 16, Some(Breakdown)),
                (150.0, 16, Some(Drop)),
                (180.0, 24, Some(Outro)),
            ]
        );
    }

    #[test]
    fn auto_cues_fill_only_free_slots_and_respect_the_dj() {
        use SectionLabel::*;
        let stage = stage_with(&[
            (0.5, Some(Intro)),
            (32.5, Some(BuildUp)),
            (48.5, Some(Drop)),
            (80.5, Some(Breakdown)),
            (112.5, Some(Drop)),
            (144.5, Some(Outro)),
        ]);
        let mut hot = [None; 8];
        hot[0] = Some(10.0); // the DJ's own pad
        let mut memory = [None; 10];
        memory[0] = Some(48.6); // the DJ already marked the first drop
        let plan = plan_auto_cues(&stage, hot, memory, None, true, true);
        assert_eq!(plan.hot[0], Some(10.0));
        assert_eq!(
            &plan.hot[1..7],
            [
                Some(0.5),
                Some(32.5),
                Some(48.5),
                Some(80.5),
                Some(112.5),
                Some(144.5)
            ]
        );
        assert_eq!(plan.hot[7], None);
        let times: Vec<f64> = plan.memory.iter().flatten().copied().collect();
        assert_eq!(times, [0.5, 32.5, 48.6, 80.5, 112.5, 144.5]);
        assert!(!plan.record.memory.contains(&48.5));

        // The DJ clears one auto pad, moves another and deletes an auto memory cue.
        let mut hot = plan.hot;
        hot[2] = None;
        hot[3] = Some(50.0);
        let mut memory = plan.memory;
        memory[1] = None;
        memory.sort_by(|a, b| {
            a.unwrap_or(f64::INFINITY)
                .total_cmp(&b.unwrap_or(f64::INFINITY))
        });
        // New sections (a changed grid) move the untouched auto cues.
        let shifted = stage_with(&[
            (0.75, Some(Intro)),
            (32.75, Some(BuildUp)),
            (48.75, Some(Drop)),
            (80.75, Some(Breakdown)),
            (112.75, Some(Drop)),
            (144.75, Some(Outro)),
        ]);
        let again = plan_auto_cues(&shifted, hot, memory, Some(&plan.record), true, true);
        assert_eq!(again.hot[0], Some(10.0));
        assert_eq!(again.hot[2], None, "a cleared auto pad stays empty");
        assert_eq!(again.hot[3], Some(50.0), "a moved pad is the DJ's");
        assert!(again.record.declined_hot.iter().any(|&(pad, _)| pad == 2));
        assert!(
            !again.hot.iter().flatten().any(|t| (t - 32.75).abs() < 0.3),
            "a cleared cue is not placed on another pad"
        );
        let times: Vec<f64> = again.memory.iter().flatten().copied().collect();
        assert!(!times.iter().any(|t| (t - 32.75).abs() < 0.3), "{times:?}");
        assert!(times.contains(&48.6) && times.contains(&0.75));

        // Turning auto cues off leaves everything as it is.
        let off = plan_auto_cues(
            &shifted,
            again.hot,
            again.memory,
            Some(&again.record),
            false,
            false,
        );
        assert_eq!((off.hot, off.memory), (again.hot, again.memory));
        assert_eq!(off.record, again.record);
    }
}
