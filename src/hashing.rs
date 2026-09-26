//! Fast, dependency-free hashing for the compiler's internal lookup tables.
//!
//! std's default SipHash dominates the cost of small-map lookups (variable
//! scopes, function signatures, struct fields). The tables here never face
//! adversarial keys, so a simple FxHash-style mixer is enough and keeps the
//! zero-external-crate rule. Hash order is never observable: none of the
//! maps using this hasher are iterated in a way that reaches program output.

use std::hash::{BuildHasher, Hasher};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Default)]
pub struct FastBuild;

impl BuildHasher for FastBuild {
    type Hasher = FastHasher;
    fn build_hasher(&self) -> FastHasher {
        FastHasher(0)
    }
}

pub struct FastHasher(u64);

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0.rotate_left(5) ^ (*b as u64)).wrapping_mul(SEED);
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.0 = (self.0.rotate_left(5) ^ (n as u64)).wrapping_mul(SEED);
    }
}
