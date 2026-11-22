pub mod bridge;
pub mod engine;
pub mod fx;
pub mod sequencer;
pub mod synth;
pub mod theory;

use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

use crate::music::bridge::{
    LayerConfig, MusicCommand, MusicRx, MusicTx, SimAudioParams, create_command_channel,
};
use crate::music::engine::MusicEngine;
use crate::music::theory::Mode;
use crate::sim::{ContainerShape, MotionMode, SimState, Unit};

#[derive(Resource, Debug, Clone)]
pub struct MusicSettings {
    pub muted: bool,
    pub volume: f32,
    pub frozen: bool,
    pub reactive: bool,
    pub layers: LayerConfig,
}

impl Default for MusicSettings {
    fn default() -> Self {
        Self {
            muted: false,
            volume: 0.6,
            frozen: false,
            reactive: true,
            layers: LayerConfig::default(),
        }
    }
}

#[derive(Resource)]
struct MusicChannel(MusicTx);

struct AudioStreamHandle {
    _stream: Stream,
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = create_command_channel();
        app.insert_resource(MusicSettings::default())
            .insert_resource(MusicChannel(tx))
            .insert_non_send_resource(AudioRxInit(Some(rx)))
            .add_systems(Startup, setup_audio_stream)
            .add_systems(
                Update,
                (handle_music_hotkeys, sync_music_settings, sync_sim_to_music).chain(),
            );
    }
}

struct AudioRxInit(Option<MusicRx>);

fn setup_audio_stream(world: &mut World) {
    let rx = world
        .get_non_send_resource_mut::<AudioRxInit>()
        .and_then(|mut init| init.0.take());
    let Some(rx) = rx else {
        return;
    };

    match start_cpal_stream(rx) {
        Ok(stream) => {
            world.insert_non_send_resource(AudioStreamHandle { _stream: stream });
        }
        Err(err) => {
            warn!("Generative music disabled (audio stream unavailable): {err}");
        }
    }
}

fn start_cpal_stream(rx: MusicRx) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|e| format!("default output config failed: {e}"))?;

    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let mut engine = MusicEngine::new(sample_rate, rx);
    let err_fn = |err| error!("Audio stream error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| engine.render_interleaved_f32(data, channels),
                err_fn,
                None,
            )
            .map_err(|e| format!("build f32 stream failed: {e}"))?,
        SampleFormat::I16 => device
            .build_output_stream(
                &config,
                move |data: &mut [i16], _| engine.render_interleaved_i16(data, channels),
                err_fn,
                None,
            )
            .map_err(|e| format!("build i16 stream failed: {e}"))?,
        SampleFormat::U16 => device
            .build_output_stream(
                &config,
                move |data: &mut [u16], _| engine.render_interleaved_u16(data, channels),
                err_fn,
                None,
            )
            .map_err(|e| format!("build u16 stream failed: {e}"))?,
        other => return Err(format!("unsupported sample format: {other:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("failed to start audio stream: {e}"))?;
    info!(
        "Generative music stream started ({} Hz, {} ch, {:?})",
        config.sample_rate.0, config.channels, sample_format
    );
    Ok(stream)
}

fn handle_music_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<MusicSettings>,
    channel: Res<MusicChannel>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        settings.muted = !settings.muted;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        channel.0.send(MusicCommand::NextChord);
    }
}

fn sync_music_settings(
    settings: Res<MusicSettings>,
    sim_state: Res<SimState>,
    channel: Res<MusicChannel>,
    mut last_frozen: Local<bool>,
) {
    if settings.is_changed() {
        channel.0.send(MusicCommand::SetMuted(settings.muted));
        channel.0.send(MusicCommand::SetVolume(settings.volume));
        channel.0.send(MusicCommand::SetLayers(settings.layers));
    }

    let frozen = settings.frozen || sim_state.paused;
    if frozen != *last_frozen || settings.is_changed() {
        *last_frozen = frozen;
        channel.0.send(MusicCommand::SetFreeze(frozen));
    }
}

fn mode_for_motion(motion: MotionMode) -> (u8, Mode) {
    match motion {
        MotionMode::Fluid => (2, Mode::Dorian),        // D Dorian
        MotionMode::LavaLamp => (5, Mode::Lydian),     // F Lydian
        MotionMode::Swirl => (7, Mode::Mixolydian),    // G Mixolydian
        MotionMode::ZeroG => (0, Mode::PentMaj),       // C Major Pentatonic
    }
}

fn sync_sim_to_music(
    time: Res<Time>,
    sim_state: Res<SimState>,
    settings: Res<MusicSettings>,
    mouse: Res<ButtonInput<MouseButton>>,
    units: Query<&Unit>,
    channel: Res<MusicChannel>,
    mut accum_secs: Local<f32>,
    mut splash_cooldown: Local<f32>,
    mut last_motion: Local<Option<MotionMode>>,
    mut last_unit_count: Local<usize>,
) {
    if !settings.reactive {
        return;
    }

    let dt = time.delta_secs();
    *accum_secs += dt;
    *splash_cooldown = (*splash_cooldown - dt).max(0.0);

    // Switch scale/mode smoothly when MotionMode changes
    if *last_motion != Some(sim_state.motion_mode) {
        *last_motion = Some(sim_state.motion_mode);
        let (root, mode) = mode_for_motion(sim_state.motion_mode);
        channel.0.send(MusicCommand::SetMode { root, mode });
    }

    // Trigger a gentle accent bell when spawning bursts or clicking with force
    let spawned_delta = sim_state.unit_count.saturating_sub(*last_unit_count);
    *last_unit_count = sim_state.unit_count;
    if (spawned_delta >= 5 || mouse.just_pressed(MouseButton::Left)) && *splash_cooldown <= 0.0 {
        *splash_cooldown = 0.22;
        let intensity = (spawned_delta as f32 / 30.0).clamp(0.35, 1.0);
        channel.0.send(MusicCommand::TriggerSplash { intensity });
        channel
            .0
            .send(MusicCommand::StirSeed(sim_state.rng_seed as u32));
    }

    // Sample particle telemetry at ~10 Hz
    if *accum_secs < 0.1 {
        return;
    }
    *accum_secs = 0.0;

    let mut count = 0usize;
    let mut speed_sum = 0.0_f32;
    let mut heat_sum = 0.0_f32;
    for u in &units {
        speed_sum += u.vel.length();
        heat_sum += u.heat;
        count += 1;
    }

    let mean_speed = if count > 0 {
        speed_sum / count as f32
    } else {
        0.0
    };
    let mean_heat = if count > 0 {
        heat_sum / count as f32
    } else {
        0.0
    };

    // Normalize speed (~0..350 px/s) and particle count (~0..6000) into 0..1 musical controls
    let energy = (mean_speed / 260.0 + mean_heat * 0.25).clamp(0.0, 1.0);
    let fullness = (count as f32 / 5000.0).clamp(0.0, 1.0);
    let radius_weight = ((sim_state.unit_radius - 1.5) / 8.5).clamp(0.0, 1.0);

    let (reverb_decay, reverb_mix): (f32, f32) = match sim_state.container_shape {
        ContainerShape::Rectangle => (0.72, 0.68),
        ContainerShape::Circle => (0.82, 0.78),
        ContainerShape::Hexagon => (0.76, 0.72),
        ContainerShape::Triangle => (0.66, 0.62),
    };

    let water_bonus = if sim_state.water_mode { 0.06 } else { 0.0 };
    let grav_tempo = if sim_state.gravity_enabled {
        ((sim_state.gravity_strength - 100.0) / 1400.0).clamp(0.0, 1.0) * 8.0
    } else {
        0.0
    };

    channel.0.send(MusicCommand::SetSimParams(SimAudioParams {
        brightness: (0.24 + 0.62 * energy).clamp(0.15, 0.92),
        density: (0.25 + 0.65 * energy).clamp(0.15, 0.95),
        pulse_level: (0.15 + 0.50 * energy).clamp(0.10, 0.70),
        bass_level: (0.28 + 0.35 * fullness + 0.15 * radius_weight).clamp(0.20, 0.75),
        reverb_decay: (reverb_decay + water_bonus).clamp(0.45, 0.88),
        reverb_mix: (reverb_mix + water_bonus).clamp(0.40, 0.86),
        tempo_bpm: 62.0 + 20.0 * energy + grav_tempo,
    }));
}
