//! Stereo-linked lookahead peak protection. The fixed delay is common to master
//! and cue, including bypass-sized signals; transport diagnostics expose it.
const LOOKAHEAD: usize = 128;
const CEILING: f32 = 0.8912509; // -1 dBFS conservative sample ceiling
pub(crate) const HEADROOM: f32 = 0.5011872; // 6 dB on the float bus before output protection
pub(crate) struct Protection {
    delay: [[f32; 2]; LOOKAHEAD],
    peaks: [(u64, f32); LOOKAHEAD + 1],
    first: usize,
    len: usize,
    frame: u64,
    cursor: usize,
    gain: f32,
    release: f32,
    rate: u32,

    pub overloaded_frames: u64,
    pub invalid_frames: u64,
}
impl Protection {
    pub fn new() -> Self {
        Self {
            delay: [[0.; 2]; LOOKAHEAD],
            peaks: [(0, 0.); LOOKAHEAD + 1],
            first: 0,
            len: 0,
            frame: 0,
            cursor: 0,
            gain: 1.,
            release: 0.,
            rate: 0,
            overloaded_frames: 0,
            invalid_frames: 0,
        }
    }
    pub fn latency_frames(&self) -> usize {
        LOOKAHEAD
    }
    pub fn reduction_db(&self) -> f32 {
        -20. * self.gain.max(1e-12).log10()
    }
    #[inline]
    pub fn frame(&mut self, input: [f32; 2], rate: u32) -> [f32; 2] {
        if self.rate != rate {
            self.rate = rate;
            self.release = 1. - (-1. / (rate as f32 * 0.080)).exp();
        }
        let mut signal = input;
        if signal.iter().any(|s| !s.is_finite()) {
            self.invalid_frames += 1;
            signal = [0.; 2];
        }
        let level = signal[0].abs().max(signal[1].abs());
        if level > CEILING {
            self.overloaded_frames += 1;
        }
        let delayed = self.delay[self.cursor];
        self.delay[self.cursor] = signal;
        // Monotonic max deque: preallocated, amortized O(1), at most 129
        // operations even for the worst incoming peak. It includes the sample
        // leaving the delay, as well as all future samples in the lookahead.
        while self.len > 0 && self.peaks[self.first].0 + (LOOKAHEAD as u64) < self.frame {
            self.first = (self.first + 1) % (LOOKAHEAD + 1);
            self.len -= 1;
        }
        while self.len > 0 {
            let last = (self.first + self.len - 1) % (LOOKAHEAD + 1);
            if self.peaks[last].1 > level {
                break;
            }
            self.len -= 1;
        }
        self.peaks[(self.first + self.len) % (LOOKAHEAD + 1)] = (self.frame, level);
        self.len += 1;
        self.frame = self.frame.wrapping_add(1);
        let horizon = self.peaks[self.first].1;
        let target = (CEILING / horizon.max(CEILING)).min(1.);
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain += (target - self.gain) * self.release;
        }
        self.cursor = (self.cursor + 1) % LOOKAHEAD;
        [delayed[0] * self.gain, delayed[1] * self.gain]
    }
}
