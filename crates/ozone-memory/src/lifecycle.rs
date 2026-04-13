use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Provenance;

const PROVENANCE_DECAY_FACTOR: f32 = 0.85;
const MILLIS_PER_HOUR: u64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageTier {
    Full,
    Reduced,
    Minimal,
}

impl StorageTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Reduced => "reduced",
            Self::Minimal => "minimal",
        }
    }
}

impl fmt::Display for StorageTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageTierPolicy {
    pub reduced_after_messages: u64,
    pub minimal_after_messages: u64,
}

impl StorageTierPolicy {
    pub const fn new(reduced_after_messages: u64, minimal_after_messages: u64) -> Self {
        Self {
            reduced_after_messages,
            minimal_after_messages,
        }
    }
}

impl Default for StorageTierPolicy {
    fn default() -> Self {
        Self::new(100, 1000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactStaleness {
    pub age_messages: u64,
    pub age_hours: u64,
    pub stale_by_messages: bool,
    pub stale_by_hours: bool,
    pub is_stale: bool,
}

pub fn adjusted_provenance_weight(
    base: f32,
    provenance: Provenance,
    cycles_since_interaction: u32,
) -> f32 {
    match provenance {
        Provenance::SystemGenerated => {
            let cycles = i32::try_from(cycles_since_interaction).unwrap_or(i32::MAX);
            base * PROVENANCE_DECAY_FACTOR.powi(cycles)
        }
        _ => base,
    }
}

pub fn message_age_since_snapshot(snapshot_version: u64, current_message_count: u64) -> u64 {
    current_message_count.saturating_sub(snapshot_version)
}

pub fn storage_tier_for_age(age_messages: u64, policy: &StorageTierPolicy) -> StorageTier {
    if age_messages >= policy.minimal_after_messages {
        StorageTier::Minimal
    } else if age_messages >= policy.reduced_after_messages {
        StorageTier::Reduced
    } else {
        StorageTier::Full
    }
}

pub fn assess_artifact_staleness(
    snapshot_version: u64,
    current_message_count: u64,
    created_at_ms: i64,
    now_ms: i64,
    max_age_messages: usize,
    max_age_hours: u64,
) -> ArtifactStaleness {
    let age_messages = message_age_since_snapshot(snapshot_version, current_message_count);
    let age_ms = now_ms.saturating_sub(created_at_ms).max(0) as u64;
    let age_hours = age_ms / MILLIS_PER_HOUR;
    let max_age_messages = u64::try_from(max_age_messages).unwrap_or(u64::MAX);
    let stale_by_messages = age_messages > max_age_messages;
    let stale_by_hours = age_hours > max_age_hours;

    ArtifactStaleness {
        age_messages,
        age_hours,
        stale_by_messages,
        stale_by_hours,
        is_stale: stale_by_messages || stale_by_hours,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_decay_affects_only_system_generated_artifacts() {
        let base = 0.7;
        let cycles = 4;

        assert_ne!(
            adjusted_provenance_weight(base, Provenance::SystemGenerated, cycles),
            base
        );

        for provenance in [
            Provenance::UserAuthored,
            Provenance::CharacterCard,
            Provenance::Lorebook,
            Provenance::UtilityModel,
            Provenance::ImportedExternal,
        ] {
            assert_eq!(
                adjusted_provenance_weight(base, provenance, cycles),
                base,
                "{provenance} should not decay"
            );
        }
    }

    #[test]
    fn provenance_decay_matches_expected_curve() {
        let base = 0.7;
        let adjusted = adjusted_provenance_weight(base, Provenance::SystemGenerated, 3);

        assert!((adjusted - (base * 0.85_f32.powi(3))).abs() < f32::EPSILON);
    }

    #[test]
    fn message_age_saturates_correctly() {
        assert_eq!(message_age_since_snapshot(12, 5), 0);
        assert_eq!(message_age_since_snapshot(12, 18), 6);
    }

    #[test]
    fn storage_tiering_respects_default_boundaries() {
        let policy = StorageTierPolicy::default();

        assert_eq!(storage_tier_for_age(99, &policy), StorageTier::Full);
        assert_eq!(storage_tier_for_age(100, &policy), StorageTier::Reduced);
        assert_eq!(storage_tier_for_age(999, &policy), StorageTier::Reduced);
        assert_eq!(storage_tier_for_age(1000, &policy), StorageTier::Minimal);
    }

    #[test]
    fn stale_artifact_detects_message_age_threshold() {
        let staleness = assess_artifact_staleness(10, 511, 0, 0, 500, 168);

        assert_eq!(staleness.age_messages, 501);
        assert_eq!(staleness.age_hours, 0);
        assert!(staleness.stale_by_messages);
        assert!(!staleness.stale_by_hours);
        assert!(staleness.is_stale);
    }

    #[test]
    fn stale_artifact_detects_elapsed_hour_threshold() {
        let staleness =
            assess_artifact_staleness(10, 10, 0, (169 * MILLIS_PER_HOUR) as i64, 500, 168);

        assert_eq!(staleness.age_messages, 0);
        assert_eq!(staleness.age_hours, 169);
        assert!(!staleness.stale_by_messages);
        assert!(staleness.stale_by_hours);
        assert!(staleness.is_stale);
    }

    #[test]
    fn fresh_artifact_at_thresholds_remains_non_stale() {
        let staleness =
            assess_artifact_staleness(500, 1000, 0, (168 * MILLIS_PER_HOUR) as i64, 500, 168);

        assert_eq!(staleness.age_messages, 500);
        assert_eq!(staleness.age_hours, 168);
        assert!(!staleness.stale_by_messages);
        assert!(!staleness.stale_by_hours);
        assert!(!staleness.is_stale);
    }

    #[test]
    fn stale_artifact_time_saturates_when_created_in_future() {
        let staleness = assess_artifact_staleness(25, 20, 10_000, 5_000, 500, 168);

        assert_eq!(staleness.age_messages, 0);
        assert_eq!(staleness.age_hours, 0);
        assert!(!staleness.is_stale);
    }
}
