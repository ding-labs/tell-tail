//! Owned live log capture. Only private child pipes are made nonblocking.
use super::{Event, read_event};
use crate::{platform, stats::State};
use std::{
    io::{self, Read},
    os::unix::process::CommandExt,
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const STDERR_BYTES: usize = 4096;

pub struct AppleStream {
    child: Child,
    output: ChildStdout,
    errors: ChildStderr,
    diagnostic: Vec<u8>,
}

impl AppleStream {
    pub fn open() -> io::Result<Self> {
        if !cfg!(target_os = "macos") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "live Apple unified logging requires macOS",
            ));
        }
        let mut command = Command::new("/usr/bin/log");
        command.args([
            "stream", "--style", "ndjson", "--level", "info", "--type", "log",
        ]);
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> io::Result<Self> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // The parent owns terminal signals and final snapshot delivery.
            .process_group(0)
            .spawn()?;
        let stream = Self {
            output: child.stdout.take().expect("piped log stdout"),
            errors: child.stderr.take().expect("piped log stderr"),
            child,
            diagnostic: Vec::with_capacity(STDERR_BYTES),
        };
        // The guard above also reaps the child if pipe setup fails.
        for pipe in [&stream.output as &dyn std::os::fd::AsFd, &stream.errors] {
            let flags = rustix::fs::fcntl_getfl(pipe)?;
            rustix::fs::fcntl_setfl(pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
        }
        Ok(stream)
    }

    fn drain_errors(&mut self) -> io::Result<()> {
        let mut bytes = [0; 4096];
        // Bound work per input tick as well as retained diagnostic memory.
        for _ in 0..16 {
            match self.errors.read(&mut bytes) {
                Ok(0) => break,
                Ok(n) => {
                    let keep = n.min(STDERR_BYTES - self.diagnostic.len());
                    self.diagnostic.extend_from_slice(&bytes[..keep]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn ended(&mut self) -> io::Error {
        let deadline = Instant::now() + Duration::from_millis(25);
        let status = loop {
            let _ = self.drain_errors();
            match self.child.try_wait() {
                Ok(Some(status)) => break status.to_string(),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                _ => break "stdout closed".into(),
            }
        };
        let detail: String = String::from_utf8_lossy(&self.diagnostic)
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        io::Error::other(format!(
            "Apple unified log stream ended unexpectedly ({status}){}{}",
            if detail.trim().is_empty() { "" } else { ": " },
            detail.trim()
        ))
    }

    pub fn next(&mut self, buffer: &mut [u8]) -> io::Result<Event> {
        self.drain_errors()?;
        if !platform::readable(&self.output)? {
            if self.child.try_wait()?.is_some() {
                return Err(self.ended());
            }
            return Ok(Event::Idle(State::Reading));
        }
        match read_event(&mut self.output, buffer)? {
            Event::End => Err(self.ended()),
            event => Ok(event),
        }
    }
}

impl Drop for AppleStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    fn gone(pid: u32) {
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .output()
                .unwrap()
                .status
                .success(),
            "log child was not reaped"
        );
    }

    #[test]
    fn drains_records_before_reporting_capture_exit_and_retains_bounded_errors() {
        let script = "i=0; while [ \"$i\" -lt 2048 ]; do printf 'native capture failure\n' >&2; i=$((i+1)); done; printf '{\"messageType\":\"Info\"}\n'; exit 7";
        let mut stream = AppleStream::spawn(command(script)).unwrap();
        let pid = stream.child.id();
        let mut output = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        let error = loop {
            assert!(Instant::now() < deadline);
            let mut buffer = [0; 7];
            match stream.next(&mut buffer) {
                Ok(Event::Data { len, .. }) => output.extend_from_slice(&buffer[..len]),
                Ok(Event::Idle(_)) => (),
                Err(error) => break error,
                _ => panic!("unexpected source event"),
            }
        };
        assert_eq!(output, b"{\"messageType\":\"Info\"}\n");
        assert!(error.to_string().contains("exit status: 7"));
        assert!(error.to_string().contains("native capture failure"));
        assert_eq!(stream.diagnostic.len(), STDERR_BYTES);
        drop(stream);
        gone(pid);
    }

    #[test]
    fn idle_capture_can_be_stopped_and_reaped_promptly() {
        let mut stream = AppleStream::spawn(command("exec /bin/sleep 30")).unwrap();
        let pid = stream.child.id();
        let began = Instant::now();
        assert!(matches!(
            stream.next(&mut [0; 64]).unwrap(),
            Event::Idle(State::Reading)
        ));
        drop(stream);
        assert!(began.elapsed() < Duration::from_millis(500));
        gone(pid);
    }

    #[test]
    fn closed_stdout_is_a_failure_even_when_the_process_is_still_alive() {
        let mut stream = AppleStream::spawn(command("exec 1>&-; exec /bin/sleep 30")).unwrap();
        let pid = stream.child.id();
        let began = Instant::now();
        assert!(
            stream
                .next(&mut [0; 64])
                .err()
                .expect("closed capture must fail")
                .to_string()
                .contains("stdout closed")
        );
        drop(stream);
        assert!(began.elapsed() < Duration::from_millis(500));
        gone(pid);
    }

    #[test]
    fn successful_capture_exit_is_still_unexpected_for_a_live_source() {
        let mut stream = AppleStream::spawn(command("exit 0")).unwrap();
        let error = stream
            .next(&mut [0; 64])
            .err()
            .expect("live capture cannot complete silently");
        assert!(error.to_string().contains("exit status: 0"));
    }
}
