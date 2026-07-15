// Maps to: TS internal/utils/sleep.ts
//
//! Async sleep helper.

use std::time::Duration;

/// Sleep for `ms` milliseconds.
///
/// Maps to TS `sleep(ms)`.
pub async fn sleep(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn sleep_waits_at_least_requested_milliseconds() {
        let start = Instant::now();
        sleep(5).await;
        assert!(start.elapsed() >= Duration::from_millis(5));
    }
}
