//! Fixed-capacity send/return effects. Returns outlive source transport and faders.
pub(crate) struct EffectBus {
    echo: Vec<[f32; 2]>,
    reverb: Vec<[f32; 2]>,
    echo_epoch: Vec<u32>,
    reverb_epoch: Vec<u32>,
    cursor: usize,
    epoch: u32,
    remaining: u32,
    rate: u32,
    pub restricted: bool,
    pub peak: [f32; 2],
}
impl EffectBus {
    pub fn new() -> Self {
        Self {
            echo: vec![[0.; 2]; 384000],
            reverb: vec![[0.; 2]; 48000],
            echo_epoch: vec![0; 384000],
            reverb_epoch: vec![0; 48000],
            cursor: 0,
            epoch: 1,
            remaining: 0,
            rate: 0,
            restricted: false,
            peak: [0.; 2],
        }
    }
    pub fn active(&self) -> bool {
        self.remaining > 0
    }
    pub fn process(
        &mut self,
        input: [f32; 2],
        echo: f32,
        reverb: f32,
        delay_seconds: f64,
        rate: u32,
        restricted: bool,
    ) -> [f32; 2] {
        if self.rate != rate {
            self.epoch = self.epoch.wrapping_add(1).max(1);
            self.remaining = 0;
            self.restricted = false;
            self.peak = [0.; 2];
            self.rate = rate;
        }
        let was_active = self.active();
        let sending =
            (echo > 0.000001 || reverb > 0.000001) && input.iter().any(|x| x.abs() > 1e-9);
        if sending {
            self.remaining = rate.saturating_mul(8);
            self.restricted |= restricted;
        } else if self.remaining > 0 {
            self.remaining -= 1;
        }
        if self.remaining == 0 {
            if was_active {
                self.epoch = self.epoch.wrapping_add(1).max(1);
                self.peak = [0.; 2];
                self.restricted = false;
            }
            return [0.; 2];
        }
        let n = self.echo.len();
        let rn = self.reverb.len();
        let delay = (delay_seconds * rate as f64)
            .round()
            .clamp(1., (n - 1) as f64) as usize;
        let read = (self.cursor + n - delay) % n;
        let wet = if self.echo_epoch[read] == self.epoch {
            self.echo[read]
        } else {
            [0.; 2]
        };
        let p = self.cursor % rn;
        let t1 = (p + rn - (rate as usize * 73 / 1000).min(rn - 1)) % rn;
        let t2 = (p + rn - (rate as usize * 109 / 1000).min(rn - 1)) % rn;
        let r1 = if self.reverb_epoch[t1] == self.epoch {
            self.reverb[t1]
        } else {
            [0.; 2]
        };
        let r2 = if self.reverb_epoch[t2] == self.epoch {
            self.reverb[t2]
        } else {
            [0.; 2]
        };
        let rev = [r1[0] * 0.58 + r2[1] * 0.34, r1[1] * 0.58 + r2[0] * 0.34];
        let mut out = [0.; 2];
        for c in 0..2 {
            self.echo[self.cursor][c] = input[c] * echo + wet[c] * 0.42;
            self.reverb[p][c] = input[c] * reverb + rev[c] * 0.56;
            out[c] = wet[c] + rev[c];
        }
        self.echo_epoch[self.cursor] = self.epoch;
        self.reverb_epoch[p] = self.epoch;
        self.cursor = (self.cursor + 1) % n;
        self.peak = out.map(f32::abs);
        out
    }
}
