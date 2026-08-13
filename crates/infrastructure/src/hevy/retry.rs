//! Backing off when the source is struggling.
//!
//! No throttling is documented, advertised or observable on this API: thirty
//! rapid requests all succeeded, no response carries a rate-limit header, and
//! no throttling status appears anywhere in its published interface. That is
//! not a guarantee, and backoff costs nothing if the source never throttles —
//! whereas the cost of being wrong the other way is a run that fails partway,
//! which is safe but wasteful.

use std::time::Duration;

/// How a request is retried before the source is declared unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: u32,
    base: Duration,
    ceiling: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            // Four attempts spans roughly a minute of backoff, which is long
            // enough to ride out a restart and short enough that a genuinely
            // broken source is reported rather than waited on.
            attempts: 4,
            base: Duration::from_millis(500),
            ceiling: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    pub const fn new(attempts: u32, base: Duration, ceiling: Duration) -> Self {
        Self {
            attempts,
            base,
            ceiling,
        }
    }

    /// A policy that never waits, for tests that assert retry *counts* rather
    /// than timing. Sleeping to prove backoff exists makes a suite slow and
    /// tells you nothing the count does not.
    pub const fn immediate(attempts: u32) -> Self {
        Self {
            attempts,
            base: Duration::ZERO,
            ceiling: Duration::ZERO,
        }
    }

    pub const fn attempts(self) -> u32 {
        self.attempts
    }

    /// How long to wait before attempt `attempt`, counting from zero.
    ///
    /// Exponential, capped, with deterministic jitter derived from the attempt
    /// rather than a random source — a single-operator batch job has no
    /// thundering herd to disperse, and a reproducible delay is easier to
    /// reason about.
    pub fn backoff(self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        // The source's own instruction wins when it gives one.
        if let Some(requested) = retry_after {
            return requested.min(self.ceiling);
        }

        let factor = 1_u32.checked_shl(attempt).unwrap_or(u32::MAX);
        let delay = self.base.saturating_mul(factor);
        let jitter = self.base / 4 * u32::from(u8::try_from(attempt % 4).unwrap_or(0));
        delay.saturating_add(jitter).min(self.ceiling)
    }
}

/// Whether a status is worth trying again.
///
/// `429` and `5xx` are transient by nature. Everything else is not: a rejected
/// credential will not un-reject itself, and a malformed request is a bug in
/// ours rather than a fault in theirs.
pub fn is_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

#[cfg(test)]
mod tests {
    use super::{RetryPolicy, is_retryable};
    use std::time::Duration;

    #[test]
    fn only_throttling_and_server_faults_are_retried() {
        assert!(is_retryable(429));
        assert!(is_retryable(500));
        assert!(is_retryable(503));
        assert!(!is_retryable(200));
        assert!(!is_retryable(400));
        // A rejected credential is terminal: retrying looks like an attack.
        assert!(!is_retryable(401));
        assert!(!is_retryable(404));
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let policy = RetryPolicy::new(6, Duration::from_millis(100), Duration::from_secs(1));
        assert!(policy.backoff(0, None) > Duration::ZERO);
        assert!(policy.backoff(0, None) < policy.backoff(2, None));
        assert!(policy.backoff(20, None) <= Duration::from_secs(1));
    }

    #[test]
    fn the_sources_own_instruction_wins() {
        let policy = RetryPolicy::default();
        assert_eq!(
            policy.backoff(0, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
    }

    /// Even an instruction to wait is bounded: a source asking for an hour
    /// should not hang a run that long.
    #[test]
    fn an_absurd_retry_after_is_capped() {
        let policy = RetryPolicy::default();
        assert_eq!(
            policy.backoff(0, Some(Duration::from_hours(1))),
            Duration::from_secs(30)
        );
    }
}
