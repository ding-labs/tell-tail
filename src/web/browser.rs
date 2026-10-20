//! Best-effort desktop handoff, with no shell or wait on the server's lifetime.
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
};

pub(super) struct Browser(Option<Child>);

impl Browser {
    pub fn open(url: &str) -> io::Result<Self> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let child = Command::new(program)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Terminal signals belong to the monitor, not the user's browser.
            .process_group(0)
            .spawn()?;
        Ok(Self(Some(child)))
    }

    /// Reap promptly if the opener exits. Some Linux openers run for the entire
    /// browser session, so neither a deadline nor shutdown should kill them.
    /// A still-running desktop process is left running when the monitor exits.
    pub fn failed(&mut self) -> bool {
        let Some(child) = self.0.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(None) => false,
            Ok(Some(status)) => {
                self.0 = None;
                !status.success()
            }
            Err(_) => {
                self.0 = None;
                true
            }
        }
    }
}

pub(super) fn report_failure() {
    crate::writer::diagnostic(
        "tell-tail: could not open a browser; use the printed dashboard URL\n",
    );
}
