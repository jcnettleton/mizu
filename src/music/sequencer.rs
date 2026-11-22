use crate::music::theory::{Mode, chord_tones, scale_steps};

pub const NUM_BELL_LOOPS: usize = 4;
pub const MAX_LOOP_LEN: usize = 29;
pub const BELL_LOOP_LENS: [usize; NUM_BELL_LOOPS] = [13, 17, 23, 29];
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
}

impl BellLoops {
    pub fn new(rng: &mut Rng, density: f32) -> Self {
        let mut loops = Self {
            slots: [[BellSlot::default(); MAX_LOOP_LEN]; NUM_BELL_LOOPS],
            pos: [0; NUM_BELL_LOOPS],
            resting: [false; NUM_BELL_LOOPS],
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
        let target = Self::target_notes(density, 0.0, rng);
        for _ in 0..target {
            self.add_random_note(loop_idx, rng);
        }
    }

    fn target_notes(density: f32, arc_mod: f32, rng: &mut Rng) -> usize {
        let d = (density * 4.5 + 1.5 * arc_mod).clamp(0.0, 5.0);
        (d + rng.next_f32()) as usize
    }

    fn count_notes(&self, loop_idx: usize) -> usize {
        let len = BELL_LOOP_LENS[loop_idx];
        self.slots[loop_idx][..len]
            .iter()
            .filter(|s| s.degree_offset.is_some())
            .count()
    }

    fn add_random_note(&mut self, loop_idx: usize, rng: &mut Rng) {
        let len = BELL_LOOP_LENS[loop_idx];
        for _ in 0..8 {
            let slot = rng.range(len);
            if self.slots[loop_idx][slot].degree_offset.is_none() {
                // Bias toward chord-friendly intervals (0, 2, 4, 6) and occasional color tones
                const OFFSETS: [i8; 8] = [0, 2, 4, 6, 1, 3, 5, 7];
                let idx = if rng.next_f32() < 0.72 {
                    rng.range(4)
                } else {
                    rng.range(OFFSETS.len())
                };
                self.slots[loop_idx][slot] = BellSlot {
                    degree_offset: Some(OFFSETS[idx]),
                    velocity: 0.45 + 0.5 * rng.next_f32(),
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

    /// Advances all 4 bell loops by one step and returns triggered `(loop_idx, slot)` pairs.
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
        self.resting[loop_idx] = rng.next_f32() < evolve * 0.15 + 0.15 * (-arc_mod).max(0.0);
        let target = Self::target_notes(density, arc_mod, rng);
        let current = self.count_notes(loop_idx);
        if current < target {
            self.add_random_note(loop_idx, rng);
        } else if current > target {
            self.remove_random_note(loop_idx, rng);
        } else if current > 0 && rng.next_f32() < evolve * 0.55 {
            self.remove_random_note(loop_idx, rng);
            self.add_random_note(loop_idx, rng);
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarmonyState {
    pub root: u8,
    pub mode: Mode,
    pub degree: usize,
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
            degree: 0,
            chord_pcs,
            chord_len,
            drift_secs: drift_secs.max(5.0),
            elapsed_in_chord: 0.0,
        }
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
        // Favor smooth harmonic moves: +/-1, +/-2, +/-3, or 4th/5th
        const MOVES: [isize; 6] = [1, -1, 2, -2, 3, 4];
        let delta = MOVES[rng.range(MOVES.len())];
        let next = ((self.degree as isize + delta).rem_euclid(n_steps as isize)) as usize;
        self.degree = next;
        let (pcs, len) = chord_tones(self.mode.pool(), self.root, self.degree);
        self.chord_pcs = pcs;
        self.chord_len = len;
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
}
