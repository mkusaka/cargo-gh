use anyhow::Result;
use backoff::future::retry;
use backoff::ExponentialBackoff;
use std::time::Duration;
use tracing::{info, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_interval: Duration,
    pub max_interval: Duration,
    pub max_elapsed_time: Option<Duration>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_interval: Duration::from_secs(1),
            max_interval: Duration::from_secs(30),
            max_elapsed_time: Some(Duration::from_secs(60)),
        }
    }
}

impl RetryConfig {
    /// Create an exponential backoff from this configuration
    pub fn to_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff {
            initial_interval: self.initial_interval,
            max_interval: self.max_interval,
            max_elapsed_time: self.max_elapsed_time,
            ..Default::default()
        }
    }
}

/// Execute an async operation with retry logic
pub async fn with_retry<F, Fut, T>(
    operation_name: &str,
    config: &RetryConfig,
    mut operation: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let backoff = config.to_backoff();
    let mut attempt = 0;

    retry(backoff, || {
        attempt += 1;
        let op = operation();
        
        async move {
            match op.await {
                Ok(result) => {
                    if attempt > 1 {
                        info!("{} succeeded on attempt {}", operation_name, attempt);
                    }
                    Ok(result)
                }
                Err(e) => {
                    if attempt <= config.max_retries {
                        warn!(
                            "{} failed on attempt {} of {}: {}. Retrying...",
                            operation_name, attempt, config.max_retries, e
                        );
                        Err(backoff::Error::transient(e))
                    } else {
                        warn!(
                            "{} failed after {} attempts: {}",
                            operation_name, config.max_retries, e
                        );
                        Err(backoff::Error::permanent(e))
                    }
                }
            }
        }
    })
    .await
}

/// Check if an error is retryable based on its characteristics
pub fn is_retryable_error(error: &anyhow::Error) -> bool {
    // Check if it's a network-related error that should be retried
    if let Some(reqwest_err) = error.downcast_ref::<reqwest::Error>() {
        // Retry on timeout, connection errors, and 5xx status codes
        return reqwest_err.is_timeout()
            || reqwest_err.is_connect()
            || reqwest_err
                .status()
                .map(|s| s.is_server_error() || s.as_u16() == 429) // 429 = Too Many Requests
                .unwrap_or(true); // Retry if no status code (network error)
    }

    // Check for IO errors that might be transient
    if let Some(io_err) = error.downcast_ref::<std::io::Error>() {
        use std::io::ErrorKind;
        matches!(
            io_err.kind(),
            ErrorKind::ConnectionAborted
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionRefused
                | ErrorKind::TimedOut
                | ErrorKind::Interrupted
                | ErrorKind::UnexpectedEof
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_on_transient_error() {
        let config = RetryConfig {
            max_retries: 3,
            initial_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(100),
            max_elapsed_time: Some(Duration::from_secs(1)),
        };

        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result = with_retry("test operation", &config, || {
            let count = attempt_count_clone.clone();
            async move {
                let attempts = count.fetch_add(1, Ordering::SeqCst);
                if attempts < 2 {
                    // Fail first two attempts
                    Err(anyhow::anyhow!("Transient error"))
                } else {
                    // Succeed on third attempt
                    Ok("success")
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_immediate_success_no_retry() {
        let config = RetryConfig::default();

        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result = with_retry("test operation", &config, || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok("immediate success")
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "immediate success");
        assert_eq!(attempt_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let config = RetryConfig {
            max_retries: 2,
            initial_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(50),
            max_elapsed_time: Some(Duration::from_secs(1)),
        };

        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result: Result<String> = with_retry("test operation", &config, || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("Persistent error"))
            }
        })
        .await;

        assert!(result.is_err());
        // Should attempt max_retries + 1 times (initial attempt + retries)
        assert_eq!(attempt_count.load(Ordering::SeqCst), config.max_retries + 1);
    }

    #[test]
    fn test_is_retryable_error() {
        // Test IO errors
        let io_error = anyhow::Error::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "Connection reset",
        ));
        assert!(is_retryable_error(&io_error));

        let io_error_not_retryable = anyhow::Error::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found",
        ));
        assert!(!is_retryable_error(&io_error_not_retryable));

        // Test other errors
        let other_error = anyhow::anyhow!("Some other error");
        assert!(!is_retryable_error(&other_error));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_interval, Duration::from_secs(1));
        assert_eq!(config.max_interval, Duration::from_secs(30));
        assert_eq!(config.max_elapsed_time, Some(Duration::from_secs(60)));
    }
}