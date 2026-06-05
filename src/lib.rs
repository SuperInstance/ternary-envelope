#![forbid(unsafe_code)]

//! Envelope/ADSR dynamics for ternary (-1, 0, +1) signals.

/// A ternary value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ternary {
    Neg,
    Zero,
    Pos,
}

impl Ternary {
    pub fn to_f64(self) -> f64 {
        match self {
            Ternary::Neg => -1.0,
            Ternary::Zero => 0.0,
            Ternary::Pos => 1.0,
        }
    }

    pub fn from_f64(v: f64) -> Self {
        if v < -0.33 {
            Ternary::Neg
        } else if v > 0.33 {
            Ternary::Pos
        } else {
            Ternary::Zero
        }
    }
}

// ── ADSR Profile ───────────────────────────────────────────────────

/// Attack-Decay-Sustain-Release envelope profile.
#[derive(Debug, Clone, PartialEq)]
pub struct AdsrProfile {
    pub attack_samples: usize,
    pub decay_samples: usize,
    pub sustain_level: f64,  // 0.0..1.0
    pub release_samples: usize,
}

impl AdsrProfile {
    pub fn new(attack: usize, decay: usize, sustain: f64, release: usize) -> Self {
        Self { attack_samples: attack, decay_samples: decay, sustain_level: sustain.clamp(0.0, 1.0), release_samples: release }
    }

    /// Compute the envelope level (0.0..1.0) at a given sample index within a note of `total` samples.
    pub fn level_at(&self, sample_idx: usize, total: usize) -> f64 {
        let a_end = self.attack_samples;
        let d_end = a_end + self.decay_samples;
        let r_start = total.saturating_sub(self.release_samples);

        if sample_idx >= total {
            return 0.0;
        }
        if sample_idx < a_end {
            // Attack: ramp 0 → 1
            if a_end == 0 { 1.0 } else { sample_idx as f64 / a_end as f64 }
        } else if sample_idx < d_end {
            // Decay: ramp 1 → sustain
            if self.decay_samples == 0 { self.sustain_level } else {
                let t = (sample_idx - a_end) as f64 / self.decay_samples as f64;
                1.0 - t * (1.0 - self.sustain_level)
            }
        } else if sample_idx < r_start {
            // Sustain
            self.sustain_level
        } else {
            // Release: ramp sustain → 0
            if self.release_samples == 0 { 0.0 } else {
                let t = (sample_idx - r_start) as f64 / self.release_samples as f64;
                self.sustain_level * (1.0 - t)
            }
        }
    }
}

// ── Apply envelope to ternary stream ───────────────────────────────

/// Apply an ADSR envelope to a ternary signal, returning continuous f64 values.
pub fn apply_envelope(signal: &[Ternary], profile: &AdsrProfile) -> Vec<f64> {
    let total = signal.len();
    signal.iter().enumerate().map(|(i, &t)| {
        t.to_f64() * profile.level_at(i, total)
    }).collect()
}

/// Apply envelope and re-quantize back to ternary.
pub fn apply_envelope_ternary(signal: &[Ternary], profile: &AdsrProfile) -> Vec<Ternary> {
    apply_envelope(signal, profile).into_iter().map(Ternary::from_f64).collect()
}

// ── Envelope detection ─────────────────────────────────────────────

/// Extract the envelope (peak magnitude over a running window) from a ternary signal.
pub fn detect_envelope(signal: &[Ternary], window_size: usize) -> Vec<f64> {
    if window_size == 0 || signal.is_empty() {
        return vec![0.0; signal.len()];
    }
    let mut result = Vec::with_capacity(signal.len());
    for i in 0..signal.len() {
        let start = i.saturating_sub(window_size / 2);
        let end = (i + window_size / 2 + 1).min(signal.len());
        let peak = signal[start..end].iter().map(|t| t.to_f64().abs()).fold(0.0_f64, f64::max);
        result.push(peak);
    }
    result
}

// ── Gate trigger ───────────────────────────────────────────────────

/// Gate state: on (note playing) or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    On,
    Off,
}

/// Generate a gate signal from a ternary stream: On when non-zero, Off when zero.
pub fn gate_trigger(signal: &[Ternary]) -> Vec<Gate> {
    signal.iter().map(|&t| match t {
        Ternary::Zero => Gate::Off,
        _ => Gate::On,
    }).collect()
}

// ── Velocity sensitivity ───────────────────────────────────────────

/// Scale the attack sharpness based on velocity (0.0..1.0).
/// Higher velocity → sharper (shorter) attack.
pub fn apply_velocity_sensitivity(profile: &AdsrProfile, velocity: f64) -> AdsrProfile {
    let v = velocity.clamp(0.01, 1.0);
    AdsrProfile {
        attack_samples: ((profile.attack_samples as f64) / v).round() as usize,
        ..profile.clone()
    }
}

// ── Legato mode ────────────────────────────────────────────────────

/// In legato mode, re-trigger without a full release. Returns a modified profile
/// that skips the release phase and goes directly into a new attack.
pub fn legato_retrigger(profile: &AdsrProfile) -> AdsrProfile {
    AdsrProfile {
        release_samples: 0,
        ..profile.clone()
    }
}

// ── Envelope follower ──────────────────────────────────────────────

/// Track peak level over time with a decay factor.
pub struct EnvelopeFollower {
    peak: f64,
    attack_coeff: f64,
    release_coeff: f64,
}

impl EnvelopeFollower {
    pub fn new(attack_coeff: f64, release_coeff: f64) -> Self {
        Self { peak: 0.0, attack_coeff, release_coeff }
    }

    pub fn process(&mut self, sample: f64) -> f64 {
        let abs = sample.abs();
        let coeff = if abs > self.peak { self.attack_coeff } else { self.release_coeff };
        self.peak = self.peak + coeff * (abs - self.peak);
        self.peak
    }

    pub fn process_ternary(&mut self, t: Ternary) -> f64 {
        self.process(t.to_f64())
    }

    pub fn peak(&self) -> f64 {
        self.peak
    }
}

// ════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn pos_signal(n: usize) -> Vec<Ternary> { vec![Ternary::Pos; n] }

    #[test]
    fn test_ternary_f64_roundtrip() {
        assert_eq!(Ternary::from_f64(Ternary::Neg.to_f64()), Ternary::Neg);
        assert_eq!(Ternary::from_f64(Ternary::Zero.to_f64()), Ternary::Zero);
        assert_eq!(Ternary::from_f64(Ternary::Pos.to_f64()), Ternary::Pos);
    }

    #[test]
    fn test_adsr_level_attack() {
        let p = AdsrProfile::new(4, 2, 0.5, 2);
        assert!((p.level_at(0, 10) - 0.0).abs() < 1e-9);
        assert!((p.level_at(2, 10) - 0.5).abs() < 1e-9);
        assert!((p.level_at(4, 10) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_adsr_level_sustain() {
        let p = AdsrProfile::new(2, 2, 0.6, 2);
        // Sample 6 in a 10-sample note: sustain phase (attack 0-1, decay 2-3, sustain 4-7, release 8-9)
        let level = p.level_at(6, 10);
        assert!((level - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_adsr_level_release() {
        let p = AdsrProfile::new(2, 2, 0.5, 2);
        let level = p.level_at(9, 10); // last sample before end
        assert!(level < 0.5);
        assert!(level > 0.0);
    }

    #[test]
    fn test_adsr_beyond_end() {
        let p = AdsrProfile::new(2, 2, 0.5, 2);
        assert_eq!(p.level_at(20, 10), 0.0);
    }

    #[test]
    fn test_apply_envelope_all_pos() {
        let sig = pos_signal(10);
        let p = AdsrProfile::new(5, 0, 1.0, 0);
        let env = apply_envelope(&sig, &p);
        // At sample 0: level = 0/5 = 0.0, signal = 1.0, output = 0.0
        assert!((env[0]).abs() < 1e-9);
        // At sample 5: level = 1.0, output = 1.0
        assert!((env[5] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_apply_envelope_ternary() {
        let sig = pos_signal(10);
        let p = AdsrProfile::new(1, 0, 1.0, 0);
        let result = apply_envelope_ternary(&sig, &p);
        assert_eq!(result[0], Ternary::Zero); // attack ramp start
        assert_eq!(result[5], Ternary::Pos);  // full level
    }

    #[test]
    fn test_detect_envelope() {
        let sig = vec![Ternary::Pos, Ternary::Pos, Ternary::Zero, Ternary::Neg];
        let env = detect_envelope(&sig, 3);
        assert_eq!(env.len(), 4);
        assert!((env[0] - 1.0).abs() < 1e-9); // peak of Pos
        assert!((env[2] - 1.0).abs() < 1e-9); // window includes Pos
    }

    #[test]
    fn test_gate_trigger() {
        let sig = vec![Ternary::Pos, Ternary::Zero, Ternary::Neg, Ternary::Zero];
        let gates = gate_trigger(&sig);
        assert_eq!(gates, vec![Gate::On, Gate::Off, Gate::On, Gate::Off]);
    }

    #[test]
    fn test_velocity_sensitivity() {
        let p = AdsrProfile::new(100, 10, 0.7, 20);
        let soft = apply_velocity_sensitivity(&p, 0.5);
        let loud = apply_velocity_sensitivity(&p, 1.0);
        assert!(soft.attack_samples > loud.attack_samples); // softer = longer attack
        assert_eq!(loud.attack_samples, 100);
    }

    #[test]
    fn test_legato_retrigger() {
        let p = AdsrProfile::new(10, 5, 0.8, 20);
        let legato = legato_retrigger(&p);
        assert_eq!(legato.release_samples, 0);
        assert_eq!(legato.attack_samples, 10); // unchanged
    }

    #[test]
    fn test_envelope_follower() {
        let mut follower = EnvelopeFollower::new(1.0, 0.5);
        let v = follower.process_ternary(Ternary::Pos);
        assert!((v - 1.0).abs() < 1e-9);
        let v2 = follower.process_ternary(Ternary::Zero);
        assert!(v2 < 1.0); // decaying
        assert!(v2 > 0.0);
    }

    #[test]
    fn test_envelope_follower_neg() {
        let mut follower = EnvelopeFollower::new(1.0, 0.5);
        let v = follower.process_ternary(Ternary::Neg);
        assert!((v - 1.0).abs() < 1e-9); // tracks abs
    }

    #[test]
    fn test_zero_length_envelope() {
        let env = apply_envelope(&[], &AdsrProfile::new(10, 10, 0.5, 10));
        assert!(env.is_empty());
    }

    #[test]
    fn test_adsr_instant_attack() {
        let p = AdsrProfile::new(0, 0, 1.0, 0);
        assert!((p.level_at(0, 10) - 1.0).abs() < 1e-9);
    }
}
