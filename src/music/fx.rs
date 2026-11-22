use std::f32::consts::TAU;

#[derive(Debug, Clone)]
struct DelayLine {
    buf: Vec<f32>,
    write_pos: usize,
}

impl DelayLine {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            write_pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, sample: f32) {
        self.buf[self.write_pos] = sample;
        self.write_pos += 1;
        if self.write_pos >= self.buf.len() {
            self.write_pos = 0;
        }
    }

    #[inline]
    fn read_at(&self, delay_samples: usize) -> f32 {
        let len = self.buf.len();
        let d = delay_samples.min(len - 1);
        let idx = (self.write_pos + len - 1 - d) % len;
        self.buf[idx]
    }

    #[inline]
    fn read_frac(&self, delay_samples: f32) -> f32 {
        let len = self.buf.len();
        let d = delay_samples.clamp(0.0, (len - 2) as f32);
        let i0 = d as usize;
        let frac = d - i0 as f32;
        let a = self.read_at(i0);
        let b = self.read_at(i0 + 1);
        a + (b - a) * frac
    }
}

#[derive(Debug, Clone)]
struct AllPass {
    delay: DelayLine,
    gain: f32,
}

impl AllPass {
    fn new(len: usize, gain: f32) -> Self {
        Self {
            delay: DelayLine::new(len),
            gain,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.delay.read_at(self.delay.buf.len() - 1);
        let v = input - self.gain * delayed;
        self.delay.push(flush_denormal(v));
        delayed + self.gain * v
    }

    #[inline]
    fn process_modulated(&mut self, input: f32, delay_samples: f32) -> f32 {
        let delayed = self.delay.read_frac(delay_samples);
        let v = input - self.gain * delayed;
        self.delay.push(flush_denormal(v));
        delayed + self.gain * v
    }
}

#[inline]
fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-18 { 0.0 } else { x }
}

pub struct StereoEcho {
    left: DelayLine,
    right: DelayLine,
    delay_samples: usize,
    lp_l: f32,
    lp_r: f32,
    feedback: f32,
}

impl StereoEcho {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let max_len = (sr * 1.2) as usize + 4;
        let delay_samples = (sr * 0.375) as usize; // dotted 8th-ish at 80 BPM
        Self {
            left: DelayLine::new(max_len),
            right: DelayLine::new(max_len),
            delay_samples: delay_samples.min(max_len - 2),
            lp_l: 0.0,
            lp_r: 0.0,
            feedback: 0.48,
        }
    }

    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let dl = self.left.read_at(self.delay_samples);
        let dr = self.right.read_at(self.delay_samples);

        // Gentle 1-pole low-pass darkening in the cross-feedback loop
        self.lp_l = flush_denormal(self.lp_l + (dr - self.lp_l) * 0.35);
        self.lp_r = flush_denormal(self.lp_r + (dl - self.lp_r) * 0.35);

        self.left.push(in_l + self.lp_l * self.feedback);
        self.right.push(in_r + self.lp_r * self.feedback);

        (dl, dr)
    }
}

pub struct TapeLoop {
    buf: DelayLine,
    head1: usize,
    head2: usize,
    lp: f32,
    feedback: f32,
}

impl TapeLoop {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let len = (sr * 4.8) as usize + 8;
        Self {
            buf: DelayLine::new(len),
            head1: ((sr * 1.85) as usize).min(len - 2),
            head2: ((sr * 3.70) as usize).min(len - 2),
            lp: 0.0,
            feedback: 0.44,
        }
    }

    #[inline]
    pub fn process(&mut self, mono_in: f32) -> (f32, f32) {
        let h1 = self.buf.read_at(self.head1);
        let h2 = self.buf.read_at(self.head2);
        let fb = (h1 * 0.55 + h2 * 0.45) * self.feedback;
        self.lp = flush_denormal(self.lp + (fb - self.lp) * 0.25);
        self.buf.push(mono_in + self.lp);
        (h1 * 0.75 + h2 * 0.25, h2 * 0.75 + h1 * 0.25)
    }
}

/// Jon Dattorro (1997) figure-8 plate reverberator, scaled to the host sample rate.
pub struct PlateReverb {
    sr_scale: f32,
    in_lp: f32,
    in_ap: [AllPass; 4],
    tank_ap1: [AllPass; 2],
    tank_d1: [DelayLine; 2],
    tank_lp: [f32; 2],
    tank_ap2: [AllPass; 2],
    tank_d2: [DelayLine; 2],
    tank_state: [f32; 2],
    lfo_phase: f32,
    lfo_inc: f32,
    base_mod_delay: [f32; 2],
    mod_excursion: f32,
    pub decay: f32,
    pub damping: f32,
}

impl PlateReverb {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let scale = sr / 29761.0;
        let s = |n: usize| -> usize { ((n as f32 * scale).round() as usize).max(4) };

        Self {
            sr_scale: scale,
            in_lp: 0.0,
            in_ap: [
                AllPass::new(s(142), 0.75),
                AllPass::new(s(107), 0.75),
                AllPass::new(s(379), 0.625),
                AllPass::new(s(277), 0.625),
            ],
            tank_ap1: [
                AllPass::new(s(672) + s(24), -0.70),
                AllPass::new(s(908) + s(24), -0.70),
            ],
            tank_d1: [DelayLine::new(s(4453)), DelayLine::new(s(4217))],
            tank_lp: [0.0, 0.0],
            tank_ap2: [AllPass::new(s(1800), 0.50), AllPass::new(s(2656), 0.50)],
            tank_d2: [DelayLine::new(s(3720)), DelayLine::new(s(3163))],
            tank_state: [0.0, 0.0],
            lfo_phase: 0.0,
            lfo_inc: 0.65 / sr,
            base_mod_delay: [672.0 * scale, 908.0 * scale],
            mod_excursion: 12.0 * scale,
            decay: 0.76,
            damping: 0.32,
        }
    }

    #[inline]
    pub fn process(&mut self, mono_in: f32, frozen: bool) -> (f32, f32) {
        let in_gain = if frozen { 0.0 } else { 1.0 };
        let decay = if frozen { 0.9985 } else { self.decay };
        let damp = if frozen { 0.08 } else { self.damping };

        // Input bandwidth filter + 4-stage diffuser
        self.in_lp = flush_denormal(self.in_lp + (mono_in * in_gain - self.in_lp) * 0.7);
        let mut x = self.in_lp;
        for ap in &mut self.in_ap {
            x = ap.process(x);
        }

        self.lfo_phase += self.lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo_s = (self.lfo_phase * TAU).sin();
        let lfo_c = (self.lfo_phase * TAU).cos();
        let mod_delays = [
            self.base_mod_delay[0] + lfo_s * self.mod_excursion,
            self.base_mod_delay[1] + lfo_c * self.mod_excursion,
        ];

        // Figure-8 tank
        for i in 0..2 {
            let other = 1 - i;
            let tank_in = x + self.tank_state[other] * decay;
            let a = self.tank_ap1[i].process_modulated(tank_in, mod_delays[i]);
            self.tank_d1[i].push(a);
            let d1 = self.tank_d1[i].read_at(self.tank_d1[i].buf.len() - 2);
            self.tank_lp[i] = flush_denormal(self.tank_lp[i] + (d1 - self.tank_lp[i]) * damp);
            let b = self.tank_ap2[i].process(self.tank_lp[i] * decay);
            self.tank_d2[i].push(b);
            self.tank_state[i] = self.tank_d2[i].read_at(self.tank_d2[i].buf.len() - 2);
        }

        // Stereo multi-tap readout
        let t = |n: usize| -> usize { (n as f32 * self.sr_scale).round() as usize };
        let out_l = 0.6 * self.tank_d1[1].read_at(t(266))
            + 0.6 * self.tank_d1[1].read_at(t(2974))
            - 0.6 * self.tank_d2[1].read_at(t(1996))
            - 0.6 * self.tank_d1[0].read_at(t(1990));
        let out_r = 0.6 * self.tank_d1[0].read_at(t(353))
            + 0.6 * self.tank_d1[0].read_at(t(3627))
            - 0.6 * self.tank_d2[0].read_at(t(1228))
            - 0.6 * self.tank_d1[1].read_at(t(2673));

        (out_l * 0.45, out_r * 0.45)
    }
}

pub struct SpaceFx {
    pub echo: StereoEcho,
    pub tape: TapeLoop,
    pub reverb: PlateReverb,
    pub echo_mix: f32,
    pub tape_mix: f32,
    pub reverb_mix: f32,
}

impl SpaceFx {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            echo: StereoEcho::new(sample_rate),
            tape: TapeLoop::new(sample_rate),
            reverb: PlateReverb::new(sample_rate),
            echo_mix: 0.42,
            tape_mix: 0.28,
            reverb_mix: 0.72,
        }
    }

    #[inline]
    pub fn process(
        &mut self,
        dry_l: f32,
        dry_r: f32,
        echo_send: f32,
        rev_send: f32,
        frozen: bool,
    ) -> (f32, f32) {
        let (el, er) = self.echo.process(dry_l * echo_send, dry_r * echo_send);
        let (tl, tr) = self.tape.process((el + er) * 0.5);
        let rev_in = rev_send * ((dry_l + dry_r) * 0.5 + (el + er + tl + tr) * 0.25);
        let (rl, rr) = self.reverb.process(rev_in, frozen);

        let out_l = dry_l + el * self.echo_mix + tl * self.tape_mix + rl * self.reverb_mix;
        let out_r = dry_r + er * self.echo_mix + tr * self.tape_mix + rr * self.reverb_mix;
        (out_l, out_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_fx_produces_finite_reverb_tail_and_freezes() {
        let mut fx = SpaceFx::new(44100.0);
        let mut max_tail = 0.0_f32;
        for i in 0..4096 {
            let impulse = if i == 0 { 0.8 } else { 0.0 };
            let (l, r) = fx.process(impulse, impulse, 0.5, 0.8, false);
            assert!(l.is_finite() && r.is_finite());
            if i > 100 {
                max_tail = max_tail.max(l.abs()).max(r.abs());
            }
        }
        assert!(max_tail > 0.001, "expected audible reverb/echo tail");

        // Verify freeze remains finite
        for _ in 0..1024 {
            let (l, r) = fx.process(0.0, 0.0, 0.5, 0.8, true);
            assert!(l.is_finite() && r.is_finite() && l.abs() < 4.0 && r.abs() < 4.0);
        }
    }
}
