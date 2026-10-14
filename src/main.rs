use clap::{CommandFactory, Parser};
use std::io::{self, IsTerminal, Write};
use tell_tail::{cli::Args, engine, output};

fn run(args: &Args) -> io::Result<()> {
    let snapshot = if let Some(path) = args.path() {
        if !args.once {
            return Err(io::Error::other(
                "file following is not yet implemented; use --once",
            ));
        }
        engine::scan(std::fs::File::open(path)?, args)?
    } else {
        if io::stdin().is_terminal() {
            Args::command().print_help()?;
            return Ok(());
        }
        engine::scan(io::stdin().lock(), args)?
    };
    io::stdout()
        .lock()
        .write_all(output::render(&snapshot, args.output)?.as_bytes())
}
fn main() {
    let args = Args::parse();
    if let Err(message) = args.validate() {
        eprintln!("tell-tail: {message}");
        std::process::exit(2);
    }
    if let Err(error) = run(&args) {
        if error.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("tell-tail: {error}");
            std::process::exit(1);
        }
    }
}
