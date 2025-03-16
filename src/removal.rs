//! Pure removal observation and finite row transition; no registry or process effects.
use std::time::Duration;

pub const CHECK_INTERVAL: Duration = Duration::from_secs(1);
pub const ROW_GRACE: Duration = Duration::from_secs(10);
pub const ROW_COLLAPSE: Duration = Duration::from_millis(260);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missing,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Waiting,
    Removed,
    VerificationUnavailable,
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub process_running: bool,
    pub exit_code: Option<u32>,
    pub monitoring: bool,
}
#[derive(Default)]
pub struct Observer {
    absent_reads: u8,
    confirmed_at: Option<Duration>,
    exited_at: Option<Duration>,
}
impl Observer {
    pub fn observe(
        &mut self,
        now: Duration,
        presence: Presence,
        process_running: bool,
        exit_code: Option<u32>,
    ) -> Status {
        match presence {
            Presence::Present => {
                self.absent_reads = 0;
                self.confirmed_at = None;
            }
            Presence::Unknown => {
                self.absent_reads = 0;
                self.confirmed_at = None;
            }
            Presence::Missing => {
                self.absent_reads = self.absent_reads.saturating_add(1);
                if self.absent_reads >= 2 && self.confirmed_at.is_none() {
                    self.confirmed_at = Some(now);
                }
            }
        }
        if !process_running && self.exited_at.is_none() {
            self.exited_at = Some(now);
        }
        let confirmed = self.confirmed_at.is_some() && presence != Presence::Unknown;
        let grace_finished = if confirmed {
            self.confirmed_at
                .is_some_and(|at| now.saturating_sub(at) >= Duration::from_secs(12))
        } else {
            self.exited_at
                .is_some_and(|at| now.saturating_sub(at) >= Duration::from_secs(120))
        };
        let done = (!process_running && grace_finished) || now >= Duration::from_secs(1800);
        let phase = if confirmed {
            Phase::Removed
        } else if done {
            Phase::Unconfirmed
        } else if presence == Presence::Unknown {
            Phase::VerificationUnavailable
        } else if process_running {
            Phase::Running
        } else {
            Phase::Waiting
        };
        Status {
            phase,
            process_running,
            exit_code,
            monitoring: !done,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RowProgress {
    pub seconds_left: u32,
    pub fraction: f32,
    pub finished: bool,
    pub next_wake: Duration,
}
pub fn row_progress(elapsed: Duration) -> RowProgress {
    if elapsed < ROW_GRACE {
        let seconds_left = (ROW_GRACE - elapsed).as_secs_f64().ceil() as u32;
        let next = Duration::from_secs(elapsed.as_secs() + 1).saturating_sub(elapsed);
        RowProgress {
            seconds_left,
            fraction: 1.,
            finished: false,
            next_wake: next,
        }
    } else {
        let t = (elapsed.saturating_sub(ROW_GRACE).as_secs_f32() / ROW_COLLAPSE.as_secs_f32())
            .clamp(0., 1.);
        let ease = t * t * (3. - 2. * t);
        RowProgress {
            seconds_left: 0,
            fraction: 1. - ease,
            finished: t >= 1.,
            next_wake: Duration::from_millis(16),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_exit_and_missing_uninstaller_do_not_prove_removal() {
        let mut observer = Observer::default();
        assert_eq!(
            observer
                .observe(Duration::ZERO, Presence::Present, false, Some(0))
                .phase,
            Phase::Waiting
        );
        assert_eq!(
            observer
                .observe(Duration::from_secs(120), Presence::Present, false, Some(0))
                .phase,
            Phase::Unconfirmed
        );
    }
    #[test]
    fn confirmation_needs_two_reads_and_registration_reappearance_cancels_it() {
        let mut observer = Observer::default();
        assert_ne!(
            observer
                .observe(Duration::ZERO, Presence::Missing, true, None)
                .phase,
            Phase::Removed
        );
        assert_eq!(
            observer
                .observe(Duration::from_secs(1), Presence::Missing, true, None)
                .phase,
            Phase::Removed
        );
        assert_eq!(
            observer
                .observe(Duration::from_secs(2), Presence::Present, true, None)
                .phase,
            Phase::Running
        );
    }
    #[test]
    fn unreadable_registry_never_counts_as_absence() {
        let mut observer = Observer::default();
        observer.observe(Duration::ZERO, Presence::Missing, false, None);
        assert_eq!(
            observer
                .observe(Duration::from_secs(1), Presence::Unknown, false, None)
                .phase,
            Phase::VerificationUnavailable
        );
        assert_eq!(
            observer
                .observe(Duration::from_secs(2), Presence::Missing, false, None)
                .phase,
            Phase::Waiting
        );
    }
    #[test]
    fn uncertainty_after_confirmation_requires_new_consecutive_evidence() {
        let mut observer = Observer::default();
        observer.observe(Duration::ZERO, Presence::Missing, false, Some(0));
        observer.observe(Duration::from_secs(1), Presence::Missing, false, Some(0));
        observer.observe(Duration::from_secs(2), Presence::Unknown, false, Some(0));
        assert_ne!(
            observer
                .observe(Duration::from_secs(3), Presence::Missing, false, Some(0))
                .phase,
            Phase::Removed
        );
        assert_eq!(
            observer
                .observe(Duration::from_secs(4), Presence::Missing, false, Some(0))
                .phase,
            Phase::Removed
        );
    }
    #[test]
    fn detached_launcher_is_observed_after_exit_and_monitor_eventually_sleeps() {
        let mut observer = Observer::default();
        observer.observe(Duration::ZERO, Presence::Present, false, Some(0));
        observer.observe(Duration::from_secs(5), Presence::Missing, false, Some(0));
        assert_eq!(
            observer
                .observe(Duration::from_secs(6), Presence::Missing, false, Some(0))
                .phase,
            Phase::Removed
        );
        assert!(
            !observer
                .observe(Duration::from_secs(18), Presence::Missing, false, Some(0))
                .monitoring
        );
    }
    #[test]
    fn row_holds_ten_seconds_then_collapses_monotonically_and_stops() {
        assert_eq!(row_progress(Duration::ZERO).seconds_left, 10);
        assert_eq!(row_progress(Duration::from_millis(9200)).seconds_left, 1);
        assert_eq!(row_progress(Duration::from_secs(10)).fraction, 1.);
        let middle = row_progress(Duration::from_millis(10130));
        assert!((middle.fraction - 0.5).abs() < 0.01);
        assert!(row_progress(ROW_GRACE + ROW_COLLAPSE).finished);
        assert_eq!(row_progress(ROW_GRACE + ROW_COLLAPSE).fraction, 0.);
    }
}
