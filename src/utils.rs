#[cfg(feature = "rate-limiting")]
pub mod rate_limiting {
    use governor::{Quota, RateLimiter};
    use std::num::NonZeroU32;

    pub struct RateLimiter {
        limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    }

    impl RateLimiter {
        pub fn new(requests_per_second: u32) -> Self {
            let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap());
            Self {
                limiter: RateLimiter::direct(quota),
            }
        }

        pub async fn wait(&self) {
            self.limiter.until_ready().await;
        }
    }
}

#[cfg(feature = "circuit-breaker")]
pub mod circuit_breaker {
    use tokio_retry::strategy::{ExponentialBackoff, jitter};
    use tokio_retry::Retry;

    pub async fn with_retry<F, T, E, Fut>(operation: F) -> Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let strategy = ExponentialBackoff::from_millis(100)
            .map(jitter)
            .take(3);
        
        Retry::spawn(strategy, || operation()).await
    }
}