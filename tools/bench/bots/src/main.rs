//! `gamma-bots`: connects load bots to a Minecraft 1.7.10 Forge server in offline mode, keeps
//! them in game with a movement behaviour, and reports what each one saw. Two subcommands drive
//! the bench: `run` plays a scenario end to end against a server it starts, `compare` turns two
//! results into a before and after table.

mod args;
mod behaviour;
mod bot;
mod compare;
mod fleet;
mod orchestrate;
mod report;
mod scenario;
mod server;
mod sha256;
mod signal;
mod toml;
mod util;

use std::process::ExitCode;

use args::{Args, Command};
use fleet::Fleet;

fn main() -> ExitCode {
    let mut raw = std::env::args().skip(1).peekable();
    match raw.peek().map(String::as_str) {
        Some("run") => {
            raw.next();
            return orchestrate::main(raw);
        }
        Some("compare") => {
            raw.next();
            return compare::main(raw);
        }
        _ => {}
    }
    let args = match args::parse(raw) {
        Ok(Command::Run(args)) => args,
        Ok(Command::Help) => {
            print!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if !signal::install() {
        eprintln!("warning: cannot catch Ctrl-C; interrupting will skip the report");
    }
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Returns whether every bot joined and stayed until the end.
fn run(args: &Args) -> Result<bool, String> {
    let mut fleet = Fleet::connect(args)?;
    println!(
        "starting {}, {} per second, for {} s",
        fleet.title,
        if args.rate > 0.0 {
            args.rate.to_string()
        } else {
            "all".into()
        },
        args.duration.as_secs_f64()
    );
    fleet.arrive(Some(args.duration));
    if let Some(deadline) = fleet.deadline() {
        fleet.wait(deadline);
    }
    let title = fleet.title.clone();
    let (rows, elapsed) = fleet.finish();
    print!("{}", report::summary(&rows, elapsed, &title));
    if let Some(path) = &args.report {
        report::write_csv(path, &rows)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!("report: {}", path.display());
    }
    Ok(rows.iter().all(|(_, s)| s.connected_at_end))
}
