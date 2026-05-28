use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct TtlCache<T: Clone + Send + Sync + 'static> {
    inner: RwLock<Option<(Instant, T)>>,
    ttl: Duration,
}

impl<T: Clone + Send + Sync + 'static> TtlCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: RwLock::new(None),
            ttl,
        }
    }

    pub async fn get_or_refresh<F, Fut>(&self, refresh: F) -> crate::Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = crate::Result<T>>,
    {
        {
            let guard = self.inner.read().await;
            if let Some((ts, val)) = guard.as_ref()
                && ts.elapsed() < self.ttl
            {
                return Ok(val.clone());
            }
        }
        let mut guard = self.inner.write().await;
        if let Some((ts, val)) = guard.as_ref()
            && ts.elapsed() < self.ttl
        {
            return Ok(val.clone());
        }
        let val = refresh().await?;
        *guard = Some((Instant::now(), val.clone()));
        Ok(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn returns_value_on_first_call() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_secs(60));
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        let val = cache
            .get_or_refresh(|| async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            })
            .await
            .unwrap();
        assert_eq!(val, 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn returns_cached_value_within_ttl() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_secs(60));
        let call_count = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let cc = call_count.clone();
            cache
                .get_or_refresh(|| async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(7)
                })
                .await
                .unwrap();
        }
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "refresh called more than once within TTL"
        );
    }

    #[tokio::test]
    async fn refreshes_after_ttl_expires() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_millis(10));
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        cache
            .get_or_refresh(|| async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Ok(1)
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let cc = call_count.clone();
        let val = cache
            .get_or_refresh(|| async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Ok(2)
            })
            .await
            .unwrap();
        assert_eq!(val, 2);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn propagates_refresh_error() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_secs(60));
        let result = cache
            .get_or_refresh(|| async { Err(crate::Error::Other("E_TEST".into())) })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cache_stays_empty_after_error() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_secs(60));
        let _ = cache
            .get_or_refresh(|| async { Err::<i32, _>(crate::Error::Other("fail".into())) })
            .await;
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        let val = cache
            .get_or_refresh(|| async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Ok(99)
            })
            .await
            .unwrap();
        assert_eq!(val, 99);
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "should have refreshed after previous error"
        );
    }
}
