//! Read-only access to the latest published snapshot and its lifecycle.
use crate::stats::Snapshot;
use std::sync::{Arc, Mutex};
#[cfg(feature = "snapshot-notifications")]
use {
    std::{io, sync::Weak},
    tokio::sync::Notify,
};

/// One publication. Generation zero is the initial snapshot; each publication
/// advances it independently of log counts. It is not part of CLI JSON.
#[derive(Clone, Debug)]
pub struct PublishedSnapshot {
    pub generation: u64,
    pub snapshot: Snapshot,
}

/// A read-only handle that can outlive the ingestion worker.
#[derive(Clone)]
pub struct SnapshotReader {
    pub(crate) slot: Arc<SnapshotSlot>,
}

impl SnapshotReader {
    /// Clone current state, releasing the lock before the caller processes it.
    pub fn latest(&self) -> PublishedSnapshot {
        self.slot
            .current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .latest
            .clone()
    }

    /// Whether ingestion has closed publication, including unwinding on panic.
    /// A closed reader still retains its last publication.
    pub fn is_closed(&self) -> bool {
        self.slot
            .current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .closed
    }

    /// Subscribe one asynchronous publisher to latest-value changes.
    ///
    /// Only one subscription may be alive at a time; distribute its serialized
    /// frames to clients separately. Dropping it allows another subscription.
    #[cfg(feature = "snapshot-notifications")]
    pub fn subscribe(&self) -> io::Result<SnapshotSubscription> {
        let mut current = self.slot.current.lock().unwrap_or_else(|p| p.into_inner());
        if current.subscriber.upgrade().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "snapshot publisher already subscribed",
            ));
        }
        let notification = Arc::new(Notify::new());
        current.subscriber = Arc::downgrade(&notification);
        Ok(SnapshotSubscription {
            reader: self.clone(),
            notification,
            seen: None,
        })
    }
}

/// An optional, single-consumer wakeup; the snapshot slot is the source of truth.
#[cfg(feature = "snapshot-notifications")]
pub struct SnapshotSubscription {
    reader: SnapshotReader,
    notification: Arc<Notify>,
    seen: Option<u64>,
}

#[cfg(feature = "snapshot-notifications")]
impl SnapshotSubscription {
    /// Return current state immediately on the first call, then wait for a new
    /// generation. Intermediate changes coalesce; there is no snapshot queue.
    ///
    /// After ingestion closes, deliver any unseen last publication before None.
    /// Cancelling this wait does not consume an unseen generation. No lock or
    /// snapshot borrow is retained while awaiting or processing the result.
    pub async fn changed(&mut self) -> Option<PublishedSnapshot> {
        loop {
            {
                let current = self
                    .reader
                    .slot
                    .current
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                if self.seen != Some(current.latest.generation) {
                    let latest = current.latest.clone();
                    self.seen = Some(latest.generation);
                    return Some(latest);
                }
                if current.closed {
                    return None;
                }
            }
            // notify_one retains a permit if publication races this await.
            // Generation checks discard stale permits and prevent lost updates.
            self.notification.notified().await;
        }
    }
}

#[cfg(feature = "snapshot-notifications")]
impl Drop for SnapshotSubscription {
    fn drop(&mut self) {
        let mut current = self
            .reader
            .slot
            .current
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if current
            .subscriber
            .ptr_eq(&Arc::downgrade(&self.notification))
        {
            // A publisher may still hold an in-flight notification Arc. It must
            // not prevent registering a replacement after this receiver drops.
            current.subscriber = Weak::new();
        }
    }
}

struct Current {
    latest: PublishedSnapshot,
    closed: bool,
    #[cfg(feature = "snapshot-notifications")]
    subscriber: Weak<Notify>,
}

pub(crate) struct SnapshotSlot {
    current: Mutex<Current>,
}

impl SnapshotSlot {
    pub(crate) fn new(snapshot: Snapshot) -> Self {
        Self {
            current: Mutex::new(Current {
                latest: PublishedSnapshot {
                    generation: 0,
                    snapshot,
                },
                closed: false,
                #[cfg(feature = "snapshot-notifications")]
                subscriber: Weak::new(),
            }),
        }
    }

    pub(crate) fn publish(&self, snapshot: Snapshot) {
        let mut current = self.current.lock().unwrap_or_else(|p| p.into_inner());
        current.latest.generation = current
            .latest
            .generation
            .checked_add(1)
            .expect("snapshot generation exhausted");
        current.latest.snapshot = snapshot;
        #[cfg(feature = "snapshot-notifications")]
        let subscriber = current.subscriber.upgrade();
        drop(current);
        #[cfg(feature = "snapshot-notifications")]
        if let Some(subscriber) = subscriber {
            subscriber.notify_one();
        }
    }
}

/// Close even when ingestion unwinds before producing a final snapshot.
pub(crate) struct CloseSnapshots(pub(crate) Arc<SnapshotSlot>);

impl Drop for CloseSnapshots {
    fn drop(&mut self) {
        let mut current = self.0.current.lock().unwrap_or_else(|p| p.into_inner());
        current.closed = true;
        #[cfg(feature = "snapshot-notifications")]
        let subscriber = current.subscriber.upgrade();
        drop(current);
        #[cfg(feature = "snapshot-notifications")]
        if let Some(subscriber) = subscriber {
            subscriber.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{State, Stats};
    use clap::Parser;
    use std::{thread, time::Duration};

    fn snapshot(state: State) -> Snapshot {
        Stats::new(1, "fixture".into()).snapshot(Duration::ZERO, state, 0, None)
    }

    #[test]
    fn reader_retains_latest_generation_after_writer_closes() {
        let slot = Arc::new(SnapshotSlot::new(snapshot(State::Reading)));
        let reader = SnapshotReader { slot: slot.clone() };
        assert_eq!(reader.latest().generation, 0);
        assert!(!reader.is_closed());
        slot.publish(snapshot(State::Following));
        slot.publish(snapshot(State::Complete));
        drop(CloseSnapshots(slot));
        let latest = reader.clone().latest();
        assert_eq!(latest.generation, 2);
        assert_eq!(latest.snapshot.state, State::Complete);
        assert!(reader.is_closed());
    }

    #[test]
    fn writer_panic_closes_readers_without_inventing_a_final_state() {
        let slot = Arc::new(SnapshotSlot::new(snapshot(State::Reading)));
        let reader = SnapshotReader { slot: slot.clone() };
        assert!(
            thread::spawn(move || {
                let _close = CloseSnapshots(slot);
                panic!("test worker panic");
            })
            .join()
            .is_err()
        );
        assert!(reader.is_closed());
        assert_eq!(reader.latest().snapshot.state, State::Reading);
        assert_eq!(reader.latest().generation, 0);
    }

    #[test]
    fn reader_outlives_running_and_retains_exact_eof_totals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        std::fs::write(&path, b"INFO complete\nWARN unterminated").unwrap();
        let args = crate::cli::Args::parse_from([
            std::ffi::OsStr::new("tell-tail"),
            path.as_os_str(),
            std::ffi::OsStr::new("--once"),
        ]);
        let mut running = crate::runtime::Running::start(&args, Arc::default()).unwrap();
        let reader = running.snapshot_reader();
        running.join().unwrap();
        drop(running);
        let publication = reader.latest();
        assert_eq!(publication.snapshot.state, State::Complete);
        assert_eq!(publication.snapshot.totals.records, 2);
        assert!(publication.generation > 0);
        assert!(reader.is_closed());
    }
}

#[cfg(all(test, feature = "snapshot-notifications"))]
mod notification_tests {
    use super::*;
    use crate::stats::{State, Stats};
    use clap::Parser;
    use std::{
        future::Future,
        pin::pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll, Wake, Waker},
        thread,
        time::{Duration, Instant},
    };

    fn value(records: u64, state: State) -> Snapshot {
        let mut value = Stats::new(1, "fixture".into()).snapshot(Duration::ZERO, state, 0, None);
        value.totals.records = records;
        value
    }

    fn channel() -> (Arc<SnapshotSlot>, SnapshotReader) {
        let slot = Arc::new(SnapshotSlot::new(value(0, State::Reading)));
        let reader = SnapshotReader { slot: slot.clone() };
        (slot, reader)
    }

    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    // Drive the async contract without adding a Tokio runtime to CLI tests.
    fn receive(subscription: &mut SnapshotSubscription) -> Option<PublishedSnapshot> {
        let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut next = pin!(subscription.changed());
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Poll::Ready(value) = next.as_mut().poll(&mut context) {
                return value;
            }
            assert!(Instant::now() < deadline, "notification wait timed out");
            thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
        }
    }

    struct CountWake(AtomicUsize);
    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn initial_state_and_slow_consumer_deliver_only_latest_generation() {
        let (slot, reader) = channel();
        let mut subscription = reader.subscribe().unwrap();
        assert_eq!(receive(&mut subscription).unwrap().generation, 0);
        for n in 1..=128 {
            slot.publish(value(n, State::Following));
        }
        let latest = receive(&mut subscription).unwrap();
        assert_eq!(latest.generation, 128);
        assert_eq!(latest.snapshot.totals.records, 128);
        // Consume any stale wakeup permit, then remain pending without a change.
        let waker = Waker::noop();
        let mut next = pin!(subscription.changed());
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(waker))
                .is_pending()
        );
    }

    #[test]
    fn cancellation_after_wakeup_does_not_consume_the_publication() {
        let (slot, reader) = channel();
        let mut subscription = reader.subscribe().unwrap();
        receive(&mut subscription).unwrap();
        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut next = Box::pin(subscription.changed());
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        for n in 1..=128 {
            slot.publish(value(n, State::Following));
        }
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        drop(next);
        assert_eq!(receive(&mut subscription).unwrap().generation, 128);
    }

    #[test]
    fn dropping_receiver_releases_registration_even_with_an_inflight_wakeup() {
        let (_slot, reader) = channel();
        let subscription = reader.subscribe().unwrap();
        assert_eq!(
            reader.subscribe().err().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        let inflight = subscription.notification.clone();
        drop(subscription);
        let mut replacement = reader.subscribe().unwrap();
        inflight.notify_one();
        assert_eq!(receive(&mut replacement).unwrap().generation, 0);
    }

    #[test]
    fn close_wakes_a_pending_receiver_without_fabricating_a_final_state() {
        let (slot, reader) = channel();
        let mut subscription = reader.subscribe().unwrap();
        receive(&mut subscription).unwrap();
        let waker = Waker::noop();
        let mut next = pin!(subscription.changed());
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(waker))
                .is_pending()
        );
        drop(CloseSnapshots(slot));
        assert!(matches!(
            next.as_mut().poll(&mut Context::from_waker(waker)),
            Poll::Ready(None)
        ));
        assert_eq!(reader.latest().snapshot.state, State::Reading);
    }

    #[test]
    fn racing_publication_and_close_never_lose_the_final_snapshot() {
        for _ in 0..64 {
            let (slot, reader) = channel();
            let mut subscription = reader.subscribe().unwrap();
            let worker = thread::spawn(move || {
                let _close = CloseSnapshots(slot.clone());
                slot.publish(value(7, State::Complete));
            });
            let mut last = None;
            while let Some(publication) = receive(&mut subscription) {
                last = Some(publication);
            }
            worker.join().unwrap();
            let last = last.unwrap();
            assert_eq!(last.snapshot.state, State::Complete);
            assert_eq!(last.snapshot.totals.records, 7);
            assert_eq!(last.generation, 1);
            drop(subscription);
            let mut late = reader.subscribe().unwrap();
            assert_eq!(receive(&mut late).unwrap().generation, 1);
            assert!(receive(&mut late).is_none());
        }
    }

    #[test]
    fn live_ingestion_notifies_idle_expiry_and_retains_final_totals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        std::fs::write(&path, b"INFO active\nWARN active\n").unwrap();
        let args = crate::cli::Args::parse_from([
            std::ffi::OsStr::new("tell-tail"),
            path.as_os_str(),
            std::ffi::OsStr::new("--from"),
            std::ffi::OsStr::new("start"),
            std::ffi::OsStr::new("--window"),
            std::ffi::OsStr::new("1"),
        ]);
        let mut running = crate::runtime::Running::start(&args, Arc::default()).unwrap();
        let mut subscription = running.snapshot_reader().subscribe().unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut saw_active_window = false;
        loop {
            let publication = receive(&mut subscription).unwrap();
            assert!(Instant::now() < deadline, "idle expiry not published");
            let snapshot = publication.snapshot;
            saw_active_window |= snapshot.window.counts.records == 2;
            if saw_active_window && snapshot.window.counts.records == 0 {
                assert_eq!(snapshot.totals.records, 2);
                break;
            }
        }
        running.stop();
        running.join().unwrap();
        let final_snapshot = receive(&mut subscription).unwrap().snapshot;
        assert_eq!(final_snapshot.state, State::Stopped);
        assert_eq!(final_snapshot.totals.records, 2);
        assert!(receive(&mut subscription).is_none());
    }
}
