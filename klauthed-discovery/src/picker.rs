//! [`RoundRobin`] — client-side load balancing across resolved instances.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ServiceInstance;

/// A round-robin picker for client-side load balancing.
///
/// Hold one per logical service; each [`pick`](Self::pick) returns the next
/// instance in rotation. Cheap and lock-free (a single atomic counter), so it
/// can be shared behind an `Arc` across tasks.
#[derive(Debug, Default)]
pub struct RoundRobin {
    next: AtomicUsize,
}

impl RoundRobin {
    /// A fresh picker starting at the first instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pick the next instance in rotation, or `None` if `instances` is empty.
    ///
    /// The rotation index is per-picker, so passing slices of different lengths
    /// across calls is safe — the index is always reduced modulo the current
    /// length.
    #[must_use]
    pub fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        let index = self.next.fetch_add(1, Ordering::Relaxed) % instances.len();
        instances.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instances(n: usize) -> Vec<ServiceInstance> {
        (0..n).map(|i| ServiceInstance::new("svc", "host", 8000 + i as u16)).collect()
    }

    #[test]
    fn empty_yields_none() {
        assert!(RoundRobin::new().pick(&[]).is_none());
    }

    #[test]
    fn rotates_in_order_and_wraps() {
        let rr = RoundRobin::new();
        let list = instances(3);
        let picked: Vec<u16> = (0..7).filter_map(|_| rr.pick(&list)).map(|i| i.port).collect();
        assert_eq!(picked, vec![8000, 8001, 8002, 8000, 8001, 8002, 8000]);
    }

    #[test]
    fn tolerates_changing_lengths() {
        let rr = RoundRobin::new();
        let _ = rr.pick(&instances(5)); // advance the counter past a shorter slice
        let _ = rr.pick(&instances(5));
        let _ = rr.pick(&instances(5));
        // Now a single-instance slice must still resolve, not panic/overflow.
        assert!(rr.pick(&instances(1)).is_some());
    }
}
