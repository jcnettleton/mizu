use crate::music::bridge::{LayerConfig, MusicCommand, MusicRx, SimAudioParams};
use crate::music::fx::SpaceFx;
use crate::music::sequencer::{
    BellLoops, DRUM_STEPS, HarmonyState, NUM_BELL_LOOPS, Rng, euclidean_pattern,
};
use crate::music::synth::{BassVoice, BellPool, PadVoice, PulseSynth};
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
    pad_notes: [u8; 4],
    pad_voices: [PadVoice; 4],
    bell_loops: BellLoops,
    bells: BellPool,
    bass: BassVoice,
    pulse: PulseSynth,
    drum_patterns: [u32; 3],
    drum_pos: [usize; 3],
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

        let mut rng = Rng::new(0x5EED_1234);
        let harmony = HarmonyState::new(2, Mode::Dorian, 24.0); // D Dorian
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

        let drum_patterns = [
            euclidean_pattern(3, DRUM_STEPS[0], 0),
            euclidean_pattern(4, DRUM_STEPS[1], 2),
            euclidean_pattern(1, DRUM_STEPS[2], 3),
        ];

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
            harmony,
            pad_notes,
            pad_voices: [
                PadVoice::new(pad_notes[0], 12.0, 0.00, -0.55, 0.24),
                PadVoice::new(pad_notes[1], 14.5, 0.25, -0.18, 0.21),
                PadVoice::new(pad_notes[2], 17.2, 0.50, 0.18, 0.19),
                PadVoice::new(pad_notes[3], 20.1, 0.75, 0.55, 0.18),
            ],
            bell_loops,
            bells: BellPool::new(),
            pulse: PulseSynth::new(),
            drum_patterns,
            drum_pos: [0; 3],
            sixteenth_samples,
            sixteenth_accum: 0.0,
            sixteenth_count: 0,
            fx: SpaceFx::new(sr),
        }
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

    fn on_sixteenth(&mut self) {
        // Step Euclidean percussion
        for d in 0..3 {
            let pos = self.drum_pos[d];
            if self.layers.pulse_enabled && (self.drum_patterns[d] >> pos) & 1 != 0 {
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
                    .step(&mut self.rng, self.params.density, self.evolve, 0.0);
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
                            self.params.brightness,
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

        if !self.frozen {
            if self.harmony.tick(self.inv_sr, &mut self.rng) {
                self.apply_chord_change();
            }
            self.sixteenth_accum += 1.0;
            if self.sixteenth_accum >= self.sixteenth_samples {
                self.sixteenth_accum -= self.sixteenth_samples;
                self.on_sixteenth();
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

        let mut dry_l = 0.0_f32;
        let mut dry_r = 0.0_f32;
        let mut echo_l = 0.0_f32;
        let mut echo_r = 0.0_f32;

        let pad_g = self.layer_gains[0];
        for voice in &mut self.pad_voices {
            let (vl, vr) = voice.next_stereo(
                self.inv_sr,
                self.freq_slew,
                self.params.brightness,
                self.layers.pad_instrument,
                self.frozen,
            );
            dry_l += vl * 0.55 * pad_g;
            dry_r += vr * 0.55 * pad_g;
            echo_l += vl * 0.25 * pad_g;
            echo_r += vr * 0.25 * pad_g;
        }

        let bell_g = self.layer_gains[1];
        let (bl, br) = self.bells.next_stereo();
        dry_l += bl * 0.65 * bell_g;
        dry_r += br * 0.65 * bell_g;
        echo_l += bl * 0.85 * bell_g;
        echo_r += br * 0.85 * bell_g;

        let bass_g = self.layer_gains[2];
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
