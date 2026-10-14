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
    runtime::{Running, Signals},
};

fn run(args: &Args) -> io::Result<i32> {
    if args.path().is_none() && io::stdin().is_terminal() {
        Args::command().print_help()?;
        return Ok(0);
    }
    if matches!(args.output, Output::Tui | Output::Prometheus) {
        return Err(io::Error::other("this output is not implemented yet"));
    }
    let signals = Signals::install()?;
    let mut running = Running::start(args, signals.requested.clone())?;
    let mut writer = StdoutWriter::new(signals.requested.clone())?;
    let mut published = Instant::now();
    loop {
        let snapshot = running.snapshot();
        if snapshot.state.is_final() {
            writer.write_all(output::render(&snapshot, args.output)?.as_bytes())?;
            running.join()?;
            break;
        }
        if running.is_finished() {
            running.join()?;
            continue;
        }
        if !args.once && published.elapsed() >= Duration::from_millis(u64::from(args.interval_ms)) {
            writer.write_all(output::render(&snapshot, args.output)?.as_bytes())?;
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
