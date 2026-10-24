//! Time out a pending socket write only when the socket makes no progress.
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep, sleep},
};

#[cfg(test)]
#[derive(Default)]
pub(super) struct Probe {
    pub written: std::sync::atomic::AtomicUsize,
    pub partial: std::sync::atomic::AtomicUsize,
    pub blocked: std::sync::atomic::AtomicBool,
    pub expired: std::sync::atomic::AtomicUsize,
}

pub(super) struct ProgressIo<T> {
    inner: T,
    deadline: Pin<Box<Sleep>>,
    waiting: bool,
    timeout: Duration,
    #[cfg(test)]
    probe: Option<std::sync::Arc<Probe>>,
}

impl<T> ProgressIo<T> {
    pub(super) fn new(inner: T, timeout: Duration) -> Self {
        Self {
            inner,
            deadline: Box::pin(sleep(timeout)),
            waiting: false,
            timeout,
            #[cfg(test)]
            probe: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_probe(mut self, probe: std::sync::Arc<Probe>) -> Self {
        self.probe = Some(probe);
        self
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for ProgressIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for ProgressIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write(cx, bytes) {
            Poll::Ready(result) => {
                #[cfg(test)]
                if let Some(probe) = &this.probe {
                    probe
                        .blocked
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    if let Ok(n) = &result {
                        probe
                            .written
                            .fetch_add(*n, std::sync::atomic::Ordering::Relaxed);
                        if *n > 0 && *n < bytes.len() {
                            probe
                                .partial
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
                this.waiting = false;
                Poll::Ready(result)
            }
            Poll::Pending => {
                #[cfg(test)]
                if let Some(probe) = &this.probe {
                    probe
                        .blocked
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if !this.waiting {
                    this.deadline.as_mut().reset(Instant::now() + this.timeout);
                    this.waiting = true;
                }
                match this.deadline.as_mut().poll(cx) {
                    Poll::Ready(()) => {
                        #[cfg(test)]
                        if let Some(probe) = &this.probe {
                            probe
                                .expired
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "web socket made no write progress",
                        )))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    #[test]
    fn stalled_writes_expire_but_idle_sockets_and_progress_do_not() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(2), async {
                    let (writer, mut reader) = duplex(4);
                    let mut writer = ProgressIo::new(writer, Duration::from_millis(100));
                    // Idle time before the first write cannot consume its deadline.
                    sleep(Duration::from_millis(120)).await;
                    writer.write_all(b"1234").await.unwrap();
                    let error = writer.write_all(b"x").await.unwrap_err();
                    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
                    reader.read_exact(&mut [0; 4]).await.unwrap();
                    writer.write_all(b"abcd").await.unwrap();

                    let drain = async {
                        for _ in 0..8 {
                            sleep(Duration::from_millis(25)).await;
                            reader.read_exact(&mut [0; 4]).await.unwrap();
                        }
                    };
                    // Total write time exceeds the timeout, with regular progress.
                    let started = Instant::now();
                    let (result, ()) = tokio::join!(writer.write_all(&[0; 32]), drain);
                    result.unwrap();
                    assert!(started.elapsed() > Duration::from_millis(100));
                    drop(reader);
                    assert!(writer.write_all(b"closed").await.is_err());
                })
                .await
                .expect("socket progress test must finish within two seconds of virtual time");
            });
    }
}
