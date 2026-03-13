//! Exponential backoff retry policy for file transfers.
//!
//! Retries only on retriable errors (network failures). Non-retriable errors
//! (hash mismatch, rejection, file errors) fail immediately.

use std::time::Duration;
use tracing::warn;

use super::queue::TransferError;

/// Exponential backoff retry policy for file transfers.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Execute a transfer function with retry on retriable errors.
    /// Returns Ok on success, or the final error after all retries exhausted.
    pub async fn execute<F, Fut>(&self, mut f: F) -> Result<(), TransferError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<(), TransferError>>,
    {
        let mut attempt = 0u32;
        let mut delay = self.initial_delay;

        loop {
            match f().await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    if !err.is_retriable() {
                        warn!("Non-retriable transfer error: {}", err);
                        return Err(err);
                    }

                    attempt += 1;
                    if attempt > self.max_retries {
                        warn!(
                            "Transfer failed after {} retries: {}",
                            self.max_retries, err
                        );
                        return Err(err);
                    }

                    warn!(
                        "Transfer attempt {}/{} failed: {}. Retrying in {:?}",
                        attempt, self.max_retries, err, delay
                    );
                    tokio::time::sleep(delay).await;

                    // Exponential backoff with cap
                    delay = Duration::from_secs_f64(
                        (delay.as_secs_f64() * self.multiplier).min(self.max_delay.as_secs_f64()),
                    );
                }
            }
        }
    }

    /// Calculate the delay for a specific attempt (for testing/inspection).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let mut delay = self.initial_delay;
        for _ in 0..attempt {
            delay = Duration::from_secs_f64(
                (delay.as_secs_f64() * self.multiplier).min(self.max_delay.as_secs_f64()),
            );
        }
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn fast_policy(max_retries: u32) -> RetryPolicy {
        RetryPolicy {
            max_retries,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
        }
    }

    #[tokio::test]
    async fn test_retry_succeeds_first_attempt() {
        let policy = fast_policy(3);
        let result = policy.execute(|| async { Ok(()) }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let policy = fast_policy(3);
        let result = policy
            .execute(move || {
                let attempt = attempt_clone.clone();
                async move {
                    let n = attempt.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(TransferError::Network("timeout".to_string()))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempt.load(Ordering::SeqCst), 3); // 2 failures + 1 success
    }

    #[tokio::test]
    async fn test_retry_exhausted_returns_error() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let policy = fast_policy(2);
        let result = policy
            .execute(move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(TransferError::Network("connection refused".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        // 1 initial + 2 retries = 3 total attempts
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_no_retry_on_hash_mismatch() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let policy = fast_policy(3);
        let result = policy
            .execute(move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(TransferError::HashMismatch {
                        expected: "abc".to_string(),
                        actual: "def".to_string(),
                    })
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempt.load(Ordering::SeqCst), 1); // No retry
    }

    #[tokio::test]
    async fn test_retry_no_retry_on_rejected() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let policy = fast_policy(3);
        let result = policy
            .execute(move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(TransferError::Rejected("no space".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_exponential_backoff_delays() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay_for_attempt(0), Duration::from_secs(1));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_secs(2));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_secs(4));
    }

    #[test]
    fn test_backoff_capped_at_max_delay() {
        let policy = RetryPolicy {
            max_retries: 10,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        };
        // After enough doublings, should cap at 30s
        let delay = policy.delay_for_attempt(10);
        assert!(delay <= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_retry_with_fast_policy() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let policy = RetryPolicy {
            max_retries: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            multiplier: 2.0,
        };

        let start = std::time::Instant::now();
        let _result = policy
            .execute(move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(TransferError::Network("fail".to_string()))
                }
            })
            .await;

        // Should complete quickly with millisecond delays
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
