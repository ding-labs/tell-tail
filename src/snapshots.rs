//! Read-only access to the latest published snapshot and its lifecycle.
use crate::stats::Snapshot;
use std::sync::{Arc, Mutex};

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

    /// Whether the ingestion worker has exited, including unwinding on panic.
    /// A closed reader still retains its last publication.
    pub fn is_closed(&self) -> bool {
        self.slot
            .current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .closed
    }
}

struct Current {
    latest: PublishedSnapshot,
    closed: bool,
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
    }
}

/// Close even when ingestion unwinds before producing a final snapshot.
pub(crate) struct CloseSnapshots(pub(crate) Arc<SnapshotSlot>);

impl Drop for CloseSnapshots {
    fn drop(&mut self) {
        self.0
            .current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .closed = true;
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
