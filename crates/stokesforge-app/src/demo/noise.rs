//! Pseudo-random noise **with a fixed seed** for the demo.
//!
//! No new dependency (`rand`): a local xorshift64* is enough to produce
//! "realistic" measurements and, above all, **the same ones at every launch**.
//! The analysis tests (`demo::tests`) recover the known parameters within a
//! tolerance, and the determinism test pins values: changing the seed or the
//! algorithm makes them fail, on purpose.

/// xorshift64* generator: non-zero state, period 2⁶⁴ − 1.
pub(super) struct Noise {
    state: u64,
}

impl Noise {
    /// A zero seed would be a fixed point of the generator: it is replaced.
    pub(super) fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A uniform number in `[0, 1)` (53 bits of mantissa).
    pub(super) fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number with mean 0 and standard deviation `sigma`: sum of twelve
    /// uniforms (central limit theorem), with no `ln` or `cos` to discuss.
    pub(super) fn gauss(&mut self, sigma: f64) -> f64 {
        let sum: f64 = (0..12).map(|_| self.uniform()).sum();
        (sum - 6.0) * sigma
    }

    /// `values` with noise of standard deviation `sigma` added.
    pub(super) fn noisy(&mut self, values: &[f64], sigma: f64) -> Vec<f64> {
        values.iter().map(|v| v + self.gauss(sigma)).collect()
    }
}
