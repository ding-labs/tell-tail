use clap::{CommandFactory, Parser};
use std::{
    io::{self, IsTerminal, Write},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};
use tell_tail::{
    cli::{Args, Output},
    output,
    platform::StdoutWriter,
    prometheus::MetricsFile,
    runtime::{Running, Signals},
    tui,
};

fn run(args: &Args) -> io::Result<i32> {
    if args.path().is_none() && io::stdin().is_terminal() {
        Args::command().print_help()?;
        return Ok(0);
    }
    let use_tui = args.output == Output::Tui
        || (args.output == Output::Auto && !args.once && tui::available());
    if use_tui && !tui::available() {
        return Err(io::Error::other(
            "dashboard needs terminal stdout and a controlling terminal; use --output text or json",
        ));
    }
    let metrics = args
        .metrics_file
        .as_ref()
        .map(|p| MetricsFile::new(p.clone(), args.path().map(|p| p.to_owned())))
        .transpose()?;
    let signals = Signals::install()?;
    let mut running = Running::start(args, signals.requested.clone())?;
    let mut writer = StdoutWriter::new(signals.requested.clone())?;
    if use_tui {
        let snapshot = tui::run(&mut running, writer, signals.requested.clone())?;
        running.join()?;
        let mut writer = StdoutWriter::new(signals.requested.clone())?;
        writer.write_all(output::render(&snapshot, Output::Text)?.as_bytes())?;
        let signal = signals.requested.load(Ordering::Relaxed);
        return Ok(if signal > 1 { 128 + signal as i32 } else { 0 });
    }
    let mut published = Instant::now();
    loop {
        let snapshot = running.snapshot();
        if snapshot.state.is_final() {
            if let Some(file) = &metrics {
                file.publish(&snapshot)?;
            } else {
                writer.write_all(output::render(&snapshot, args.output)?.as_bytes())?;
            }
            running.join()?;
            break;
        }
        if running.is_finished() {
            running.join()?;
            continue;
        }
        if !args.once
            && (args.output != Output::Prometheus || metrics.is_some())
            && published.elapsed() >= Duration::from_millis(u64::from(args.interval_ms))
        {
            if let Some(file) = &metrics {
                file.publish(&snapshot)?;
            } else {
                writer.write_all(output::render(&snapshot, args.output)?.as_bytes())?;
            }
            published = Instant::now();
        }
        thread::sleep(Duration::from_millis(10));
    }
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}
fn main() {
    let args = Args::parse();
    if let Err(message) = args.validate() {
        eprintln!("tell-tail: {message}");
        std::process::exit(2);
    }
    match run(&args) {
        Ok(code) => std::process::exit(code),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => (),
        Err(error) => {
            eprintln!("tell-tail: {error}");
            std::process::exit(1);
        }
    }
}
