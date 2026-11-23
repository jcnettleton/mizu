use std::f32::consts::TAU;

use crate::music::sequencer::Rng;
use crate::music::theory::midi_to_hz;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadInstrument {
    Rhodes,
    #[default]
    Choir,
    Organ,
}

impl PadInstrument {
    pub const ALL: [Self; 3] = [Self::Rhodes, Self::Choir, Self::Organ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Rhodes => "Rhodes",
            Self::Choir => "Choir",
            Self::Organ => "Organ",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BellInstrument {
    Glass,
    #[default]
    Marimba,
    Chime,
}

impl BellInstrument {
    pub const ALL: [Self; 3] = [Self::Glass, Self::Marimba, Self::Chime];

    pub fn name(self) -> &'static str {
        match self {
            Self::Glass => "Glass",
            Self::Marimba => "Marimba",
            Self::Chime => "Chime",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BassInstrument {
    Sub,
    Warm,
    #[default]
    Pluck,
}

impl BassInstrument {
    pub const ALL: [Self; 3] = [Self::Sub, Self::Warm, Self::Pluck];

    pub fn name(self) -> &'static str {
        match self {
            Self::Sub => "Sub",
            Self::Warm => "Warm",
            Self::Pluck => "Pluck",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PulseInstrument {
    Soft,
    Brush,
    #[default]
    Wood,
}

impl PulseInstrument {
    pub const ALL: [Self; 3] = [Self::Soft, Self::Brush, Self::Wood];

    pub fn name(self) -> &'static str {
        match self {
            Self::Soft => "Soft",
            Self::Brush => "Brush",
            Self::Wood => "Wood",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|&x| x == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Smooth breathing envelope over `t` in `0.0..1.0`:
/// 0.00..0.40 rise, 0.40..0.55 hold, 0.55..0.90 fall, 0.90..1.00 rest.
#[inline]
pub fn breath_envelope(t: f32) -> f32 {
    if t < 0.40 {
        0.5 - 0.5 * (std::f32::consts::PI * t * 2.5).cos()
    } else if t < 0.55 {
        1.0
    } else if t < 0.90 {
        0.5 + 0.5 * (std::f32::consts::PI * (t - 0.55) / 0.35).cos()
    } else {
        0.0
    }
}

/// Lightweight 2-pole Chamberlin state-variable filter for vowel formants.
#[derive(Debug, Clone, Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    #[inline]
    fn bandpass(&mut self, input: f32, cutoff_hz: f32, q: f32, inv_sr: f32) -> f32 {
        let f = (2.0 * (std::f32::consts::PI * cutoff_hz * inv_sr).sin()).clamp(0.001, 0.85);
        let damp = (1.0 / q.max(0.5)).clamp(0.05, 1.5);
        let high = input - self.low - damp * self.band;
        self.band += f * high;
        self.low += f * self.band;
        if self.band.abs() < 1.0e-18 {
            self.band = 0.0;
        }
        if self.low.abs() < 1.0e-18 {
            self.low = 0.0;
        }
        self.band
    }
}

/// Vowel formant pairs (F1, F2 in Hz) for warm /u/, /o/, /a/, /e/.
const VOWEL_FORMANTS: [(f32, f32); 4] = [
    (300.0, 780.0),  // /u/
    (460.0, 920.0),  // /o/
    (650.0, 1050.0), // /a/
    (480.0, 1350.0), // soft /e/
];

#[derive(Debug, Clone)]
pub struct PadVoice {
    carrier_phase: f32,
    mod_phase: f32,
    freq: f32,
    target_freq: f32,
    breath_phase: f32,
    breath_hz: f32,
    vowel_phase: f32,
    vowel_hz: f32,
    formant1: Svf,
    formant2: Svf,
    pan: f32,
    base_gain: f32,
}

impl PadVoice {
    pub fn new(note: u8, cycle_secs: f32, start_phase: f32, pan: f32, base_gain: f32) -> Self {
        let freq = midi_to_hz(note as f32);
        Self {
            carrier_phase: 0.0,
            mod_phase: 0.0,
            freq,
            target_freq: freq,
            breath_phase: start_phase,
            breath_hz: 1.0 / cycle_secs.max(4.0),
            vowel_phase: start_phase,
            vowel_hz: 1.0 / (cycle_secs.max(4.0) * 1.45),
            formant1: Svf::default(),
            formant2: Svf::default(),
            pan,
            base_gain,
        }
    }

    pub fn set_target_note(&mut self, note: u8) {
        self.target_freq = midi_to_hz(note as f32);
    }

    #[inline]
    pub fn next_stereo(
        &mut self,
        inv_sr: f32,
        freq_slew: f32,
        brightness: f32,
        instrument: PadInstrument,
        frozen: bool,
    ) -> (f32, f32) {
        self.freq += (self.target_freq - self.freq) * freq_slew;

        if !frozen {
            self.breath_phase += self.breath_hz * inv_sr;
            if self.breath_phase >= 1.0 {
                self.breath_phase -= 1.0;
            }
            self.vowel_phase += self.vowel_hz * inv_sr;
            if self.vowel_phase >= 1.0 {
                self.vowel_phase -= 1.0;
            }
        }

        let env = breath_envelope(self.breath_phase);
        if env <= 0.0001 {
            return (0.0, 0.0);
        }

        self.carrier_phase = (self.carrier_phase + self.freq * inv_sr).fract();
        self.mod_phase = (self.mod_phase + self.freq * 2.0 * inv_sr).fract();

        let raw = match instrument {
            PadInstrument::Rhodes => {
                let mod_index = (0.15 + 0.65 * brightness) * env;
                let mod_sig = (self.mod_phase * TAU).sin() * mod_index;
                ((self.carrier_phase * TAU) + mod_sig).sin()
            }
            PadInstrument::Choir => {
                // Soft detuned harmonic blend + gentle unity-gain vowel formant coloring
                let p1 = self.carrier_phase;
                let p2 = (self.carrier_phase * 1.003 + self.mod_phase * 0.002).fract();
                let exc = (p1 * TAU).sin() * 0.55
                    + (p1 * 2.0 * TAU).sin() * 0.28
                    + (p1 * 3.0 * TAU).sin() * (0.14 + 0.12 * brightness)
                    + (p2 * TAU).sin() * 0.40;

                let vpos = self.vowel_phase * VOWEL_FORMANTS.len() as f32;
                let v0 = (vpos.floor() as usize) % VOWEL_FORMANTS.len();
                let v1 = (v0 + 1) % VOWEL_FORMANTS.len();
                let frac = vpos.fract();
                let f1_hz = VOWEL_FORMANTS[v0].0 + (VOWEL_FORMANTS[v1].0 - VOWEL_FORMANTS[v0].0) * frac;
                let f2_hz = VOWEL_FORMANTS[v0].1 + (VOWEL_FORMANTS[v1].1 - VOWEL_FORMANTS[v0].1) * frac;

                let q1 = 2.2;
                let q2 = 2.4;
                let bp1 = self.formant1.bandpass(exc, f1_hz, q1, inv_sr) / q1;
                let bp2 = self.formant2.bandpass(exc, f2_hz, q2, inv_sr) / q2;
                let base = (p1 * TAU).sin() * 0.42 + (p2 * TAU).sin() * 0.28;
                (base + bp1 * 0.42 + bp2 * (0.18 + 0.12 * brightness)) * 0.72
            }
            PadInstrument::Organ => {
                // Warm additive drawbar harmonics (1st, 2nd, 3rd)
                let h1 = (self.carrier_phase * TAU).sin() * 0.68;
                let h2 = (self.mod_phase * TAU).sin() * (0.20 + 0.15 * brightness);
                let h3 = (self.carrier_phase * 3.0 * TAU).sin() * (0.08 + 0.12 * brightness);
                h1 + h2 + h3
            }
        };

        let sample = raw * env * self.base_gain;
        let angle = (self.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        (sample * angle.cos(), sample * angle.sin())
    }
}

/// Gentle modulated stereo ensemble chorus + low-pass filter for the summed pad bus.
pub struct PadChorus {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    lfo_phase: f32,
    lfo_inc: f32,
    base_delay_samples: f32,
    depth_samples: f32,
    lp_l: f32,
    lp_r: f32,
}

impl PadChorus {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let max_len = (sr * 0.045) as usize + 8;
        Self {
            buf_l: vec![0.0; max_len],
            buf_r: vec![0.0; max_len],
            write_pos: 0,
            lfo_phase: 0.0,
            lfo_inc: 0.34 / sr,
            base_delay_samples: sr * 0.016,
            depth_samples: sr * 0.0032,
            lp_l: 0.0,
            lp_r: 0.0,
        }
    }

    #[inline]
    fn read_frac(buf: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
        let len = buf.len();
        let d = delay_samples.clamp(1.0, (len - 2) as f32);
        let i0 = d as usize;
        let frac = d - i0 as f32;
        let idx0 = (write_pos + len - 1 - i0) % len;
        let idx1 = (write_pos + len - 2 - i0) % len;
        buf[idx0] + (buf[idx1] - buf[idx0]) * frac
    }

    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32, brightness: f32, inv_sr: f32) -> (f32, f32) {
        self.buf_l[self.write_pos] = in_l;
        self.buf_r[self.write_pos] = in_r;
        self.write_pos += 1;
        if self.write_pos >= self.buf_l.len() {
            self.write_pos = 0;
        }

        self.lfo_phase += self.lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let s = (self.lfo_phase * TAU).sin();
        let c = (self.lfo_phase * TAU).cos();

        let wet_l = Self::read_frac(
            &self.buf_l,
            self.write_pos,
            self.base_delay_samples + s * self.depth_samples,
        );
        let wet_r = Self::read_frac(
            &self.buf_r,
            self.write_pos,
            self.base_delay_samples + c * self.depth_samples,
        );

        let mix_l = in_l * 0.72 + wet_l * 0.36;
        let mix_r = in_r * 0.72 + wet_r * 0.36;

        // Gentle 1-pole low-pass tracking brightness (~950 Hz .. 4500 Hz)
        let cutoff = 950.0 + 3550.0 * brightness.clamp(0.0, 1.0);
        let alpha = (TAU * cutoff * inv_sr).clamp(0.02, 0.75);
        self.lp_l += (mix_l - self.lp_l) * alpha;
        self.lp_r += (mix_r - self.lp_r) * alpha;
        if self.lp_l.abs() < 1.0e-18 {
            self.lp_l = 0.0;
        }
        if self.lp_r.abs() < 1.0e-18 {
            self.lp_r = 0.0;
        }

        (self.lp_l, self.lp_r)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BellVoice {
    active: bool,
    carrier_phase: f32,
    mod_phase: f32,
    carrier_inc: f32,
    mod_inc: f32,
    amp: f32,
    amp_dec: f32,
    mod_index: f32,
    mod_dec: f32,
    gain_l: f32,
    gain_r: f32,
}

pub struct BellPool {
    voices: [BellVoice; 8],
    next_slot: usize,
}

impl BellPool {
    pub fn new() -> Self {
        Self {
            voices: [BellVoice::default(); 8],
            next_slot: 0,
        }
    }

    pub fn trigger(
        &mut self,
        note: u8,
        velocity: f32,
        base_ratio: f32,
        pan: f32,
        brightness: f32,
        instrument: BellInstrument,
        sample_rate: f32,
    ) {
        let sr = sample_rate.max(8000.0);
        let freq = midi_to_hz(note as f32);
        let idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                let s = self.next_slot;
                self.next_slot = (self.next_slot + 1) % self.voices.len();
                s
            });

        let (ratio, decay_secs, idx_decay_secs, mod_scale) = match instrument {
            BellInstrument::Glass => (
                base_ratio,
                1.8 + 1.6 * (1.0 - brightness * 0.4),
                0.25 + 1.1 * brightness,
                1.0,
            ),
            BellInstrument::Marimba => (4.0, 0.65 + 0.4 * (1.0 - brightness), 0.045, 0.7),
            BellInstrument::Chime => (base_ratio * 1.38, 2.8 + 1.5 * brightness, 0.9 + 1.2 * brightness, 1.2),
        };

        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let vel = velocity.clamp(0.1, 1.0);

        self.voices[idx] = BellVoice {
            active: true,
            carrier_phase: 0.0,
            mod_phase: 0.0,
            carrier_inc: freq / sr,
            mod_inc: (freq * ratio) / sr,
            amp: vel * 0.24,
            amp_dec: (-6.9 / (decay_secs * sr)).exp(),
            mod_index: (0.45 + 1.15 * brightness * vel) * mod_scale,
            mod_dec: (-6.9 / (idx_decay_secs * sr)).exp(),
            gain_l: angle.cos(),
            gain_r: angle.sin(),
        };
    }

    #[inline]
    pub fn next_stereo(&mut self) -> (f32, f32) {
        let mut l = 0.0_f32;
        let mut r = 0.0_f32;
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            v.carrier_phase += v.carrier_inc;
            if v.carrier_phase >= 1.0 {
                v.carrier_phase -= 1.0;
            }
            v.mod_phase += v.mod_inc;
            if v.mod_phase >= 1.0 {
                v.mod_phase -= 1.0;
            }

            let mod_sig = (v.mod_phase * TAU).sin() * v.mod_index;
            let sample = ((v.carrier_phase * TAU) + mod_sig).sin() * v.amp;
            l += sample * v.gain_l;
            r += sample * v.gain_r;

            v.amp *= v.amp_dec;
            v.mod_index *= v.mod_dec;
            if v.amp < 0.0002 {
                v.active = false;
            }
        }
        (l, r)
    }
}

pub struct BassVoice {
    phase: f32,
    sub_phase: f32,
    note: f32,
    target_note: f32,
    glide_slew: f32,
    pluck_env: f32,
    pluck_dec: f32,
}

impl BassVoice {
    pub fn new(initial_note: u8, sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            phase: 0.0,
            sub_phase: 0.0,
            note: initial_note as f32,
            target_note: initial_note as f32,
            glide_slew: 1.0 - (-1.0 / (0.28 * sr)).exp(),
            pluck_env: 0.0,
            pluck_dec: (-6.9 / (0.45 * sr)).exp(),
        }
    }

    pub fn set_target_pc(&mut self, pc: u8, octave_base: u8) {
        self.target_note = (octave_base + (pc % 12)) as f32;
        self.pluck_env = 1.0;
    }

    #[inline]
    pub fn next_sample(&mut self, inv_sr: f32, instrument: BassInstrument) -> f32 {
        self.note += (self.target_note - self.note) * self.glide_slew;
        let freq = midi_to_hz(self.note);

        self.phase += freq * inv_sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.sub_phase += (freq * 0.5) * inv_sr;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }
        self.pluck_env *= self.pluck_dec;

        let raw = match instrument {
            BassInstrument::Sub => {
                let main = (self.phase * TAU).sin();
                let sub = (self.sub_phase * TAU).sin() * 0.42;
                main + sub
            }
            BassInstrument::Warm => {
                let tri = 1.0 - 4.0 * (self.phase - 0.5).abs();
                let sine = (self.phase * TAU).sin();
                let h2 = (self.phase * 2.0 * TAU).sin() * 0.18;
                sine * 0.6 + tri * 0.35 + h2
            }
            BassInstrument::Pluck => {
                let mod_sig = (self.phase * 2.0 * TAU).sin() * (0.2 + 0.9 * self.pluck_env);
                let s = ((self.phase * TAU) + mod_sig).sin();
                s * (0.45 + 0.55 * self.pluck_env)
            }
        };
        raw * 0.22
    }
}

pub struct PulseSynth {
    // Kick / thump
    kick_active: bool,
    kick_phase: f32,
    kick_inc: f32,
    kick_end_inc: f32,
    kick_amp: f32,
    kick_dec: f32,
    // Closed & open hats (noise + simple 1-pole high-pass)
    hat_amp: [f32; 2],
    hat_dec: [f32; 2],
    hat_hp_prev: [f32; 2],
    hat_hp_out: [f32; 2],
}

impl PulseSynth {
    pub fn new() -> Self {
        Self {
            kick_active: false,
            kick_phase: 0.0,
            kick_inc: 0.0,
            kick_end_inc: 0.0,
            kick_amp: 0.0,
            kick_dec: 0.999,
            hat_amp: [0.0; 2],
            hat_dec: [0.99, 0.995],
            hat_hp_prev: [0.0; 2],
            hat_hp_out: [0.0; 2],
        }
    }

    pub fn trigger_kick(&mut self, root_pc: u8, instrument: PulseInstrument, sample_rate: f32) {
        let sr = sample_rate.max(8000.0);
        match instrument {
            PulseInstrument::Soft => {
                let target_freq = midi_to_hz(36.0 + (root_pc % 12) as f32);
                self.kick_active = true;
                self.kick_phase = 0.0;
                self.kick_inc = (target_freq * 2.4) / sr;
                self.kick_end_inc = target_freq / sr;
                self.kick_amp = 0.24;
                self.kick_dec = (-6.9 / (0.38 * sr)).exp();
            }
            PulseInstrument::Brush => {
                // Soft brush sweep instead of heavy kick
                self.hat_amp[1] = 0.075;
                self.hat_dec[1] = (-6.9 / (0.14 * sr)).exp();
            }
            PulseInstrument::Wood => {
                // Short pitched wood-knock
                let target_freq = midi_to_hz(60.0 + (root_pc % 12) as f32);
                self.kick_active = true;
                self.kick_phase = 0.0;
                self.kick_inc = (target_freq * 1.4) / sr;
                self.kick_end_inc = target_freq / sr;
                self.kick_amp = 0.16;
                self.kick_dec = (-6.9 / (0.065 * sr)).exp();
            }
        }
    }

    pub fn trigger_hat(&mut self, open: bool, instrument: PulseInstrument, sample_rate: f32) {
        let sr = sample_rate.max(8000.0);
        let idx = usize::from(open);
        match instrument {
            PulseInstrument::Soft => {
                let decay_secs = if open { 0.34 } else { 0.055 };
                self.hat_amp[idx] = if open { 0.06 } else { 0.08 };
                self.hat_dec[idx] = (-6.9 / (decay_secs * sr)).exp();
            }
            PulseInstrument::Brush => {
                let decay_secs = if open { 0.24 } else { 0.09 };
                self.hat_amp[idx] = if open { 0.07 } else { 0.09 };
                self.hat_dec[idx] = (-6.9 / (decay_secs * sr)).exp();
            }
            PulseInstrument::Wood => {
                let decay_secs = if open { 0.08 } else { 0.025 };
                self.hat_amp[idx] = if open { 0.05 } else { 0.07 };
                self.hat_dec[idx] = (-6.9 / (decay_secs * sr)).exp();
            }
        }
        if !open {
            // Closed hat chokes open hat slightly
            self.hat_dec[1] = (-6.9 / (0.025 * sr)).exp();
        }
    }

    #[inline]
    pub fn next_stereo(&mut self, rng: &mut Rng) -> (f32, f32) {
        let mut l = 0.0_f32;
        let mut r = 0.0_f32;

        if self.kick_active {
            let s = (self.kick_phase * TAU).sin() * self.kick_amp;
            l += s;
            r += s;
            self.kick_phase += self.kick_inc;
            if self.kick_phase >= 1.0 {
                self.kick_phase -= 1.0;
            }
            self.kick_inc += (self.kick_end_inc - self.kick_inc) * 0.0015;
            self.kick_amp *= self.kick_dec;
            if self.kick_amp < 0.0003 {
                self.kick_active = false;
            }
        }

        let white = rng.next_f32() * 2.0 - 1.0;
        for i in 0..2 {
            if self.hat_amp[i] > 0.0002 {
                // Simple 1-pole high-pass for airy shimmer
                let hp = 0.85 * (self.hat_hp_out[i] + white - self.hat_hp_prev[i]);
                self.hat_hp_prev[i] = white;
                self.hat_hp_out[i] = hp;
                let s = hp * self.hat_amp[i];
                if i == 0 {
                    l += s * 0.65;
                    r += s * 0.95;
                } else {
                    l += s * 0.95;
                    r += s * 0.65;
                }
                self.hat_amp[i] *= self.hat_dec[i];
            }
        }

        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_voices_render_finite_bounded_audio() {
        let sr = 44100.0;
        let inv_sr = 1.0 / sr;
        let mut pad = PadVoice::new(60, 12.0, 0.25, 0.0, 0.3);
        let mut choir_pad = PadVoice::new(64, 14.0, 0.25, 0.0, 0.3);
        let mut pad_chorus = PadChorus::new(sr);
        let mut bells = BellPool::new();
        bells.trigger(74, 0.8, 2.0, -0.3, 0.5, BellInstrument::Glass, sr);
        let mut bass = BassVoice::new(38, sr);
        let mut pulse = PulseSynth::new();
        pulse.trigger_kick(2, PulseInstrument::Soft, sr);
        pulse.trigger_hat(false, PulseInstrument::Soft, sr);
        let mut rng = Rng::new(77);

        for _ in 0..512 {
            let (pl, pr) = pad.next_stereo(inv_sr, 0.001, 0.5, PadInstrument::Rhodes, false);
            let (cl, cr) = choir_pad.next_stereo(inv_sr, 0.001, 0.5, PadInstrument::Choir, false);
            let (chl, chr) = pad_chorus.process(pl + cl, pr + cr, 0.5, inv_sr);
            let (bl, br) = bells.next_stereo();
            let bs = bass.next_sample(inv_sr, BassInstrument::Sub);
            let (dl, dr) = pulse.next_stereo(&mut rng);
            let sum_l = chl + bl + bs + dl;
            let sum_r = chr + br + bs + dr;
            assert!(sum_l.is_finite() && sum_r.is_finite());
            assert!(sum_l.abs() < 2.0 && sum_r.abs() < 2.0);
        }
    }
}
