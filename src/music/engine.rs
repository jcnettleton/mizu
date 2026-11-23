use crate::music::bridge::{LayerConfig, MusicCommand, MusicRx, SimAudioParams};
use crate::music::fx::SpaceFx;
use crate::music::sequencer::{
    ArcTide, BellLoops, DRUM_STEPS, HarmonyState, NUM_BELL_LOOPS, Rng, euclidean_pattern,
};
use crate::music::synth::{BassVoice, BellPool, PadChorus, PadVoice, PulseSynth};
use crate::music::theory::{Mode, scale_steps, voice_lead};

const LOOP_BASE_MIDI: [u8; NUM_BELL_LOOPS] = [72, 67, 79, 62];
const LOOP_PAN: [f32; NUM_BELL_LOOPS] = [-0.6, 0.35, 0.7, -0.25];
const LOOP_FM_RATIO: [f32; NUM_BELL_LOOPS] = [2.0, 3.0, 3.5, 4.77];

pub struct MusicEngine {
    rx: MusicRx,
    sample_rate: f32,
    inv_sr: f32,
    muted: bool,
    frozen: bool,
    target_volume: f32,
    current_gain: f32,
    gain_slew: f32,
    freq_slew: f32,
    param_slew: f32,
    params: SimAudioParams,
    target_params: SimAudioParams,
    layers: LayerConfig,
    layer_gains: [f32; 4],
    evolve: f32,
    rng: Rng,
    harmony: HarmonyState,
    arc_tide: ArcTide,
    pad_notes: [u8; 4],
    pad_voices: [PadVoice; 4],
    pad_chorus: PadChorus,
    bell_loops: BellLoops,
    bells: BellPool,
    bass: BassVoice,
    bass_leaned_fifth: bool,
    bass_rest_bars: u8,
    bass_rest_gain: f32,
    bass_rest_slew: f32,
    pulse: PulseSynth,
    pulse_rest_bars: u8,
    drum_patterns: [u32; 3],
    drum_pos: [usize; 3],
    swing: f32,
    sixteenth_samples: f32,
    sixteenth_accum: f32,
    sixteenth_count: u32,
    fx: SpaceFx,
}

impl MusicEngine {
    pub fn new(sample_rate: f32, rx: MusicRx) -> Self {
        let sr = sample_rate.max(8000.0);
        let inv_sr = 1.0 / sr;
        let gain_slew = 1.0 - (-1.0 / (0.04 * sr)).exp();
        let freq_slew = 1.0 - (-1.0 / (0.65 * sr)).exp();
        let param_slew = 1.0 - (-1.0 / (0.35 * sr)).exp();
        let bass_rest_slew = 1.0 - (-1.0 / (0.55 * sr)).exp();

        let mut rng = Rng::new(0x5EED_1234);
        let harmony = HarmonyState::new(2, Mode::Dorian, 24.0); // D Dorian
        let arc_tide = ArcTide::new(150.0); // 2.5-minute macro tide
        let initial_notes = [50, 53, 57, 60];
        let pad_notes = voice_lead(
            &initial_notes,
            &harmony.chord_pcs[..harmony.chord_len],
            48,
            72,
        );
        let params = SimAudioParams::default();
        let bell_loops = BellLoops::new(&mut rng, params.density);

        let sixteenth_samples = (60.0 / params.tempo_bpm / 4.0) * sr;
        let drum_patterns = Self::compute_drum_patterns(params.density);

        Self {
            rx,
            sample_rate: sr,
            inv_sr,
            muted: false,
            frozen: false,
            target_volume: 0.6,
            current_gain: 0.0,
            gain_slew,
            freq_slew,
            param_slew,
            params,
            target_params: params,
            layers: LayerConfig::default(),
            layer_gains: [1.0; 4],
            evolve: 0.35,
            rng,
            bass: BassVoice::new(36 + harmony.chord_pcs[0], sr),
            bass_leaned_fifth: false,
            bass_rest_bars: 0,
            bass_rest_gain: 1.0,
            bass_rest_slew,
            harmony,
            arc_tide,
            pad_notes,
            pad_voices: [
                PadVoice::new(pad_notes[0], 12.0, 0.00, -0.55, 0.24),
                PadVoice::new(pad_notes[1], 14.5, 0.25, -0.18, 0.21),
                PadVoice::new(pad_notes[2], 17.2, 0.50, 0.18, 0.19),
                PadVoice::new(pad_notes[3], 20.1, 0.75, 0.55, 0.18),
            ],
            pad_chorus: PadChorus::new(sr),
            bell_loops,
            bells: BellPool::new(),
            pulse: PulseSynth::new(),
            pulse_rest_bars: 0,
            drum_patterns,
            drum_pos: [0; 3],
            swing: 0.14,
            sixteenth_samples,
            sixteenth_accum: 0.0,
            sixteenth_count: 0,
            fx: SpaceFx::new(sr),
        }
    }

    fn compute_drum_patterns(density: f32) -> [u32; 3] {
        let d = density.clamp(0.0, 1.0);
        let k0 = (1.0 + d * 4.0).round() as usize;
        let k1 = (2.0 + d * 5.0).round() as usize;
        let k2 = if d > 0.35 {
            (1.0 + (d - 0.35) * 3.0).round() as usize
        } else {
            0
        };
        [
            euclidean_pattern(k0, DRUM_STEPS[0], 0),
            euclidean_pattern(k1, DRUM_STEPS[1], 2),
            euclidean_pattern(k2, DRUM_STEPS[2], 3),
        ]
    }

    fn apply_chord_change(&mut self) {
        self.pad_notes = voice_lead(
            &self.pad_notes,
            &self.harmony.chord_pcs[..self.harmony.chord_len],
            48,
            72,
        );
        for (i, voice) in self.pad_voices.iter_mut().enumerate() {
            voice.set_target_note(self.pad_notes[i]);
        }
        self.bass.set_target_pc(self.harmony.chord_pcs[0], 36);
        self.bass_leaned_fifth = false;
    }

    fn bell_midi_note(&self, loop_idx: usize, degree_offset: i8) -> u8 {
        let (steps, n_steps) = scale_steps(self.harmony.mode.pool());
        let deg = (self.harmony.degree as isize + degree_offset as isize)
            .rem_euclid(n_steps as isize) as usize;
        let octave_shift = (degree_offset as isize).div_euclid(n_steps as isize) * 12;
        let pc = (self.harmony.root + steps[deg]) % 12;
        let base = LOOP_BASE_MIDI[loop_idx] as isize + octave_shift;
        let mut note = base - (base % 12) + pc as isize;
        while note < 55 {
            note += 12;
        }
        while note > 96 {
            note -= 12;
        }
        note as u8
    }

    fn on_sixteenth(&mut self, eff_density: f32, eff_brightness: f32, arc_mod: f32) {
        // Every bar (16 sixteenths): update dynamic Euclidean patterns and organic rest bars
        if self.sixteenth_count % 16 == 0 {
            self.drum_patterns = Self::compute_drum_patterns(eff_density);

            let calm = (1.0 - eff_density).clamp(0.0, 1.0);
            let tide_dip = (-arc_mod).max(0.0);

            if self.pulse_rest_bars > 0 {
                self.pulse_rest_bars -= 1;
            } else {
                let pulse_rest_prob = 0.08 + 0.22 * calm + 0.10 * tide_dip;
                if self.rng.next_f32() < pulse_rest_prob {
                    self.pulse_rest_bars = 1 + self.rng.range(3) as u8;
                }
            }

            if self.bass_rest_bars > 0 {
                self.bass_rest_bars -= 1;
            } else {
                let bass_rest_prob = 0.05 + 0.16 * calm + 0.08 * tide_dip;
                if self.rng.next_f32() < bass_rest_prob {
                    self.bass_rest_bars = 1 + self.rng.range(2) as u8;
                }
            }
        }

        // Step Euclidean percussion (skip triggers during rest bars so tails decay naturally)
        let pulse_active = self.layers.pulse_enabled && self.pulse_rest_bars == 0;
        for d in 0..3 {
            let pos = self.drum_pos[d];
            if pulse_active && (self.drum_patterns[d] >> pos) & 1 != 0 {
                match d {
                    0 => self.pulse.trigger_kick(
                        self.harmony.chord_pcs[0],
                        self.layers.pulse_instrument,
                        self.sample_rate,
                    ),
                    1 => self
                        .pulse
                        .trigger_hat(false, self.layers.pulse_instrument, self.sample_rate),
                    _ => self
                        .pulse
                        .trigger_hat(true, self.layers.pulse_instrument, self.sample_rate),
                }
            }
            self.drum_pos[d] = (pos + 1) % DRUM_STEPS[d];
        }

        // Advance bell loops on every quarter note (every 4 sixteenths)
        if self.sixteenth_count % 4 == 0 {
            let triggers =
                self.bell_loops
                    .step(&mut self.rng, eff_density, self.evolve, arc_mod);
            if self.layers.bells_enabled {
                for (l, maybe_slot) in triggers.into_iter().enumerate() {
                    if let Some(slot) = maybe_slot
                        && let Some(off) = slot.degree_offset
                    {
                        let note = self.bell_midi_note(l, off);
                        self.bells.trigger(
                            note,
                            slot.velocity,
                            LOOP_FM_RATIO[l],
                            LOOP_PAN[l],
                            eff_brightness,
                            self.layers.bell_instrument,
                            self.sample_rate,
                        );
                    }
                }
            }
        }

        self.sixteenth_count = self.sixteenth_count.wrapping_add(1);
    }

    fn drain_commands(&mut self) {
        while let Some(cmd) = self.rx.try_recv() {
            match cmd {
                MusicCommand::SetMuted(m) => self.muted = m,
                MusicCommand::SetVolume(v) => self.target_volume = v.clamp(0.0, 1.0),
                MusicCommand::SetFreeze(f) => self.frozen = f,
                MusicCommand::SetLayers(layers) => self.layers = layers,
                MusicCommand::SetMode { root, mode } => {
                    self.harmony.set_mode_and_root(root, mode);
                    self.apply_chord_change();
                }
                MusicCommand::SetProgression(p) => {
                    self.harmony.set_progression(p);
                }
                MusicCommand::SetSimParams(p) => {
                    self.target_params = SimAudioParams {
                        brightness: p.brightness.clamp(0.0, 1.0),
                        density: p.density.clamp(0.0, 1.0),
                        pulse_level: p.pulse_level.clamp(0.0, 1.0),
                        bass_level: p.bass_level.clamp(0.0, 1.0),
                        reverb_decay: p.reverb_decay.clamp(0.35, 0.90),
                        reverb_mix: p.reverb_mix.clamp(0.2, 0.90),
                        tempo_bpm: p.tempo_bpm.clamp(45.0, 110.0),
                    };
                }
                MusicCommand::TriggerSplash { intensity } => {
                    if !self.frozen && !self.muted && self.layers.bells_enabled {
                        let loop_idx = self.rng.range(NUM_BELL_LOOPS);
                        const SPLASH_OFFSETS: [i8; 4] = [0, 2, 4, 7];
                        let off = SPLASH_OFFSETS[self.rng.range(SPLASH_OFFSETS.len())];
                        let note = self.bell_midi_note(loop_idx, off);
                        let pan = self.rng.next_f32() * 1.2 - 0.6;
                        self.bells.trigger(
                            note,
                            (0.35 + 0.5 * intensity).clamp(0.2, 0.95),
                            LOOP_FM_RATIO[loop_idx],
                            pan,
                            (self.params.brightness + 0.15).min(1.0),
                            self.layers.bell_instrument,
                            self.sample_rate,
                        );
                    }
                }
                MusicCommand::NextChord => {
                    self.harmony.elapsed_in_chord = 0.0;
                    self.harmony.advance_chord(&mut self.rng);
                    self.apply_chord_change();
                }
                MusicCommand::StirSeed(s) => self.rng.stir(s),
            }
        }
    }

    pub fn render_frame(&mut self) -> (f32, f32) {
        let desired_gain = if self.muted { 0.0 } else { self.target_volume };
        self.current_gain += (desired_gain - self.current_gain) * self.gain_slew;

        let s = self.param_slew;
        self.params.brightness += (self.target_params.brightness - self.params.brightness) * s;
        self.params.density += (self.target_params.density - self.params.density) * s;
        self.params.pulse_level += (self.target_params.pulse_level - self.params.pulse_level) * s;
        self.params.bass_level += (self.target_params.bass_level - self.params.bass_level) * s;
        self.params.reverb_decay +=
            (self.target_params.reverb_decay - self.params.reverb_decay) * s;
        self.params.reverb_mix += (self.target_params.reverb_mix - self.params.reverb_mix) * s;
        self.params.tempo_bpm += (self.target_params.tempo_bpm - self.params.tempo_bpm) * s;

        self.sixteenth_samples = (60.0 / self.params.tempo_bpm / 4.0) * self.sample_rate;
        self.fx.reverb.decay = self.params.reverb_decay;
        self.fx.reverb_mix = self.params.reverb_mix;

        let arc_mod = if self.frozen {
            self.arc_tide.value()
        } else {
            self.arc_tide.tick(self.inv_sr)
        };
        let eff_brightness = (self.params.brightness + 0.12 * arc_mod).clamp(0.05, 0.95);
        let eff_density = (self.params.density + 0.15 * arc_mod).clamp(0.05, 0.98);

        if !self.frozen {
            if self.harmony.tick(self.inv_sr, &mut self.rng) {
                self.apply_chord_change();
            } else if !self.bass_leaned_fifth
                && self.harmony.elapsed_in_chord >= self.harmony.drift_secs * 0.5
            {
                self.bass_leaned_fifth = true;
                if self.rng.next_f32() < 0.55 {
                    let fifth_pc = if self.harmony.chord_len >= 3 {
                        self.harmony.chord_pcs[2]
                    } else {
                        (self.harmony.chord_pcs[0] + 7) % 12
                    };
                    self.bass.set_target_pc(fifth_pc, 36);
                }
            }

            let step_mult = if self.sixteenth_count % 2 == 0 {
                1.0 + self.swing
            } else {
                1.0 - self.swing
            };
            let step_samples = (self.sixteenth_samples * step_mult).max(1.0);
            self.sixteenth_accum += 1.0;
            if self.sixteenth_accum >= step_samples {
                self.sixteenth_accum -= step_samples;
                self.on_sixteenth(eff_density, eff_brightness, arc_mod);
            }
        }

        let targets = [
            if self.layers.pad_enabled { 1.0 } else { 0.0 },
            if self.layers.bells_enabled { 1.0 } else { 0.0 },
            if self.layers.bass_enabled { 1.0 } else { 0.0 },
            if self.layers.pulse_enabled { 1.0 } else { 0.0 },
        ];
        for (g, tgt) in self.layer_gains.iter_mut().zip(targets) {
            *g += (tgt - *g) * self.gain_slew;
        }
        let bass_rest_tgt = if self.bass_rest_bars == 0 { 1.0 } else { 0.0 };
        self.bass_rest_gain += (bass_rest_tgt - self.bass_rest_gain) * self.bass_rest_slew;

        let mut dry_l = 0.0_f32;
        let mut dry_r = 0.0_f32;
        let mut echo_l = 0.0_f32;
        let mut echo_r = 0.0_f32;

        let pad_g = self.layer_gains[0];
        let mut pad_l = 0.0_f32;
        let mut pad_r = 0.0_f32;
        for voice in &mut self.pad_voices {
            let (vl, vr) = voice.next_stereo(
                self.inv_sr,
                self.freq_slew,
                eff_brightness,
                self.layers.pad_instrument,
                self.frozen,
            );
            pad_l += vl;
            pad_r += vr;
        }
        let (pad_ch_l, pad_ch_r) =
            self.pad_chorus
                .process(pad_l, pad_r, eff_brightness, self.inv_sr);
        dry_l += pad_ch_l * 0.55 * pad_g;
        dry_r += pad_ch_r * 0.55 * pad_g;
        echo_l += pad_ch_l * 0.25 * pad_g;
        echo_r += pad_ch_r * 0.25 * pad_g;

        let bell_g = self.layer_gains[1];
        let (bl, br) = self.bells.next_stereo();
        dry_l += bl * 0.26 * bell_g;
        dry_r += br * 0.26 * bell_g;
        echo_l += bl * 0.38 * bell_g;
        echo_r += br * 0.38 * bell_g;

        let bass_g = self.layer_gains[2] * self.bass_rest_gain;
        let bass_s =
            self.bass.next_sample(self.inv_sr, self.layers.bass_instrument) * self.params.bass_level * bass_g;
        dry_l += bass_s;
        dry_r += bass_s;

        let (dl, dr) = self.pulse.next_stereo(&mut self.rng);
        let pulse_g = self.params.pulse_level * self.layer_gains[3];
        dry_l += dl * pulse_g;
        dry_r += dr * pulse_g;
        echo_l += dl * pulse_g * 0.35;
        echo_r += dr * pulse_g * 0.35;

        let (wet_l, wet_r) = self.fx.process(echo_l, echo_r, 0.55, 0.68, self.frozen);
        let l = (dry_l * 0.65 + wet_l * 0.55) * self.current_gain * 0.55;
        let r = (dry_r * 0.65 + wet_r * 0.55) * self.current_gain * 0.55;
        (l.tanh(), r.tanh())
    }

    pub fn render_interleaved_f32(&mut self, out: &mut [f32], channels: usize) {
        self.drain_commands();
        let ch = channels.max(1);
        for frame in out.chunks_mut(ch) {
            let (l, r) = self.render_frame();
            if ch == 1 {
                frame[0] = (l + r) * 0.5;
            } else {
                frame[0] = l;
                frame[1] = r;
                for extra in frame.iter_mut().skip(2) {
                    *extra = 0.0;
                }
            }
        }
    }

    pub fn render_interleaved_i16(&mut self, out: &mut [i16], channels: usize) {
        self.drain_commands();
        let ch = channels.max(1);
        for frame in out.chunks_mut(ch) {
            let (l, r) = self.render_frame();
            if ch == 1 {
                let mono = ((l + r) * 0.5).clamp(-1.0, 1.0);
                frame[0] = (mono * i16::MAX as f32) as i16;
            } else {
                frame[0] = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                frame[1] = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                for extra in frame.iter_mut().skip(2) {
                    *extra = 0;
                }
            }
        }
    }

    pub fn render_interleaved_u16(&mut self, out: &mut [u16], channels: usize) {
        self.drain_commands();
        let ch = channels.max(1);
        for frame in out.chunks_mut(ch) {
            let (l, r) = self.render_frame();
            if ch == 1 {
                let mono = ((l + r) * 0.5).clamp(-1.0, 1.0) * 0.5 + 0.5;
                frame[0] = (mono * u16::MAX as f32) as u16;
            } else {
                frame[0] = ((l.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
                frame[1] = ((r.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
                for extra in frame.iter_mut().skip(2) {
                    *extra = u16::MAX / 2;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::bridge::create_command_channel;

    #[test]
    fn renders_bounded_stereo_samples() {
        let (tx, rx) = create_command_channel();
        tx.send(MusicCommand::SetVolume(0.8));
        tx.send(MusicCommand::NextChord);
        let mut engine = MusicEngine::new(44100.0, rx);
        let mut buf = [0.0_f32; 256];
        engine.render_interleaved_f32(&mut buf, 2);

        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(buf.iter().any(|s| s.abs() > 0.0));
    }
}
