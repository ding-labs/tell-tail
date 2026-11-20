#![cfg(feature = "web")]
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Monitor {
    child: Child,
    address: SocketAddr,
}
impl Monitor {
    fn start(root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        fs::create_dir_all(root.join("desktop")).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["desktop", "--source", "codex-desktop", "--log-dir"])
            .arg(root.join("desktop"))
            .arg("--sources-file")
            .arg(root.join("sources.json"))
            .args(["--output", "web", "--web-port", &address.port().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let m = Self { child, address };
        let start = Instant::now();
        while m.raw("GET", "/api/log-sources", "", "").is_err() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(20));
        }
        m
    }
    fn raw(&self, method: &str, path: &str, body: &str, headers: &str) -> std::io::Result<String> {
        let mut socket = TcpStream::connect(self.address)?;
        socket.set_read_timeout(Some(Duration::from_secs(4)))?;
        write!(
            socket,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            self.address,
            body.len()
        )?;
        let mut response = String::new();
        socket.read_to_string(&mut response)?;
        Ok(response)
    }
    fn call(&self, method: &str, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
        let headers = format!(
            "Origin: http://{}\r\nContent-Type: application/json\r\nX-Tell-Tail-Action: manage-sources\r\n",
            self.address
        );
        let body = if method == "GET" || method == "DELETE" {
            String::new()
        } else {
            body.to_string()
        };
        let r = self.raw(method, path, &body, &headers).unwrap();
        let status = r.split_whitespace().nth(1).unwrap().parse().unwrap();
        let v = serde_json::from_str(r.split_once("\r\n\r\n").unwrap().1).unwrap_or_default();
        (status, v)
    }
    fn until(&self, id: u64, records: &str) -> serde_json::Value {
        let start = Instant::now();
        loop {
            let (code, v) = self.call(
                "GET",
                &format!("/api/log-sources/{id}/snapshot"),
                serde_json::Value::Null,
            );
            assert_eq!(code, 200, "{v}");
            if v["metrics"]["totals"]["records"] == records {
                return v;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "{v}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn sources_preserve_original_parser_following_previews_and_restart_settings() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("application.log");
    fs::write(&file, "{\"level\":50,\"msg\":\"cache unavailable\"}\n").unwrap();
    let m = Monitor::start(root.path());
    let config =
        serde_json::json!({"name":"Application","path":file,"profile":"pino","from":"start"});
    let (status, v) = m.call("POST", "/api/log-sources", config.clone());
    assert_eq!(status, 201, "{v}");
    let id = v["id"].as_u64().unwrap();
    let snapshot = m.until(id, "1");
    assert_eq!(snapshot["metrics"]["totals"]["levels"]["error"], "1");
    assert!(
        snapshot["recent_errors"]
            .to_string()
            .contains("cache unavailable")
    );
    assert!(
        !m.call("GET", "/api/activity/snapshot", serde_json::Value::Null)
            .1
            .to_string()
            .contains("cache unavailable")
    );
    let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(f, "{{\"level\":40}}").unwrap();
    m.until(id, "2");
    fs::rename(&file, root.path().join("application.log.1")).unwrap();
    fs::write(&file, "{\"level\":30}\n").unwrap();
    m.until(id, "3");
    assert_eq!(
        m.call(
            "POST",
            &format!("/api/log-sources/{id}"),
            serde_json::json!({"action":"pause"})
        )
        .0,
        200
    );
    let paused = m
        .call(
            "GET",
            &format!("/api/log-sources/{id}/snapshot"),
            serde_json::Value::Null,
        )
        .1;
    assert_eq!(paused["metrics"]["state"], "stopped");
    drop(m);
    let m = Monitor::start(root.path());
    let sources = m.call("GET", "/api/log-sources", serde_json::Value::Null).1;
    assert_eq!(sources["sources"][0]["state"], "paused");
    assert_eq!(sources["sources"][0]["config"]["profile"], "pino");
    assert_eq!(
        m.call(
            "POST",
            &format!("/api/log-sources/{id}"),
            serde_json::json!({"action":"resume"})
        )
        .0,
        200
    );
    m.until(id, "1");
    let mut updated = config;
    updated["name"] = "Renamed".into();
    updated["profile"] = serde_json::Value::Null;
    assert_eq!(
        m.call("PUT", &format!("/api/log-sources/{id}"), updated).0,
        200
    );
    assert_eq!(
        m.until(id, "1")["metrics"]["totals"]["levels"]["unknown"],
        "1"
    );
    assert_eq!(
        m.call(
            "DELETE",
            &format!("/api/log-sources/{id}"),
            serde_json::Value::Null
        )
        .0,
        200
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "{\"level\":30}\n");
    assert_eq!(
        m.call(
            "GET",
            &format!("/api/log-sources/{id}/snapshot"),
            serde_json::Value::Null
        )
        .0,
        404
    );
}
#[test]
fn mutations_require_local_browser_intent_and_reject_non_regular_inputs() {
    let root = tempfile::tempdir().unwrap();
    let m = Monitor::start(root.path());
    let body = serde_json::json!({"name":"App","path":root.path().join("missing.log")}).to_string();
    for headers in ["".to_owned(),"Origin: https://evil.invalid\r\nX-Tell-Tail-Action: manage-sources\r\nContent-Type: application/json\r\n".into(),format!("Origin: http://{}\r\nContent-Type: application/json\r\n",m.address)]{assert!(m.raw("POST","/api/log-sources",&body,&headers).unwrap().starts_with("HTTP/1.1 403"));}
    assert!(
        m.raw(
            "GET",
            "/api/log-sources",
            "",
            "Origin: https://evil.invalid\r\n"
        )
        .unwrap()
        .starts_with("HTTP/1.1 403")
    );
    assert_eq!(
        m.call(
            "POST",
            "/api/log-sources",
            serde_json::json!({"name":"Device","path":"/dev/null"})
        )
        .0,
        400
    );
    let fifo = root.path().join("pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        m.call(
            "POST",
            "/api/log-sources",
            serde_json::json!({"name":"Pipe","path":fifo})
        )
        .0,
        400
    );
    let (code, v) = m.call(
        "POST",
        "/api/log-sources",
        serde_json::from_str(&body).unwrap(),
    );
    assert_eq!(code, 201);
    let id = v["id"].as_u64().unwrap();
    assert_eq!(
        m.call(
            "GET",
            &format!("/api/log-sources/{id}/snapshot"),
            serde_json::Value::Null
        )
        .1["metrics"]["totals"]["records"],
        "0"
    );
    fs::write(root.path().join("missing.log"), "ERROR appeared\n").unwrap();
    m.until(id, "1");
    let headers = format!(
        "Origin: http://{}\r\nX-Tell-Tail-Action: manage-sources\r\nContent-Type: application/json\r\n",
        m.address
    );
    assert!(
        m.raw("POST", "/api/log-sources", &"x".repeat(8193), &headers)
            .unwrap()
            .starts_with("HTTP/1.1 413")
    );
    assert!(
        m.raw("POST", "/api/snapshot", "{}", &headers)
            .unwrap()
            .starts_with("HTTP/1.1 405")
    );
}

#[test]
fn browser_delete_accepts_an_empty_body_without_content_length() {
    let root = tempfile::tempdir().unwrap();
    let m = Monitor::start(root.path());
    let (_, value) = m.call(
        "POST",
        "/api/log-sources",
        serde_json::json!({"name":"App","path":root.path().join("app.log")}),
    );
    let id = value["id"].as_u64().unwrap();
    let mut socket = TcpStream::connect(m.address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    write!(socket,"DELETE /api/log-sources/{id} HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nX-Tell-Tail-Action: manage-sources\r\n\r\n",m.address,m.address).unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
}
