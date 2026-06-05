# ternary-envelope

**Attack/decay/sustain/release of ternary ideas.** ADSR envelope shaping for ternary signals — how ideas enter, peak, hold, and fade in agent populations.

## Why This Exists

Ideas don't arrive at full strength and disappear instantly. An agent's contribution has a shape: it builds up (attack), settles (decay), holds steady (sustain), then fades (release). In synthesizer terms, this is the ADSR envelope.

In `ternary-tenforward`, we observed that agent contributions follow natural envelope patterns:

- **Attack**: An agent's first few contributions ramp up in assertiveness. Energy starts low and builds.
- **Decay**: After the initial burst, the agent settles to a sustained level. The first impassioned argument gives way to steady advocacy.
- **Sustain**: The agent maintains a consistent energy level for the middle of their contribution arc.
- **Release**: As the agent disengages or is superseded, their influence fades gradually.

Without explicit envelope modeling, all agent contributions are treated as rectangular — full on or full off. This creates discontinuities in the population dynamics. ADSR smoothing makes transitions natural and prevents the jarring on/off switches that cause cascade effects.

## The Physics Behind It

### ADSR Envelope Model

The classic ADSR model from analog synthesizers maps perfectly onto ternary agent dynamics:

```
Level
1.0 ┤     ╱‾‾‾‾‾‾‾‾‾‾╲
    │    ╱  sustain     ╲
    │   ╱ level          ╲
    │  ╱                   ╲
    │ ╱                     ╲
0.0 ├╱                       ╲
    └──┬──┬───────────────┬──┬──
       A  D       S       R
```

- **Attack** (`attack_samples`): Samples to ramp from 0.0 to 1.0. Short attack = agent bursts in. Long attack = agent warms up gradually.
- **Decay** (`decay_samples`): Samples to ramp from 1.0 to sustain level. After the initial burst, how quickly the agent settles.
- **Sustain** (`sustain_level`, 0.0-1.0): The steady-state amplitude. How much energy the agent maintains during their main contribution.
- **Release** (`release_samples`): Samples to ramp from sustain to 0.0. How gradually the agent disengages.

### Envelope Detection

`detect_envelope` extracts the amplitude envelope from a ternary signal using a running peak detector. Given a ternary stream, it tells you where the signal is active and how strongly. This is the inverse of `apply_envelope`: instead of shaping the signal, you're reading its shape.

### Gate Trigger

`gate_trigger` converts a ternary signal into a binary on/off gate. Non-zero values (Pos or Neg) = gate on. Zero = gate off. This is the fundamental signal that drives the ADSR: when the gate opens, attack+decay+sustain play. When the gate closes, release plays.

In agent dynamics, the gate represents when an agent is actively contributing. An agent in state 0 (reflecting) has their gate closed — they're listening, not shaping the conversation.

### Velocity Sensitivity

Higher velocity (assertiveness) compresses the attack time. A highly assertive agent (velocity 1.0) attacks instantly. A tentative agent (velocity 0.1) takes 10× longer to reach full amplitude. This models the real behavior where confident speakers make their point quickly while hesitant speakers take time to build to it.

### Legato Mode

In legato mode, re-triggering skips the release phase. The new note starts immediately from wherever the envelope was. For agents, this means: if an agent is already contributing and has something new to add, they don't fade out and come back in — they transition directly.

### Envelope Following

`EnvelopeFollower` is a real-time peak tracker with separate attack and release coefficients. It follows the magnitude of a ternary signal over time, rising quickly when activity increases and falling slowly when it decreases. This is how you implement level meters, compressors, and auto-gain in the ternary signal chain.

## Key Types and Functions

```rust
/// A ternary value.
pub enum Ternary { Neg, Zero, Pos }

/// Attack-Decay-Sustain-Release envelope profile.
pub struct AdsrProfile {
    pub attack_samples: usize,
    pub decay_samples: usize,
    pub sustain_level: f64,   // 0.0..1.0
    pub release_samples: usize,
}

impl AdsrProfile {
    pub fn new(attack: usize, decay: usize, sustain: f64, release: usize) -> Self
    pub fn level_at(&self, sample_idx: usize, total: usize) -> f64
}

/// Apply ADSR envelope to ternary signal → continuous f64.
pub fn apply_envelope(signal: &[Ternary], profile: &AdsrProfile) -> Vec<f64>

/// Apply envelope and re-quantize back to Ternary.
pub fn apply_envelope_ternary(signal: &[Ternary], profile: &AdsrProfile) -> Vec<Ternary>

/// Extract amplitude envelope (running peak window).
pub fn detect_envelope(signal: &[Ternary], window_size: usize) -> Vec<f64>

/// Generate gate signal: On when non-zero, Off when zero.
pub fn gate_trigger(signal: &[Ternary]) -> Vec<Gate>

/// Gate state.
pub enum Gate { On, Off }

/// Scale attack sharpness by velocity (0.0..1.0).
pub fn apply_velocity_sensitivity(profile: &AdsrProfile, velocity: f64) -> AdsrProfile

/// Skip release phase for legato re-triggering.
pub fn legato_retrigger(profile: &AdsrProfile) -> AdsrProfile

/// Real-time peak follower with attack/release coefficients.
pub struct EnvelopeFollower { /* ... */ }
impl EnvelopeFollower {
    pub fn new(attack_coeff: f64, release_coeff: f64) -> Self
    pub fn process(&mut self, sample: f64) -> f64
    pub fn process_ternary(&mut self, t: Ternary) -> f64
    pub fn peak(&self) -> f64
}
```

## Usage

### Basic ADSR

```rust
use ternary_envelope::{Ternary, AdsrProfile, apply_envelope};

let signal = vec![Ternary::Pos; 20];
let profile = AdsrProfile::new(3, 2, 0.6, 4);

let shaped = apply_envelope(&signal, &profile);
// Samples 0-2: attack ramp (0.0 → 1.0)
// Samples 3-4: decay ramp (1.0 → 0.6)
// Samples 5-15: sustain at 0.6
// Samples 16-19: release ramp (0.6 → 0.0)
```

### Envelope Detection

```rust
use ternary_envelope::{Ternary, detect_envelope};

let signal = vec![Ternary::Pos, Ternary::Pos, Ternary::Zero, Ternary::Neg];
let envelope = detect_envelope(&signal, 3);
// Running peak window shows where activity is concentrated
```

### Gate and Velocity

```rust
use ternary_envelope::{Ternary, gate_trigger, apply_velocity_sensitivity, AdsrProfile};

let signal = vec![Ternary::Pos, Ternary::Zero, Ternary::Neg, Ternary::Zero];
let gates = gate_trigger(&signal);
// [On, Off, On, Off]

let profile = AdsrProfile::new(100, 10, 0.7, 20);
let soft = apply_velocity_sensitivity(&profile, 0.5);  // attack × 2
let loud = apply_velocity_sensitivity(&profile, 1.0);  // attack unchanged
```

### Real-Time Following

```rust
use ternary_envelope::{Ternary, EnvelopeFollower};

let mut follower = EnvelopeFollower::new(1.0, 0.5);

// Process a stream of agent states
let states = vec![Ternary::Pos, Ternary::Pos, Ternary::Zero, Ternary::Pos];
let mut levels = Vec::new();
for state in &states {
    levels.push(follower.process_ternary(*state));
}
// Rises instantly on activity, decays slowly during silence
```

### Legato Mode

```rust
use ternary_envelope::{AdsrProfile, legato_retrigger};

let profile = AdsrProfile::new(10, 5, 0.8, 20);
let legato = legato_retrigger(&profile);
// release_samples = 0 — no fade-out between consecutive notes
```

## Connection to Ten-Forward Dynamics

The ADSR model explains several empirical observations:

| Phenomenon | ADSR Explanation |
|-----------|-----------------|
| Agents take ~5 ticks to reach full assertiveness | Attack phase |
| Initial passion gives way to steady advocacy | Decay → Sustain |
| Energy decay in anti-monoculture mechanism | Release phase (forced) |
| Fibonacci tunnel agents "burst in" | Instant attack (velocity=1.0) |
| Dominant speakers gradually lose influence | Slow release |

## In the Ternary Fleet

This is the **dynamics shaping** layer in the DJ metaphor product stack:

- `ternary-tenforward` — raw agent output needs envelope shaping
- `ternary-crossfader` — crossfade position affects envelope level
- `ternary-mixer` — channel strips carry ADSR-shaped signals
- **ternary-envelope** — shapes how contributions enter and leave
- `ternary-rack` — envelopes can be patched as rooms in the processing chain
- `ternary-grain` — grains have their own envelopes (Hann window)

## License

MIT
