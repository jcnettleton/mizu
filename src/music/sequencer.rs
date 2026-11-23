use crate::music::theory::{Mode, chord_tones, scale_steps};

pub const NUM_BELL_LOOPS: usize = 4;
pub const MAX_LOOP_LEN: usize = 29;
pub const BELL_LOOP_LENS: [usize; NUM_BELL_LOOPS] = [16, 16, 12, 24];
#[allow(dead_code)]
pub const DRUM_STEPS: [usize; 3] = [16, 12, 7];

#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub const fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    pub fn stir(&mut self, entropy: u32) {
        self.state ^= entropy.wrapping_mul(0x85EB_CA6B);
        if self.state == 0 {
            self.state = 0x9E37_79B9;
        }
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[inline]
    pub fn range(&mut self, max_exclusive: usize) -> usize {
        if max_exclusive == 0 {
            0
        } else {
            ((self.next_u32() >> 8) as usize) % max_exclusive
        }
    }
}

/// Bjorklund's Euclidean rhythm algorithm.
/// Returns a bitmask where bit `i` is 1 if step `i` triggers a hit, rotated by `rot`.
pub fn euclidean_pattern(k: usize, n: usize, rot: usize) -> u32 {
    if n == 0 || n > 32 || k == 0 {
        return 0;
    }
    let k = k.min(n);

    let mut groups: [u32; 32] = [0; 32];
    let mut lengths: [u8; 32] = [1; 32];
    for g in groups.iter_mut().take(k) {
        *g = 1;
    }

    let mut heads = k;
    let mut tails = n - k;
    let mut num_groups = n;

    while tails > 1 && heads > 0 {
        let pairs = heads.min(tails);
        for i in 0..pairs {
            let tail_idx = num_groups - pairs + i;
            groups[i] |= groups[tail_idx] << lengths[i];
            lengths[i] = lengths[i].saturating_add(lengths[tail_idx]);
        }
        if heads > tails {
            tails = heads - pairs;
        } else {
            tails -= pairs;
        }
        heads = pairs;
        num_groups = heads + tails;
    }

    let mut mask = 0u32;
    let mut pos = 0usize;
    for i in 0..num_groups {
        let len = lengths[i] as usize;
        for j in 0..len {
            if (groups[i] >> j) & 1 != 0 {
                let idx = (pos + j + rot) % n;
                mask |= 1u32 << idx;
            }
        }
        pos += len;
    }
    mask
}

#[derive(Debug, Clone, Copy)]
pub struct BellSlot {
    pub degree_offset: Option<i8>,
    #[allow(dead_code)]
    pub velocity: f32,
}

impl Default for BellSlot {
    fn default() -> Self {
        Self {
            degree_offset: None,
            velocity: 0.7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BellLoops {
    pub slots: [[BellSlot; MAX_LOOP_LEN]; NUM_BELL_LOOPS],
    pub pos: [usize; NUM_BELL_LOOPS],
    pub resting: [bool; NUM_BELL_LOOPS],
    pub passes_since_mutate: [u8; NUM_BELL_LOOPS],
}

impl BellLoops {
    pub fn new(rng: &mut Rng, density: f32) -> Self {
        let mut loops = Self {
            slots: [[BellSlot::default(); MAX_LOOP_LEN]; NUM_BELL_LOOPS],
            pos: [0; NUM_BELL_LOOPS],
            resting: [false; NUM_BELL_LOOPS],
            passes_since_mutate: [0; NUM_BELL_LOOPS],
        };
        for l in 0..NUM_BELL_LOOPS {
            loops.regrow(l, rng, density);
        }
        loops
    }

    pub fn regrow(&mut self, loop_idx: usize, rng: &mut Rng, density: f32) {
        let len = BELL_LOOP_LENS[loop_idx];
        for slot in self.slots[loop_idx][..len].iter_mut() {
            *slot = BellSlot::default();
        }
        self.passes_since_mutate[loop_idx] = 0;

        match loop_idx {
            0 => {
                // Lead motif: stepwise melodic contour anchored on chord tones on strong beats
                let target = Self::target_notes(0, density, 0.0, rng);
                let mut cur_deg: i8 = [0, 2, 4][rng.range(3)];
                // Favor downbeats & rhythmic groupings across 16 steps
                const LEAD_SLOTS: [usize; 10] = [0, 2, 3, 4, 6, 8, 10, 11, 12, 14];
                let mut placed = 0usize;
                for &s in &LEAD_SLOTS {
                    if placed >= target {
                        break;
                    }
                    // Leave a breath around step 6-7 or 14-15
                    if rng.next_f32() < 0.72 || s == 0 {
                        let deg = if s % 4 == 0 {
                            Self::nearest_chord_tone(cur_deg)
                        } else {
                            cur_deg
                        };
                        self.slots[0][s] = BellSlot {
                            degree_offset: Some(deg),
                            velocity: if s % 4 == 0 {
                                0.72 + 0.18 * rng.next_f32()
                            } else {
                                0.52 + 0.22 * rng.next_f32()
                            },
                        };
                        cur_deg = Self::next_stepwise_deg(deg, rng);
                        placed += 1;
                    }
                }
            }
            1 => {
                // Answer / counter-phrase: gentle response in the second half & gaps
                let target = Self::target_notes(1, density, 0.0, rng);
                let mut cur_deg: i8 = [2, 4, 0][rng.range(3)];
                const ANSWER_SLOTS: [usize; 6] = [5, 7, 9, 12, 13, 15];
                let mut placed = 0usize;
                for &s in &ANSWER_SLOTS {
                    if placed >= target {
                        break;
                    }
                    if rng.next_f32() < 0.65 {
                        self.slots[1][s] = BellSlot {
                            degree_offset: Some(cur_deg),
                            velocity: 0.48 + 0.20 * rng.next_f32(),
                        };
                        cur_deg = Self::next_stepwise_deg(cur_deg, rng);
                        placed += 1;
                    }
                }
            }
            _ => {
                // Loops 2 & 3: sparse harmonic shimmer (chord tones only)
                let target = Self::target_notes(loop_idx, density, 0.0, rng);
                for _ in 0..target {
                    self.add_melodic_note(loop_idx, rng);
                }
            }
        }
    }

    fn nearest_chord_tone(deg: i8) -> i8 {
        const CHORD_DEGS: [i8; 5] = [-3, 0, 2, 4, 7];
        let mut best = 0;
        let mut best_dist = i8::MAX;
        for &c in &CHORD_DEGS {
            let d = (c - deg).abs();
            if d < best_dist {
                best_dist = d;
                best = c;
            }
        }
        best
    }

    fn next_stepwise_deg(cur: i8, rng: &mut Rng) -> i8 {
        // Favor stepwise motion (-1, +1) with occasional small skips (-2, +2)
        const STEPS: [i8; 6] = [1, -1, 1, -1, 2, -2];
        let delta = STEPS[rng.range(STEPS.len())];
        let next = cur + delta;
        if next > 7 {
            cur - 1
        } else if next < -3 {
            cur + 1
        } else {
            next
        }
    }

    fn target_notes(loop_idx: usize, density: f32, arc_mod: f32, rng: &mut Rng) -> usize {
        let d = (density + 0.15 * arc_mod).clamp(0.0, 1.0);
        match loop_idx {
            0 => (3.0 + d * 4.5 + rng.next_f32() * 0.5).clamp(2.0, 8.0) as usize,
            1 => (1.5 + d * 3.0 + rng.next_f32() * 0.5).clamp(1.0, 5.0) as usize,
            _ => {
                if d < 0.25 {
                    0
                } else {
                    ((d - 0.15) * 3.0 + rng.next_f32() * 0.5).clamp(0.0, 3.0) as usize
                }
            }
        }
    }

    fn count_notes(&self, loop_idx: usize) -> usize {
        let len = BELL_LOOP_LENS[loop_idx];
        self.slots[loop_idx][..len]
            .iter()
            .filter(|s| s.degree_offset.is_some())
            .count()
    }

    fn prev_note_deg(&self, loop_idx: usize, before_slot: usize) -> i8 {
        let len = BELL_LOOP_LENS[loop_idx];
        for offset in 1..=len {
            let idx = (before_slot + len - offset) % len;
            if let Some(deg) = self.slots[loop_idx][idx].degree_offset {
                return deg;
            }
        }
        0
    }

    fn add_melodic_note(&mut self, loop_idx: usize, rng: &mut Rng) {
        let len = BELL_LOOP_LENS[loop_idx];
        for _ in 0..8 {
            let slot = rng.range(len);
            if self.slots[loop_idx][slot].degree_offset.is_none() {
                let deg = if loop_idx >= 2 {
                    // Shimmer voices stay on chord tones
                    const CHORD_DEGS: [i8; 4] = [0, 2, 4, 7];
                    CHORD_DEGS[rng.range(CHORD_DEGS.len())]
                } else {
                    let prev = self.prev_note_deg(loop_idx, slot);
                    let cand = Self::next_stepwise_deg(prev, rng);
                    if slot % 4 == 0 {
                        Self::nearest_chord_tone(cand)
                    } else {
                        cand
                    }
                };
                let vel = if loop_idx >= 2 {
                    0.35 + 0.25 * rng.next_f32()
                } else {
                    0.52 + 0.28 * rng.next_f32()
                };
                self.slots[loop_idx][slot] = BellSlot {
                    degree_offset: Some(deg),
                    velocity: vel,
                };
                return;
            }
        }
    }

    fn remove_random_note(&mut self, loop_idx: usize, rng: &mut Rng) {
        let len = BELL_LOOP_LENS[loop_idx];
        let mut active = [0usize; MAX_LOOP_LEN];
        let mut n = 0;
        for (i, slot) in self.slots[loop_idx][..len].iter().enumerate() {
            if slot.degree_offset.is_some() {
                active[n] = i;
                n += 1;
            }
        }
        if n > 0 {
            let pick = active[rng.range(n)];
            self.slots[loop_idx][pick].degree_offset = None;
        }
    }

    fn mutate_one_note(&mut self, loop_idx: usize, rng: &mut Rng) {
        let len = BELL_LOOP_LENS[loop_idx];
        let mut active = [0usize; MAX_LOOP_LEN];
        let mut n = 0;
        for (i, slot) in self.slots[loop_idx][..len].iter().enumerate() {
            if slot.degree_offset.is_some() {
                active[n] = i;
                n += 1;
            }
        }
        if n > 0 {
            let pick = active[rng.range(n)];
            if let Some(cur) = self.slots[loop_idx][pick].degree_offset {
                let next = if loop_idx >= 2 {
                    const CHORD_DEGS: [i8; 4] = [0, 2, 4, 7];
                    CHORD_DEGS[rng.range(CHORD_DEGS.len())]
                } else {
                    Self::next_stepwise_deg(cur, rng)
                };
                self.slots[loop_idx][pick].degree_offset = Some(next);
            }
        }
    }

    /// Advances all 4 bell loops by one step and returns triggered slots.
    pub fn step(
        &mut self,
        rng: &mut Rng,
        density: f32,
        evolve: f32,
        arc_mod: f32,
    ) -> [Option<BellSlot>; NUM_BELL_LOOPS] {
        let mut out = [None; NUM_BELL_LOOPS];
        for l in 0..NUM_BELL_LOOPS {
            let len = BELL_LOOP_LENS[l];
            let p = self.pos[l];
            if !self.resting[l] && self.slots[l][p].degree_offset.is_some() {
                out[l] = Some(self.slots[l][p]);
            }

            let next_p = p + 1;
            if next_p >= len {
                self.pos[l] = 0;
                self.on_pass_end(l, rng, density, evolve, arc_mod);
            } else {
                self.pos[l] = next_p;
            }
        }
        out
    }

    fn on_pass_end(
        &mut self,
        loop_idx: usize,
        rng: &mut Rng,
        density: f32,
        evolve: f32,
        arc_mod: f32,
    ) {
        let rest_base = if loop_idx == 0 { 0.08 } else { 0.16 };
        self.resting[loop_idx] =
            rng.next_f32() < evolve * rest_base + 0.16 * (-arc_mod).max(0.0);

        self.passes_since_mutate[loop_idx] = self.passes_since_mutate[loop_idx].saturating_add(1);

        let target = Self::target_notes(loop_idx, density, arc_mod, rng);
        let current = self.count_notes(loop_idx);
        if current < target {
            self.add_melodic_note(loop_idx, rng);
        } else if current > target {
            self.remove_random_note(loop_idx, rng);
        } else if current > 0
            && self.passes_since_mutate[loop_idx] >= 2
            && rng.next_f32() < evolve * 0.65
        {
            // Repeat the motif at least twice before gently nudging one note by a step
            self.mutate_one_note(loop_idx, rng);
            self.passes_since_mutate[loop_idx] = 0;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProgressionType {
    #[default]
    Drift,
    Ambient,
    Fifths,
    Pendulum,
}

impl ProgressionType {
    pub const ALL: [Self; 4] = [Self::Drift, Self::Ambient, Self::Fifths, Self::Pendulum];

    pub fn name(self) -> &'static str {
        match self {
            Self::Drift => "Drift",
            Self::Ambient => "Ambient",
            Self::Fifths => "Fifths",
            Self::Pendulum => "Pendulum",
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

#[derive(Debug, Clone)]
pub struct HarmonyState {
    pub root: u8,
    pub mode: Mode,
    pub progression: ProgressionType,
    pub degree: usize,
    pub step_index: usize,
    pub chord_pcs: [u8; 4],
    pub chord_len: usize,
    pub drift_secs: f32,
    pub elapsed_in_chord: f32,
}

impl HarmonyState {
    pub fn new(root: u8, mode: Mode, drift_secs: f32) -> Self {
        let (chord_pcs, chord_len) = chord_tones(mode.pool(), root, 0);
        Self {
            root: root % 12,
            mode,
            progression: ProgressionType::default(),
            degree: 0,
            step_index: 0,
            chord_pcs,
            chord_len,
            drift_secs: drift_secs.max(5.0),
            elapsed_in_chord: 0.0,
        }
    }

    pub fn set_progression(&mut self, progression: ProgressionType) {
        self.progression = progression;
        self.step_index = 0;
    }

    pub fn set_mode_and_root(&mut self, root: u8, mode: Mode) {
        self.root = root % 12;
        self.mode = mode;
        let (_, n_steps) = scale_steps(mode.pool());
        self.degree %= n_steps;
        let (pcs, len) = chord_tones(mode.pool(), self.root, self.degree);
        self.chord_pcs = pcs;
        self.chord_len = len;
    }

    /// Advances drift timer; returns `true` if the chord changed.
    pub fn tick(&mut self, dt: f32, rng: &mut Rng) -> bool {
        self.elapsed_in_chord += dt;
        if self.elapsed_in_chord >= self.drift_secs {
            self.elapsed_in_chord = 0.0;
            self.advance_chord(rng);
            true
        } else {
            false
        }
    }

    pub fn advance_chord(&mut self, rng: &mut Rng) {
        let (_, n_steps) = scale_steps(self.mode.pool());
        if n_steps <= 1 {
            return;
        }
        self.step_index = self.step_index.wrapping_add(1);
        let next = match self.progression {
            ProgressionType::Drift => {
                // Favor smooth harmonic moves: +/-1, +/-2, +/-3, or 4th/5th
                const MOVES: [isize; 6] = [1, -1, 2, -2, 3, 4];
                let delta = MOVES[rng.range(MOVES.len())];
                ((self.degree as isize + delta).rem_euclid(n_steps as isize)) as usize
            }
            ProgressionType::Ambient => {
                // Classic meditative 4-chord cycle: I -> VI -> III -> VII
                const CYCLE: [usize; 4] = [0, 5, 2, 6];
                CYCLE[self.step_index % CYCLE.len()] % n_steps
            }
            ProgressionType::Fifths => {
                // Circle-of-fifths root movement (up a diatonic 4th / down a 5th)
                (self.degree + 3) % n_steps
            }
            ProgressionType::Pendulum => {
                // Smooth stepwise rise and fall
                const PENDULUM: [usize; 8] = [0, 1, 2, 3, 4, 3, 2, 1];
                PENDULUM[self.step_index % PENDULUM.len()] % n_steps
            }
        };
        self.degree = next;
        let (pcs, len) = chord_tones(self.mode.pool(), self.root, self.degree);
        self.chord_pcs = pcs;
        self.chord_len = len;
    }
}

/// Slow macro-level sine tide (-1.0..=1.0) over ~2-3 minutes so the music
/// continues to swell and thin gently even during steady simulation states.
#[derive(Debug, Clone)]
pub struct ArcTide {
    phase: f32,
    rate_hz: f32,
    value: f32,
}

impl ArcTide {
    pub fn new(period_secs: f32) -> Self {
        Self {
            phase: 0.0,
            rate_hz: 1.0 / period_secs.max(30.0),
            value: 0.0,
        }
    }

    #[inline]
    pub fn tick(&mut self, dt_secs: f32) -> f32 {
        self.phase += self.rate_hz * dt_secs;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.value = (self.phase * std::f32::consts::TAU).sin();
        self.value
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_tresillo_and_cinquillo() {
        // E(3, 8) = 10010010 (bits 0, 3, 6)
        assert_eq!(euclidean_pattern(3, 8, 0), 0b0100_1001);
        // E(5, 8) = 10110110 -> 5 hits
        assert_eq!(euclidean_pattern(5, 8, 0).count_ones(), 5);
        assert_eq!(euclidean_pattern(0, 16, 0), 0);
    }

    #[test]
    fn bell_loops_advance_and_wrap() {
        let mut rng = Rng::new(42);
        let mut loops = BellLoops::new(&mut rng, 0.5);
        for _ in 0..30 {
            let _ = loops.step(&mut rng, 0.5, 0.3, 0.0);
        }
        assert_eq!(loops.pos[0], 30 % BELL_LOOP_LENS[0]);
        assert_eq!(loops.pos[1], 30 % BELL_LOOP_LENS[1]);
    }

    #[test]
    fn arc_tide_oscillates_within_unit_range() {
        let mut tide = ArcTide::new(120.0);
        let v_quarter = tide.tick(30.0);
        assert!((v_quarter - 1.0).abs() < 0.01);
        let v_three_quarters = tide.tick(60.0);
        assert!((v_three_quarters + 1.0).abs() < 0.01);
        assert!((tide.value() + 1.0).abs() < 0.01);
    }

    #[test]
    fn harmony_progression_types_advance_valid_degrees() {
        let mut rng = Rng::new(7);
        let mut h = HarmonyState::new(2, Mode::Dorian, 20.0);

        h.set_progression(ProgressionType::Ambient);
        h.advance_chord(&mut rng);
        assert_eq!(h.degree, 5);
        h.advance_chord(&mut rng);
        assert_eq!(h.degree, 2);

        h.set_progression(ProgressionType::Fifths);
        h.degree = 0;
        h.advance_chord(&mut rng);
        assert_eq!(h.degree, 3);
        h.advance_chord(&mut rng);
        assert_eq!(h.degree, 6);

        h.set_progression(ProgressionType::Pendulum);
        h.advance_chord(&mut rng);
        assert_eq!(h.degree, 1);
    }
}
