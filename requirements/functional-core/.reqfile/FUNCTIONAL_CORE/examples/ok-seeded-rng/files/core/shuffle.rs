use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Deterministic for a given seed, so it stays pure.
pub fn pick(seed: u64, len: usize) -> usize {
    StdRng::seed_from_u64(seed).gen_range(0..len)
}
