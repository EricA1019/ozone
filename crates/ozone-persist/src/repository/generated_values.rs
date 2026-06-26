use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ozone_core::session::UnixTimestamp;

const UNIX_EPOCH_FALLBACK_SECS: u64 = 0;
const COUNTER_SHIFT_BITS: u32 = 64;
const PROCESS_ID_SHIFT_BITS: u32 = 32;
const UUID_VERSION_BYTE_INDEX: usize = 6;
const UUID_VERSION_MASK: u8 = 0x0f;
const UUID_VERSION_FOUR: u8 = 0x40;
const UUID_VARIANT_BYTE_INDEX: usize = 8;
const UUID_VARIANT_MASK: u8 = 0x3f;
const UUID_VARIANT_RFC4122: u8 = 0x80;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn safe_unix_duration() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(UNIX_EPOCH_FALLBACK_SECS))
}

pub(super) fn current_timestamp_ms() -> UnixTimestamp {
    i64::try_from(safe_unix_duration().as_millis()).unwrap_or(i64::MAX)
}

pub(super) fn generate_uuid_like() -> String {
    let counter = u128::from(ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let nanos = safe_unix_duration().as_nanos();
    let pid = u128::from(std::process::id());
    let mut bytes =
        (nanos ^ (counter << COUNTER_SHIFT_BITS) ^ (pid << PROCESS_ID_SHIFT_BITS)).to_be_bytes();

    bytes[UUID_VERSION_BYTE_INDEX] =
        (bytes[UUID_VERSION_BYTE_INDEX] & UUID_VERSION_MASK) | UUID_VERSION_FOUR;
    bytes[UUID_VARIANT_BYTE_INDEX] =
        (bytes[UUID_VARIANT_BYTE_INDEX] & UUID_VARIANT_MASK) | UUID_VARIANT_RFC4122;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{current_timestamp_ms, generate_uuid_like};

    const UUID_LENGTH: usize = 36;
    const UUID_HYPHEN_INDICES: [usize; 4] = [8, 13, 18, 23];
    const UUID_VERSION_INDEX: usize = 14;
    const UUID_VARIANT_INDEX: usize = 19;

    fn unix_ms_now() -> i64 {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));

        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    }

    #[test]
    fn generate_uuid_like_matches_expected_shape() {
        let id = generate_uuid_like();
        let bytes = id.as_bytes();

        assert_eq!(id.len(), UUID_LENGTH);
        for index in UUID_HYPHEN_INDICES {
            assert_eq!(bytes[index], b'-');
        }
        assert_eq!(bytes[UUID_VERSION_INDEX], b'4');
        assert!(matches!(
            bytes[UUID_VARIANT_INDEX],
            b'8' | b'9' | b'a' | b'b'
        ));
    }

    #[test]
    fn generate_uuid_like_changes_between_calls() {
        let first = generate_uuid_like();
        let second = generate_uuid_like();

        assert_ne!(first, second);
    }

    #[test]
    fn current_timestamp_ms_stays_within_system_time_bounds() {
        let lower_bound = unix_ms_now();
        let value = current_timestamp_ms();
        let upper_bound = unix_ms_now();

        assert!(value >= lower_bound);
        assert!(value <= upper_bound);
    }
}
