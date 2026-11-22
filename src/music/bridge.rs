use crossbeam_channel::{Receiver, Sender, bounded};

use crate::music::synth::{BassInstrument, BellInstrument, PadInstrument, PulseInstrument};
use crate::music::theory::Mode;

const COMMAND_QUEUE_CAP: usize = 256;

#[derive(Debug, Clone, Copy)]
pub struct SimAudioParams {
    pub brightness: f32,
    pub density: f32,
    pub pulse_level: f32,
    pub bass_level: f32,
    pub reverb_decay: f32,
    pub reverb_mix: f32,
    pub tempo_bpm: f32,
}

impl Default for SimAudioParams {
    fn default() -> Self {
        Self {
            brightness: 0.42,
            density: 0.42,
            pulse_level: 0.45,
            bass_level: 0.50,
            reverb_decay: 0.76,
            reverb_mix: 0.72,
            tempo_bpm: 72.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerConfig {
    pub pad_enabled: bool,
    pub bells_enabled: bool,
    pub bass_enabled: bool,
    pub pulse_enabled: bool,
    pub pad_instrument: PadInstrument,
    pub bell_instrument: BellInstrument,
    pub bass_instrument: BassInstrument,
    pub pulse_instrument: PulseInstrument,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            pad_enabled: true,
            bells_enabled: true,
            bass_enabled: true,
            pulse_enabled: true,
            pad_instrument: PadInstrument::default(),
            bell_instrument: BellInstrument::default(),
            bass_instrument: BassInstrument::default(),
            pulse_instrument: PulseInstrument::default(),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum MusicCommand {
    SetMuted(bool),
    SetVolume(f32),
    SetFreeze(bool),
    SetLayers(LayerConfig),
    SetMode { root: u8, mode: Mode },
    SetSimParams(SimAudioParams),
    TriggerSplash { intensity: f32 },
    NextChord,
    StirSeed(u32),
}

pub struct MusicTx {
    tx: Sender<MusicCommand>,
}

impl MusicTx {
    pub fn send(&self, cmd: MusicCommand) {
        let _ = self.tx.try_send(cmd);
    }
}

pub struct MusicRx {
    rx: Receiver<MusicCommand>,
}

impl MusicRx {
    pub fn try_recv(&self) -> Option<MusicCommand> {
        self.rx.try_recv().ok()
    }
}

pub fn create_command_channel() -> (MusicTx, MusicRx) {
    let (tx, rx) = bounded(COMMAND_QUEUE_CAP);
    (MusicTx { tx }, MusicRx { rx })
}
