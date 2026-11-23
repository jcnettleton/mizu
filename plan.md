# Generative Music Engine Plan for `mizu`

## Goal
Build an original, clean-room generative ambient music engine in Rust for `mizu` that plays continuously and evolves in response to the particle simulation—without copying code from external projects.

### Clean-Room Reference Policy
- **AMB-1 (`fm1-amb`)** is used strictly as a **conceptual reference** (e.g., to understand how a multi-layer ambient system balances pad breathing cycles, unequal-length loops, Euclidean rhythms, and reverb).
- All Rust code in `mizu` will be written **from scratch** using standard, well-known DSP and music-theory algorithms (2-operator FM synthesis, state-variable filters, Bjorklund Euclidean rhythms, and Jon Dattorro's 1997 plate reverb paper).

---

## Architecture Overview

### Module Layout (`src/music/`)
- `src/music/mod.rs` — Public Bevy plugin (`MusicPlugin`), `MusicSettings` resource, and audio stream lifecycle.
- `src/music/bridge.rs` — Lock-free command channel (`SimAudioMetrics` / `MusicCommand`) from Bevy's main/sim thread to the real-time audio thread, plus parameter smoothing.
- `src/music/theory.rs` — Scales/modes (bitmask pitch-class pools), chord degree transitions, and minimal-movement voice leading.
- `src/music/sequencer.rs` — Slow macro `Arc` tide, chord drift timer, unequal-length bell loops, and Euclidean rhythm generator.
- `src/music/synth.rs` — Polyphonic breathing pad voices, 2-op FM bell voices, gliding bass, and soft percussion.
- `src/music/fx.rs` — Stereo echo, long tape-style delay loop, Dattorro plate reverb, and soft-clip output limiter.

### Threading & Audio Output
- **Main / Bevy Thread**: A system reads `SimState` and `Unit` velocities/positions (already computed during simulation) and sends smoothed target parameters over a lock-free ring buffer (e.g., `rtrb` or `crossbeam_channel`).
- **Audio Render Thread**: Driven by `cpal` (already in Bevy's dependency tree) at 44.1 kHz / 48 kHz stereo `f32`. Zero heap allocations, zero locks, and no blocking calls inside the audio callback.

---

## Phased Implementation

### Phase 1: Audio Pipeline & Minimal Pad
1. Add `cpal` (or a lightweight `rodio`/custom `Decodable` wrapper) and a lock-free ring buffer (`rtrb`).
2. Implement `MusicPlugin` in `src/main.rs` with a clean shutdown/restart lifecycle.
3. Build a minimal 2-voice stereo sine/triangle drone with smooth attack/release and a master volume/mute control.
4. **Verify**: Audio plays cleanly with no clicks, pops, or xruns while `mizu` runs at 120 Hz `FixedUpdate`.

### Phase 2: Music Theory & Generative Sequencers
1. **Scales & Chords (`theory.rs`)**:
   - Pitch-class bitmasks for modes (Ionian, Dorian, Lydian, Mixolydian, Aeolian, Major/Minor Pentatonic).
   - Diatonic chord tone extraction and smooth voice leading (each voice moves to the nearest valid chord tone).
2. **Sequencers (`sequencer.rs`)**:
   - **Chord Drift**: Slowly transitions between related scale degrees on a configurable timer.
   - **Bell Loops**: 4 independent note loops of unequal prime-ish lengths (e.g., `13, 17, 23, 29` steps) that phase against each other and occasionally mutate.
   - **Euclidean Pulse**: Bjorklund's algorithm generating polyrhythmic patterns (e.g., kick on 16 steps, hats on 12 and 7 steps).
3. **Verify**: Unit tests for scale extraction, voice leading, and Euclidean bitmasks.

### Phase 3: Synthesis Voices (`synth.rs`)
1. **Pad**: 4 independent voices on unequal breathing cycles (slow swell, hold, fade, short rest), with an FM Rhodes-like timbre and/or dual-formant vowel filter.
2. **Bells**: 2-operator FM voices with exponential amplitude/modulation-index decay and stereo panning per loop.
3. **Bass**: Fundamental sine + subtle sub-octave with exponential pitch glide following the chord root (and occasional fifth).
4. **Pulse**: Soft pitched thump + band-passed noise hats with subtle swing.

### Phase 4: Space & Effects (`fx.rs`)
1. **Echo**: Stereo feedback delay synced to tempo subdivisions.
2. **Tape Loop**: Long dual-head delay buffer (~4–6 seconds) with gentle low-pass darkening per repeat.
3. **Plate Reverb**: Dattorro-style plate reverb with adjustable size/decay and a **Freeze** state (cuts dry input into the tank and raises feedback near 1.0 so the current chord shimmers indefinitely).
4. **Output Stage**: Stereo `tanh` soft-clipper and master gain.

### Phase 5: Particle-to-Music Mapping (`bridge.rs`)
Compute a lightweight `SimAudioMetrics` summary each frame (or every few `FixedUpdate` ticks) and slew-limit all targets before applying them to DSP:

| `mizu` Simulation State | Musical Parameter | Behavior |
|---|---|---|
| `MotionMode` (`Fluid`, `LavaLamp`, `Swirl`, `ZeroG`) | Mode / Mood Preset | Shifts scale, pad timbre, and tempo character |
| Mean particle speed / kinetic energy | `Bright`, `Density`, `Pulse` level | Calm water = sparse & dark; agitated water = brighter & busier |
| `unit_count` & `unit_radius` | `Bass` warmth & `Pad` swell | More/larger particles add low-end weight |
| `ContainerShape` (`Rectangle`, `Circle`, `Hexagon`, `Triangle`) | Reverb `Size` / Echo character | Subtle acoustic space shift per container |
| Cursor `Force` / `Spawn` bursts | Bell accents & `Seed` stir | Interactive splashes gently trigger extra bell notes or nudge evolution |
| `SimState.paused` | `Freeze` | Freezes the reverb tail and pauses new note generation smoothly |

### Phase 6: UI & Polish (`src/ui.rs`)
1. Add compact music controls to `mizu`'s UI:
   - Mute / Volume slider.
   - **Reactive Mode** toggle (`Auto (Particles)` vs. manual Scale/Preset lock).
2. Tune parameter smoothing time constants so sudden particle resets or mode switches sound musical rather than abrupt.
3. Profile CPU usage on the audio thread (target < 2–3% of a single core).

### Phase 7: Musical Depth & Long-Session Polish
1. **Rest Bars (Intentional Silence)**: Allow `Pulse`, `Bass`, and individual `Bell` loops to occasionally sit out for a few bars (especially when particles are calm) so layers enter and exit organically.
2. **Pad Ensemble Chorus & Vowel-Formant `Choir`**: Add a gentle modulated stereo chorus + low-pass on the summed pad bus, and upgrade `PadInstrument::Choir` with parallel state-variable formant filters morphing across vowels (`/u/`, `/o/`, `/a/`, `/e/`).
3. **Dynamic Euclidean `Hits` & Swing**: Scale Euclidean hit counts (`k`) smoothly with particle energy and add optional 16th-note swing.
4. **Bass Fifth-Leaning**: Halfway through a chord's drift, allow the bass to occasionally glide to the chord's fifth before the next chord change.
5. **Slow Background `Arc` Tide**: Blend a slow 2–3 minute macro tide (`-1..+1`) with live particle energy so the music continues to swell and thin gently even when the simulation is in a steady state.
