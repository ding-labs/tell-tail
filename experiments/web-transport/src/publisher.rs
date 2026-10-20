use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tell_tail::{runtime::Running, snapshots::PublishedSnapshot, stats::Snapshot};
use tell_tail_web_transport::Frame;
use tokio::{sync::watch, task::JoinHandle, time::MissedTickBehavior};

pub struct Publisher {
    pub frames: watch::Receiver<Arc<Frame>>,
    pub task: JoinHandle<io::Result<()>>,
}

fn frame(
    sequence: u64,
    snapshot: &Snapshot,
    clock: Instant,
    padding: u32,
) -> io::Result<Arc<Frame>> {
    Frame::new(
        sequence,
        clock.elapsed().as_micros() as u64,
        snapshot,
        padding,
    )
    .map(Arc::new)
}

fn live_frame(
    publication: PublishedSnapshot,
    clock: Instant,
    padding: u32,
) -> io::Result<Arc<Frame>> {
    frame(
        publication.generation,
        &publication.snapshot,
        clock,
        padding,
    )
}

pub async fn start(
    running: Option<Running>,
    captured: Snapshot,
    hz: u32,
    padding: u32,
    clock: Instant,
) -> io::Result<Publisher> {
    if let Some(mut running) = running {
        let mut subscription = running.snapshot_reader().subscribe()?;
        // A late subscriber gets even a completed input's final state immediately.
        let initial = subscription.changed().await.expect("initial publication");
        let (tx, frames) = watch::channel(live_frame(initial, clock, padding)?);
        let task = tokio::spawn(async move {
            while let Some(publication) = subscription.changed().await {
                // The owned publication carries no ingestion lock into encoding/I/O.
                tx.send_replace(live_frame(publication, clock, padding)?);
            }
            running.join()?;
            drop(subscription);
            drop(running);
            // Retain final state and open streams until the server is stopped.
            // There is no timer or repeated final serialization after EOF.
            let _keep_frames_open = tx;
            std::future::pending::<io::Result<()>>().await
        });
        return Ok(Publisher { frames, task });
    }

    // Replay alone uses a synthetic timer, preserving the protocol comparison.
    let (tx, frames) = watch::channel(frame(0, &captured, clock, padding)?);
    let task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(hz)));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence = 0;
        loop {
            tick.tick().await;
            sequence += 1;
            tx.send_replace(frame(sequence, &captured, clock, padding)?);
        }
    });
    Ok(Publisher { frames, task })
}
