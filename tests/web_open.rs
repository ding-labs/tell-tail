#![cfg(feature = "web")]
use std::{
    fs,
    io::Read,
    net::TcpStream,
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Opened {
    child: Child,
    desktop_pid: Option<String>,
}
impl Drop for Opened {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(pid) = &self.desktop_pid {
            let _ = Command::new("/bin/kill").args(["-KILL", pid]).status();
        }
    }
}

#[test]
fn browser_handoff_is_literal_nonfatal_and_never_delays_server_shutdown() {
    for mode in ["success", "failure", "running", "missing"] {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("arguments");
        let pid = dir.path().join("pid");
        if mode != "missing" {
            let program = if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            };
            let opener = dir.path().join(program);
            let behavior = match mode {
                "success" => "exit 0",
                "failure" => "exit 7",
                _ => "printf '%s' \"$$\" > \"$OPEN_PID\"\nexec /bin/sleep 30",
            };
            fs::write(
                &opener,
                format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$OPEN_LOG\"\n{behavior}\n"),
            )
            .unwrap();
            fs::set_permissions(&opener, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["--output", "web", "--open"])
            .env("PATH", dir.path())
            .env("OPEN_LOG", &log)
            .env("OPEN_PID", &pid)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut opened = Opened {
            child,
            desktop_pid: None,
        };
        let stderr = opened.child.stderr.as_mut().unwrap();
        let flags = rustix::fs::fcntl_getfl(&*stderr).unwrap();
        rustix::fs::fcntl_setfl(&*stderr, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
        let began = Instant::now();
        let mut diagnostic = String::new();
        let url = loop {
            let mut bytes = [0; 4096];
            match stderr.read(&mut bytes) {
                Ok(n) => diagnostic.push_str(std::str::from_utf8(&bytes[..n]).unwrap()),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(error) => panic!("{error}"),
            }
            let url = diagnostic
                .split_whitespace()
                .find(|s| s.starts_with("http://"));
            let launched = if mode == "missing" {
                true
            } else {
                fs::read_to_string(&log).is_ok_and(|s| !s.is_empty())
            };
            let reported = !matches!(mode, "failure" | "missing")
                || diagnostic.contains("could not open a browser");
            let desktop_ready = mode != "running" || pid.exists();
            if let Some(url) = url.filter(|_| launched && reported && desktop_ready) {
                break url.to_owned();
            }
            assert!(
                began.elapsed() < Duration::from_secs(3),
                "{mode}: {diagnostic}"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        if mode == "running" {
            opened.desktop_pid = Some(fs::read_to_string(&pid).unwrap());
        }
        if mode != "missing" {
            assert_eq!(fs::read_to_string(&log).unwrap(), format!("{url}\n"));
        }
        let address = url.trim_start_matches("http://").trim_end_matches('/');
        let socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut socket = socket;
        use std::io::Write;
        write!(socket, "GET / HTTP/1.1\r\nHost: {address}\r\n\r\n").unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{mode}: {response}");
        assert!(response.contains("Live metrics"));
        let began = Instant::now();
        assert!(
            Command::new("/bin/kill")
                .args(["-TERM", &opened.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        loop {
            if let Some(status) = opened.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(143));
                break;
            }
            assert!(
                began.elapsed() < Duration::from_secs(1),
                "{mode} delayed shutdown"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut stdout = String::new();
        opened
            .child
            .stdout
            .as_mut()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        assert!(stdout.is_empty(), "desktop opener must not inherit stdout");
    }
}
