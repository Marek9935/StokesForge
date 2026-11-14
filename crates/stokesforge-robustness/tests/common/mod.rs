//! Helpers shared by the twin-session oracles `b22` and `b23`.
//!
//! Each test file includes this module (`mod common;`) and uses the helpers it needs.
//!
//! The twins' trees are compared exactly, transposed-sheet links and column
//! versions included (#439): a pass that finds the same content keeps the
//! version, so the targeted and full twins number alike.

use stokesforge_core::{NodeId, Tree};

/// The derived state of every link: (sheet, stale, modified). Compared across a
/// reload of the targeted twin, where the first pass must not change it (#430).
#[allow(dead_code)] // b23 only
pub fn link_states(tree: &Tree) -> Vec<(NodeId, bool, bool)> {
    tree.transposes()
        .iter()
        .map(|(sheet, link)| {
            let state = link.state(tree);
            (*sheet, state.stale, state.modified)
        })
        .collect()
}

// -- shared seed and scale (#522) ------------------------------------------
//
// `SF_PROPTEST_SEED` (a u64) replaces the fixed ChaCha seed and
// `SF_PROPTEST_SCALE` (an integer >= 1) multiplies the case count. Neither set:
// the draws are exactly the fixed ones. The weekly workflow sets both.

use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

pub const SEED_VAR: &str = "SF_PROPTEST_SEED";
pub const SCALE_VAR: &str = "SF_PROPTEST_SCALE";

/// Parses the seed variable: absent means the fixed seed; a value that is not
/// a u64 is a loud failure, never a silent fall back to the default.
#[allow(dead_code)] // not every battery draws
pub fn parse_seed(raw: Option<&str>) -> Option<u64> {
    raw.map(|v| {
        v.trim()
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{SEED_VAR} must be an unsigned integer, got {v:?}"))
    })
}

/// Parses the scale variable: absent means 1.
#[allow(dead_code)]
pub fn parse_scale(raw: Option<&str>) -> u32 {
    raw.map_or(1, |v| match v.trim().parse::<u32>() {
        Ok(s) if s >= 1 => s,
        _ => panic!("{SCALE_VAR} must be an integer >= 1, got {v:?}"),
    })
}

/// The generator for a seed: `None` is the fixed generator of the batteries.
#[allow(dead_code)]
pub fn rng_for(seed: Option<u64>) -> TestRng {
    match seed {
        None => TestRng::deterministic_rng(RngAlgorithm::ChaCha),
        Some(s) => {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&s.to_le_bytes());
            TestRng::from_seed(RngAlgorithm::ChaCha, &bytes)
        }
    }
}

#[allow(dead_code)]
pub fn env_seed() -> Option<u64> {
    parse_seed(std::env::var(SEED_VAR).ok().as_deref())
}

#[allow(dead_code)]
pub fn env_scale() -> u32 {
    parse_scale(std::env::var(SCALE_VAR).ok().as_deref())
}

/// What a failure message must carry to be replayed.
#[allow(dead_code)]
pub fn seed_note() -> String {
    format!(
        "replay with {SEED_VAR}={} {SCALE_VAR}={}",
        env_seed().map_or_else(|| "(unset: fixed seed)".to_string(), |s| s.to_string()),
        env_scale()
    )
}

/// Runner of a battery: `base_cases` times the scale, seeded from the
/// environment, no regression file on the disk of the repository.
#[allow(dead_code)]
pub fn env_runner(base_cases: u32) -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases: base_cases.saturating_mul(env_scale()),
            failure_persistence: None,
            ..Config::default()
        },
        rng_for(env_seed()),
    )
}
