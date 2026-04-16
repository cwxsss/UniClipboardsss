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
