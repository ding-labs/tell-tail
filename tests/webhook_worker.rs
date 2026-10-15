use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tell_tail::webhook::{HELPER_ARG, Request};
use tell_tail::webhook_delivery::DeliveryQueue;

fn spawn(url: &str) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg(HELPER_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .spawn()
        .unwrap();
    let request = serde_json::to_vec(&Request {
        url: url.into(),
        body: r#"{"message":"test"}"#.into(),
    })
    .unwrap();
    let input = child.stdin.as_mut().unwrap();
    input
        .write_all(&(request.len() as u32).to_le_bytes())
        .unwrap();
    input.write_all(&request).unwrap();
    child
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let start = Instant::now();
    loop {
        if let Ok((stream, _)) = listener.accept() {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            return stream;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "helper did not connect"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn finish(child: &mut Child, deadline: Duration) -> i32 {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().unwrap();
        }
        if started.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("webhook helper exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn posts_json_and_requires_success_without_following_redirects() {
    for (status, code) in [
        ("204 No Content", 0),
        ("500 Internal Server Error", 10),
        ("302 Found", 10),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/secret-token", listener.local_addr().unwrap());
        let mut child = spawn(&url);
        let mut stream = accept(&listener);
        let mut request = Vec::new();
        while !request.ends_with(br#"{"message":"test"}"#) {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 4096);
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("POST /secret-token HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/json\r\n")
        );
        write!(
            stream,
            "HTTP/1.1 {status}\r\nLocation: {url}\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
        assert_eq!(finish(&mut child, Duration::from_secs(2)), code);
        assert!(
            listener.accept().is_err(),
            "redirect must not send another request"
        );
    }
}

#[test]
fn parent_pipe_closure_cancels_a_stalled_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut child = spawn(&format!("http://{}/", listener.local_addr().unwrap()));
    let _stream = accept(&listener);
    drop(child.stdin.take());
    assert_eq!(finish(&mut child, Duration::from_secs(1)), 11);
}

#[test]
fn stalled_http_response_has_a_request_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut child = spawn(&format!("http://{}/", listener.local_addr().unwrap()));
    let _stream = accept(&listener);
    assert_eq!(finish(&mut child, Duration::from_secs(7)), 11);
}

#[test]
fn stalled_delivery_has_a_bounded_queue_and_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let queue = DeliveryQueue::start(
        format!("http://{}/", listener.local_addr().unwrap()),
        env!("CARGO_BIN_EXE_tell-tail").into(),
    )
    .unwrap();
    assert!(queue.try_send("{}".into()));
    let _stream = accept(&listener);
    // Once the server has accepted the in-flight request, only one pending
    // payload can be retained, regardless of how many breaches are attempted.
    assert!(queue.try_send("{}".into()));
    let started = Instant::now();
    for _ in 0..1000 {
        assert!(!queue.try_send("{}".into()));
    }
    assert!(started.elapsed() < Duration::from_secs(1));
    let started = Instant::now();
    drop(queue);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        listener.accept().is_err(),
        "pending delivery should be cancelled"
    );
}
