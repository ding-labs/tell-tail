use clap::Parser;
use tell_tail::cli::Args;

fn main() {
    let args = Args::parse();
    if let Err(message) = args.validate() {
        eprintln!("tell-tail: {message}");
        std::process::exit(2);
    }
    eprintln!("tell-tail: the engine is not yet implemented");
    std::process::exit(1);
}
