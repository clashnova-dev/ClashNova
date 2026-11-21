//! Bounded core recovery. Only a sustained healthy run resets the crash budget.
use std::time::{Duration, Instant};
const MAX_ATTEMPTS: u8 = 5;
const STABLE_AFTER: Duration = Duration::from_secs(120);
#[derive(Default)]
pub struct Recovery {
    attempts: u8,
    next: Option<Instant>,
    healthy_since: Option<Instant>,
}
impl Recovery {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn healthy(&mut self, now: Instant) {
        let since = *self.healthy_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= STABLE_AFTER {
            self.attempts = 0;
        }
    }
    pub fn schedule(&mut self, now: Instant) -> Option<(u8, u64)> {
        self.healthy(now);
        self.healthy_since = None;
        if self.attempts >= MAX_ATTEMPTS {
            self.next = None;
            return None;
        }
        let delay = 1u64 << self.attempts;
        self.attempts += 1;
        self.next = Some(now + Duration::from_secs(delay));
        Some((self.attempts, delay))
    }
    pub fn due(&mut self, now: Instant) -> bool {
        if self.next.is_some_and(|at| now >= at) {
            self.next = None;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_is_bounded_even_after_short_successes() {
        let mut r = Recovery::default();
        let mut now = Instant::now();
        for (i, delay) in [1, 2, 4, 8, 16].into_iter().enumerate() {
            assert_eq!(r.schedule(now), Some((i as u8 + 1, delay)));
            assert!(!r.due(now));
            now += Duration::from_secs(delay);
            assert!(r.due(now));
            assert!(!r.due(now));
            r.healthy(now);
            now += Duration::from_secs(3);
        }
        assert_eq!(r.schedule(now), None);
    }
    #[test]
    fn only_a_stable_run_or_user_action_resets_the_budget() {
        let mut r = Recovery::default();
        let now = Instant::now();
        r.schedule(now);
        r.healthy(now + Duration::from_secs(1));
        assert_eq!(r.schedule(now + Duration::from_secs(120)), Some((2, 2)));
        r.healthy(now + Duration::from_secs(121));
        r.healthy(now + Duration::from_secs(241));
        assert_eq!(r.schedule(now + Duration::from_secs(242)), Some((1, 1)));
        r.reset();
        assert!(!r.due(now + Duration::from_secs(1000)));
        assert_eq!(r.schedule(now), Some((1, 1)));
    }
}
