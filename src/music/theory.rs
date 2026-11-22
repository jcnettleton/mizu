#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Ionian,
    #[default]
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    PentMaj,
    PentMin,
}

impl Mode {
    #[allow(dead_code)]
    pub const ALL: [Self; 8] = [
        Self::Ionian,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Aeolian,
        Self::PentMaj,
        Self::PentMin,
    ];

    pub const fn pool(self) -> u16 {
        const fn mask(steps: &[u8]) -> u16 {
            let mut m = 0u16;
            let mut i = 0;
            while i < steps.len() {
                m |= 1u16 << steps[i];
                i += 1;
            }
            m
        }

        match self {
            Self::Ionian => mask(&[0, 2, 4, 5, 7, 9, 11]),
            Self::Dorian => mask(&[0, 2, 3, 5, 7, 9, 10]),
            Self::Phrygian => mask(&[0, 1, 3, 5, 7, 8, 10]),
            Self::Lydian => mask(&[0, 2, 4, 6, 7, 9, 11]),
            Self::Mixolydian => mask(&[0, 2, 4, 5, 7, 9, 10]),
            Self::Aeolian => mask(&[0, 2, 3, 5, 7, 8, 10]),
            Self::PentMaj => mask(&[0, 2, 4, 7, 9]),
            Self::PentMin => mask(&[0, 3, 5, 7, 10]),
        }
    }
}

#[inline]
pub fn midi_to_hz(note: f32) -> f32 {
    440.0 * ((note - 69.0) / 12.0).exp2()
}

/// Extracts ascending semitone offsets (0..12) from a 12-bit pitch-class pool.
/// Bit 0 (the root) is always included.
pub fn scale_steps(pool: u16) -> ([u8; 12], usize) {
    let pool = (pool & 0x0FFF) | 1;
    let mut steps = [0u8; 12];
    let mut count = 0;
    for semitone in 0..12u8 {
        if (pool >> semitone) & 1 != 0 {
            steps[count] = semitone;
            count += 1;
        }
    }
    (steps, count)
}

/// Builds a 3- or 4-note chord on `degree` by stacking diatonic thirds in `pool`.
/// Returns absolute pitch classes (0..12) and the count of distinct tones.
pub fn chord_tones(pool: u16, root: u8, degree: usize) -> ([u8; 4], usize) {
    const THIRD_STACK: [usize; 7] = [0, 2, 4, 6, 1, 3, 5];
    let (steps, n_steps) = scale_steps(pool);
    let want = if n_steps >= 6 { 4 } else { 3 };
    let deg = degree % n_steps;

    let mut pcs = [0u8; 4];
    let mut count = 0;
    for &offset in &THIRD_STACK {
        if count >= want {
            break;
        }
        let pc = (root + steps[(deg + offset) % n_steps]) % 12;
        if !pcs[..count].contains(&pc) {
            pcs[count] = pc;
            count += 1;
        }
    }
    (pcs, count)
}

fn nearest_in_range(from: u8, pc: u8, lo: u8, hi: u8) -> u8 {
    let mut best = lo;
    let mut best_dist = i16::MAX;
    let mut n = pc % 12;
    while n <= hi {
        if n >= lo {
            let dist = (n as i16 - from as i16).abs();
            if dist < best_dist {
                best_dist = dist;
                best = n;
            }
        }
        n = n.saturating_add(12);
    }
    best
}

/// Voice-leads 4 pad voices from `from` to target pitch classes `pcs`,
/// minimizing total semitone movement while keeping notes within `lo..=hi`.
pub fn voice_lead<const N: usize>(
    from: &[u8; N],
    pcs: &[u8],
    lo: u8,
    hi: u8,
) -> [u8; N] {
    if pcs.is_empty() {
        return *from;
    }

    let mut to = *from;
    let mut used_voice = [false; N];
    let mut used_pc = [false; N];

    // Pass 1: hold common tones first so shared chord notes never jump unnecessarily.
    for (p_idx, &pc) in pcs.iter().take(N).enumerate() {
        for (v, &cur) in from.iter().enumerate() {
            if !used_voice[v] && (cur % 12) == (pc % 12) && (lo..=hi).contains(&cur) {
                used_voice[v] = true;
                used_pc[p_idx] = true;
                to[v] = cur;
                break;
            }
        }
    }

    // Pass 2: assign remaining distinct pitch classes to the closest remaining voice.
    for (p_idx, &pc) in pcs.iter().take(N).enumerate() {
        if used_pc[p_idx] {
            continue;
        }
        let mut best_voice = 0;
        let mut best_note = lo;
        let mut best_dist = i16::MAX;

        for (v, &cur) in from.iter().enumerate() {
            if used_voice[v] {
                continue;
            }
            let cand = nearest_in_range(cur, pc, lo, hi);
            let dist = (cand as i16 - cur as i16).abs();
            if dist < best_dist {
                best_dist = dist;
                best_voice = v;
                best_note = cand;
            }
        }

        used_voice[best_voice] = true;
        to[best_voice] = best_note;
    }

    // Pass 3: any remaining voice (e.g. 4th voice on a triad) doubles the nearest chord tone.
    for (v, &cur) in from.iter().enumerate() {
        if used_voice[v] {
            continue;
        }
        let mut best_note = lo;
        let mut best_dist = i16::MAX;
        for &pc in pcs {
            let cand = nearest_in_range(cur, pc, lo, hi);
            let dist = (cand as i16 - cur as i16).abs();
            if dist < best_dist {
                best_dist = dist;
                best_note = cand;
            }
        }
        to[v] = best_note;
    }

    to
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_steps_extracts_dorian() {
        let (steps, count) = scale_steps(Mode::Dorian.pool());
        assert_eq!(count, 7);
        assert_eq!(&steps[..count], &[0, 2, 3, 5, 7, 9, 10]);
    }

    #[test]
    fn chord_tones_builds_dm7_in_d_dorian() {
        let (pcs, count) = chord_tones(Mode::Dorian.pool(), 2, 0);
        assert_eq!(count, 4);
        // D=2, F=5, A=9, C=0
        assert_eq!(&pcs[..count], &[2, 5, 9, 0]);
    }

    #[test]
    fn voice_lead_keeps_common_tones_and_stays_in_bounds() {
        let from = [50, 53, 57, 60]; // Dm7
        let (pcs, count) = chord_tones(Mode::Dorian.pool(), 2, 3); // G7: G(7), B(11), D(2), F(5)
        let to = voice_lead(&from, &pcs[..count], 48, 72);
        // D(50) and F(53) are common tones and should stay put
        assert_eq!(to[0], 50);
        assert_eq!(to[1], 53);
        assert!(to.iter().all(|&n| (48..=72).contains(&n)));
    }
}
