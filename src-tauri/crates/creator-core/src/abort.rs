//! Cooperative cancellation token. Passed into long-running tasks
//! (transcription, silence detection, AI batches) so the frontend can
//! cancel without killing the whole tokio runtime.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Clone, Default)]
pub struct AbortFlag {
    inner: Arc<AtomicBool>,
}

impl AbortFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn abort(&self) {
        self.inner.store(true, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.inner.load(Ordering::Acquire)
    }

    /// Returns `Err(())` if aborted. Useful at iteration boundaries:
    /// `flag.check()?;`.
    pub fn check(&self) -> Result<(), ()> {
        if self.is_aborted() {
            Err(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_not_aborted() {
        let f = AbortFlag::new();
        assert!(!f.is_aborted());
        assert!(f.check().is_ok());
    }

    #[test]
    fn abort_propagates_across_clones() {
        let a = AbortFlag::new();
        let b = a.clone();
        assert!(!b.is_aborted());
        a.abort();
        assert!(b.is_aborted());
        assert!(b.check().is_err());
    }
}
