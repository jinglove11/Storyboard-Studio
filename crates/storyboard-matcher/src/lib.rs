//! Deterministic template matcher (plan §8-9).
//!
//! Rule filter + weighted scoring produce a Top-K with a full score
//! breakdown; the LLM (later phases) may only rerank *within* this K.
//! Selection: `top1 - top2 >= 0.15` picks top1 outright, otherwise a
//! score-weighted random draw inside the Top-K (seeded, reproducible).

pub mod intent;
pub mod score;

pub use intent::parse_intent;
pub use score::{Candidate, MatchMode, Matcher, MatcherConfig, ScoreBreakdown, ScoreWeights, Selection};

/// Tiny deterministic RNG (SplitMix64) so weighted random is reproducible
/// from a seed without an external dependency.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    /// f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_is_deterministic() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
