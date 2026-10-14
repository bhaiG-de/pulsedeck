export type DeckId = number;
export interface WaveformBands {
  low: number[];
  mid: number[];
  high: number[];
}
export interface BpmCandidate {
  bpm: number;
  score: number;
}
export interface KeyCandidate {
  label: string;
  camelot: string;
  score: number;
}
export interface BeatModelGrid {
  bpm: number;
  downbeat_offset_seconds: number;
  on_grid: number;
  bar_agreement: number;
}
export interface BeatModelStage {
  version: number;
  beats_seconds: number[];
  downbeats_seconds: number[];
  grid?: BeatModelGrid | null;
}
export type SectionLabel =
  "intro" | "build_up" | "drop" | "breakdown" | "verse" | "hook" | "outro";
export type Vocabulary = "dance" | "hip_hop";
export interface Section {
  start_seconds: number;
  bars: number;
  label: SectionLabel | null;
  energy: number;
  groove: number;
  rise: number;
  group: number;
}
export interface SectionStage {
  version: number;
  bpm: number;
  downbeat_offset_seconds: number;
  vocabulary: Vocabulary;
  vocabulary_override?: Vocabulary | null;
  sections: Section[];
}
export interface AnalysisSettings {
  beat_model_spotify: boolean;
  auto_hot_cues: boolean;
  auto_memory_cues: boolean;
}
export interface TrackRecord {
  id: string | number;
  path: string;
  title: string;
  artist?: string;
  tags?: string[];
  album?: string;
  duration?: number;
  duration_seconds?: number;
  bpm?: number;
  key?: string;
  camelot?: string | null;
  key_confidence?: number | null;
  analysis_status?: string;
  waveform?: number[];
  waveform_bands?: WaveformBands | null;
  beatgrid_offset_seconds?: number | null;
  beatgrid_confidence?: number | null;
  beatgrid_source?: string | null;
  analysis_version?: number;
  bpm_confidence?: number;
  bpm_candidates?: BpmCandidate[];
  bpm_ambiguous?: boolean;
  bpm_source?: string | null;
  key_candidates?: KeyCandidate[];
  beat_model?: BeatModelStage | null;
  sections?: SectionStage | null;
  recording_allowed?: boolean;
  error?: string;
}
export interface Playlist {
  id: string | number;
  name: string;
  track_ids: (string | number)[];
}
export interface AudioDeviceInfo {
  name: string;
  output_channels?: number;
  channels?: number;
  sample_rates?: number[];
  is_default?: boolean;
}
export interface DeckSnapshot {
  id?: number;
  peak?: number[];
  memory_cues?: (number | null)[];
  pitch_semitones?: number;
  keylock?: boolean;
  preparation_pending?: boolean;
  channel_fader?: number;
  sync_enabled?: boolean;
  is_master?: boolean;
  beatgrid_offset_seconds?: number;
  track?: TrackRecord | null;
  playback_rate?: number;
  echo?: number;
  reverb?: number;
  loop_start_seconds?: number | null;
  loop_end_seconds?: number | null;
  track_title?: string;
  title?: string;
  track_path?: string;
  track_id?: string | number;
  duration_seconds?: number;
  position_seconds?: number;
  duration?: number;
  position?: number;
  playing?: boolean;
  jog_touch?: boolean;
  position_revision?: number;
  bpm?: number;
  original_bpm?: number;
  tempo?: number;
  tempo_percent?: number;
  rate?: number;
  key?: string;
  gain?: number;
  volume?: number;
  eq_low?: number;
  eq_mid?: number;
  eq_high?: number;
  filter?: number;
  cue_enabled?: boolean;
  cue_seconds?: number;
  hot_cues?: Array<number | null>;
  hotcues?: Array<number | null>;
  loop_active?: boolean;
  loop_start?: number;
  loop_end?: number;
  waveform?: number[];
  peak_left?: number;
  peak_right?: number;
  master?: boolean;
  sync?: boolean;
}
export interface EngineSnapshot {
  decks: DeckSnapshot[];
  sampler_gain?: number;
  crossfader?: number;
  master_gain?: number;
  cue_gain?: number;
  cue_mix?: number;
  running?: boolean;
  audio_running?: boolean;
  audio_status?: string;
  sample_rate?: number;
  buffer_frames?: number;
  audio_device?: string;
  master_peak?: number[];
  cue_peak?: number[];
  output_channels?: number;
  master_peak_left?: number;
  master_peak_right?: number;
  xruns?: number;
  underruns?: number;
  effects?: Record<string, number | boolean>;
  [key: string]: unknown;
}
export interface MidiState {
  connected?: boolean;
  messages_received?: number;
  dropped_messages?: number;
  feedback_enabled?: boolean;
  last_error?: string | null;
  input_name?: string;
  output_name?: string;
  inputs?: string[];
  outputs?: string[];
  last_message?: string;
  error?: string;
}
export interface AppState {
  engine: EngineSnapshot;
  library: TrackRecord[];
  playlists: Playlist[];
  audio_devices: AudioDeviceInfo[];
  audio_status?: {
    running: boolean;
    device_name: string;
    sample_rate: number;
    channels: number;
    buffer_frames: number;
    routing: {
      master_left: number;
      master_right: number;
      cue_left: number | null;
      cue_right: number | null;
    };
  } | null;
  midi: MidiState;
  smart_modes?: { fader: boolean; cfx: boolean };
  spotify_decks?: {
    preparing: boolean;
    uri?: string | null;
    error?: string | null;
  }[];
  analysis?: {
    active: boolean;
    total: number;
    completed: number;
    current_path?: string | null;
    errors: string[];
    cancelled?: boolean;
  };
  analysis_settings?: AnalysisSettings;
  messages?: string[];
}
export const emptyEngine: EngineSnapshot = {
  decks: [{}, {}],
  crossfader: 0,
  master_gain: 1,
  cue_gain: 1,
  cue_mix: 0,
};

export interface RecordingStatus {
  recording?: boolean;
  active?: boolean;
  path?: string | null;
  frames_written?: number;
  dropped_frames?: number;
  error?: string | null;
}
