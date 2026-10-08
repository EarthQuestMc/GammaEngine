//! `gamma-bots run <scenario.toml>`: plays a scenario end to end and collects its results.
//!
//! Once per run: a fresh server folder from `tools/test-server/setup`, the jar and the Java
//! version fingerprinted. Then for each repetition: the folder emptied back to the jar and the
//! libraries, the test settings and the scenario's written again, the starting world copied, the
//! server started on its console, `Done` awaited, the bots brought in, the warm-up, `autothread
//! record start`, the measure, `autothread record stop`, the bots stopped, `stop`, and the files
//! collected into `bench/results/<date>_<scenario>_<commit>/rep<n>/`.

use std::fmt::Write as _;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::compare::Sample;
use crate::fleet::Fleet;
use crate::report;
use crate::scenario::Scenario;
use crate::server::{self, ServerProcess};
use crate::sha256;
use crate::signal;
use crate::util::{self, json_string};

const USAGE: &str = "\
Usage: gamma-bots run <scenario.toml> [options]

Plays a scenario of bench/scenarios end to end: fresh server, bots, warm-up, recording, stop,
and results in bench/results/<UTC date>_<scenario>_<commit>/.

Options:
  --set <table.key=value>  Override a scenario value, e.g. --set bots.count=50 (repeatable)
  --java <path>            Java executable, instead of [server] java
  --accept-eula            Write eula=true in the server folder: only if you accept the
                           Minecraft EULA (https://aka.ms/MinecraftEULA)
  --results <dir>          Parent folder of the results (default: bench/results in the repository)
  --dry-run                Print what would run, change nothing
  -h, --help               This help

Exit code: 0 when every repetition was recorded, 1 when one failed, 2 for an error before
the first start.
";

/// Written in every server folder the orchestrator creates: it only ever empties those.
const MARKER: &str = ".gamma-orchestrator";
/// What survives from one repetition to the next: the setup's output, not the server's.
const KEPT: &[&str] = &[MARKER, "libraries", "server.jar", "eula.txt"];
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Attribution resolves every class to its mod when the recording stops.
const RECORD_STOP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Default)]
struct Options {
    scenario: PathBuf,
    overrides: Vec<String>,
    java: Option<String>,
    accept_eula: bool,
    results: Option<PathBuf>,
    dry_run: bool,
}

fn parse_options(raw: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut scenario = None;
    let mut raw = raw.peekable();
    while let Some(arg) = raw.next() {
        let mut value = |name: &str| raw.next().ok_or_else(|| format!("{name}: missing value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--set" => options.overrides.push(value("--set")?),
            "--java" => options.java = Some(value("--java")?),
            "--results" => options.results = Some(PathBuf::from(value("--results")?)),
            "--accept-eula" => options.accept_eula = true,
            "--dry-run" => options.dry_run = true,
            _ if arg.starts_with('-') => return Err(format!("unknown option '{arg}'")),
            _ if scenario.is_none() => scenario = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument '{arg}'")),
        }
    }
    options.scenario = scenario.ok_or("missing the scenario file")?;
    Ok(Some(options))
}

pub fn main(raw: impl Iterator<Item = String>) -> ExitCode {
    let options = match parse_options(raw) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !signal::install() {
        eprintln!("warning: cannot catch Ctrl-C; interrupting may leave the server folder busy");
    }
    match run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// The repository root: the first folder up from `start` that holds `tools/test-server`.
fn find_repo(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| {
            dir.join("tools")
                .join("test-server")
                .join("config")
                .is_dir()
        })
        .map(Path::to_path_buf)
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        // Rebuilt from its components: one kind of separator in what is printed and recorded.
        .map(|dir| dir.join(path).components().collect())
        .map_err(|e| format!("cannot read the current folder: {e}"))
}

fn expand_home(text: &str) -> String {
    match text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        Some(rest) => match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            Some(home) => Path::new(&home).join(rest).display().to_string(),
            None => text.to_owned(),
        },
        None => text.to_owned(),
    }
}

/// A path of the scenario: relative to the repository root unless absolute.
fn in_repo(repo: &Path, path: &Path) -> PathBuf {
    let expanded = PathBuf::from(expand_home(&path.to_string_lossy()));
    if expanded.is_absolute() {
        expanded
    } else {
        repo.join(expanded).components().collect()
    }
}

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// `server.properties` with `settings` written over: existing keys keep their place, new ones
/// are appended in order.
pub fn apply_properties(text: &str, settings: &[(String, String)]) -> String {
    let mut done = vec![false; settings.len()];
    let mut out = String::new();
    for line in text.lines() {
        let key = line.split_once('=').map(|(k, _)| k.trim());
        let found = key
            .filter(|_| !line.trim_start().starts_with('#'))
            .and_then(|key| settings.iter().position(|(k, _)| k == key));
        match found {
            Some(i) => {
                let _ = writeln!(out, "{}={}", settings[i].0, settings[i].1);
                done[i] = true;
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    for (i, (key, value)) in settings.iter().enumerate() {
        if !done[i] {
            let _ = writeln!(out, "{key}={value}");
        }
    }
    out
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_=+:,./@%\\".contains(c))
    {
        arg.to_owned()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

/// Everything a repetition needs, fixed for the whole run.
struct Plan {
    scenario: Scenario,
    scenario_text: String,
    scenario_path: PathBuf,
    overrides: Vec<String>,
    repo: PathBuf,
    server_dir: PathBuf,
    jar: Option<PathBuf>,
    world: Option<PathBuf>,
    world_sha256: Option<String>,
    java: String,
    java_version: String,
    jvm_args: Vec<String>,
    commit: String,
    dirty: bool,
    accept_eula: bool,
}

#[derive(Debug, Default)]
struct Repetition {
    index: u32,
    started: i64,
    ended: i64,
    startup_s: Option<f64>,
    error: Option<String>,
    server_exit: Option<String>,
    recording: Option<String>,
    in_game_at_record_start: Option<usize>,
    in_game_at_record_stop: Option<usize>,
}

fn run(options: &Options) -> Result<bool, String> {
    let scenario_path = absolute(&options.scenario)?;
    let mut scenario = Scenario::load(&scenario_path, &options.overrides)?;
    if let Some(java) = &options.java {
        scenario.server.java = java.clone();
    }
    let scenario_text = fs::read_to_string(&scenario_path)
        .map_err(|e| format!("cannot read {}: {e}", scenario_path.display()))?;
    let repo = find_repo(&scenario_path)
        .or_else(|| find_repo(&std::env::current_dir().ok()?))
        .ok_or("cannot find the repository root (a folder with tools/test-server/config)")?;

    let server_dir = in_repo(&repo, &scenario.server.dir);
    if server_dir == repo || repo.starts_with(&server_dir) {
        return Err(format!(
            "[server] dir {} would hold the repository",
            server_dir.display()
        ));
    }
    let jar = scenario.server.jar.as_deref().map(|p| in_repo(&repo, p));
    if let Some(jar) = &jar {
        if !jar.is_file() {
            return Err(format!("[server] jar {} does not exist", jar.display()));
        }
    }
    let java = {
        let expanded = expand_home(&scenario.server.java);
        if expanded.contains(['/', '\\']) {
            in_repo(&repo, Path::new(&expanded)).display().to_string()
        } else {
            expanded
        }
    };
    let (java_version, major) = server::java_version(&java)?;
    let jvm_args = server::jvm_arguments(&scenario.server, major, &repo.join("java9args.txt"))?;

    let world = scenario.server.world.as_deref().map(|p| in_repo(&repo, p));
    let mut world_sha256 = None;
    if let Some(world) = &world {
        if !world.join("level.dat").is_file() {
            return Err(format!(
                "[server] world {}: no level.dat in that folder",
                world.display()
            ));
        }
        println!("fingerprinting the world {} ...", world.display());
        let hash = sha256::dir_hex(world)
            .map_err(|e| format!("cannot read the world {}: {e}", world.display()))?;
        if let Some(expected) = &scenario.server.world_sha256 {
            if *expected != hash {
                return Err(format!(
                    "the world {} is not the one of the scenario: sha256 {hash}, expected {expected}",
                    world.display()
                ));
            }
        }
        println!("world sha256 {hash}");
        world_sha256 = Some(hash);
    }

    let commit = git(&repo, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".into());
    let dirty = git(&repo, &["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|s| !s.is_empty());

    let plan = Plan {
        scenario,
        scenario_text,
        scenario_path,
        overrides: options.overrides.clone(),
        repo,
        server_dir,
        jar,
        world,
        world_sha256,
        java,
        java_version,
        jvm_args,
        commit,
        dirty,
        accept_eula: options.accept_eula,
    };
    print_plan(&plan);
    if options.dry_run {
        return Ok(true);
    }

    let port = plan.scenario.server.port;
    TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
        format!("port {port} is not free on 127.0.0.1 ({e}): is another server running?")
    })?;

    // Prepared first: a run that cannot start leaves no empty results folder behind.
    let (jar_name, jar_sha256) = prepare_server(&plan)?;
    let started = util::unix_now();
    let results_parent = match &options.results {
        Some(dir) => absolute(dir)?,
        None => plan.repo.join("bench").join("results"),
    };
    let results = results_parent.join(format!(
        "{}_{}_{}",
        util::stamp(started),
        plan.scenario.name,
        plan.commit
    ));
    fs::create_dir_all(&results)
        .map_err(|e| format!("cannot create {}: {e}", results.display()))?;
    println!("results: {}", results.display());

    write_static_files(&plan, &results)?;

    let mut reps = Vec::new();
    for index in 1..=plan.scenario.repetitions {
        if signal::interrupted() {
            break;
        }
        println!(
            "\n=== {} repetition {index}/{} ===",
            plan.scenario.name, plan.scenario.repetitions
        );
        let rep = play(&plan, index, &results.join(format!("rep{index}")));
        reps.push(rep);
        write_run_json(&plan, &results, started, &jar_name, &jar_sha256, &reps)?;
        signal::reset_stop();
    }

    println!("\n=== {} ===", plan.scenario.name);
    for rep in &reps {
        println!("rep{}: {}", rep.index, rep_line(&results, rep));
    }
    println!("results: {}", results.display());
    let complete =
        reps.len() == plan.scenario.repetitions as usize && reps.iter().all(|r| r.error.is_none());
    Ok(complete)
}

fn print_plan(plan: &Plan) {
    let s = &plan.scenario;
    println!(
        "scenario {} ({}), {} repetition(s)",
        s.name,
        plan.scenario_path.display(),
        s.repetitions
    );
    if !s.description.is_empty() {
        println!("  {}", s.description);
    }
    println!(
        "bots: {} {}, {} per second; warm-up {} s, measure {} s, attribution {}",
        s.bots.count,
        s.bots.behaviour.name(),
        s.bots.rate,
        s.warmup.as_secs_f64(),
        s.measure.as_secs_f64(),
        if s.attribution { "on" } else { "off" }
    );
    println!(
        "server: {} on 127.0.0.1:{}, commit {}{}",
        plan.server_dir.display(),
        s.server.port,
        plan.commit,
        if plan.dirty {
            " (uncommitted changes)"
        } else {
            ""
        }
    );
    println!(
        "java: {}",
        plan.java_version.lines().next().unwrap_or("?").trim()
    );
    println!("command: {}", command_line(plan));
}

fn command_line(plan: &Plan) -> String {
    std::iter::once(plan.java.as_str())
        .chain(plan.jvm_args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A fresh server folder from the setup script; returns the jar's name and fingerprint.
fn prepare_server(plan: &Plan) -> Result<(String, String), String> {
    let dir = &plan.server_dir;
    if dir.exists() {
        let empty = fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none());
        if !empty && !dir.join(MARKER).is_file() {
            return Err(format!(
                "{} exists and was not created by the orchestrator (no {MARKER} file): \
                 choose another [server] dir or delete it yourself",
                dir.display()
            ));
        }
        // An accepted EULA stays accepted, as with setup alone.
        util::clear_dir_except(dir, &[MARKER, "eula.txt"]).map_err(|e| {
            format!(
                "cannot empty {} ({e}): is a server still running in it?",
                dir.display()
            )
        })?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    fs::write(
        dir.join(MARKER),
        "Server folder of the bench orchestrator (gamma-bots run): emptied before every run.\n",
    )
    .map_err(|e| format!("cannot write in {}: {e}", dir.display()))?;

    let setup_dir = plan.repo.join("tools").join("test-server");
    let mut command = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(setup_dir.join("setup.ps1"))
            .arg("-Target")
            .arg(dir);
        if plan.accept_eula {
            c.arg("-AcceptEula");
        }
        c
    } else {
        let mut c = Command::new("bash");
        c.arg(setup_dir.join("setup.sh")).arg("--target").arg(dir);
        if plan.accept_eula {
            c.arg("--accept-eula");
        }
        c
    };
    println!("preparing {} ...", dir.display());
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run the setup script: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        println!("  {line}");
    }
    if !output.status.success() {
        return Err(format!(
            "the setup script failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let eula = fs::read_to_string(dir.join("eula.txt")).unwrap_or_default();
    if !eula.contains("eula=true") {
        return Err(
            "the Minecraft EULA is not accepted in the server folder: run again with \
             --accept-eula if you accept it (https://aka.ms/MinecraftEULA)"
                .into(),
        );
    }
    let mut jar_name = text
        .lines()
        .find_map(|l| {
            l.split_once("Server jar: ")
                .map(|(_, name)| name.trim().to_owned())
        })
        .unwrap_or_else(|| "server.jar".into());
    if let Some(jar) = &plan.jar {
        fs::copy(jar, dir.join("server.jar"))
            .map_err(|e| format!("cannot copy {}: {e}", jar.display()))?;
        jar_name = jar
            .file_name()
            .map_or(jar_name, |n| n.to_string_lossy().into_owned());
    }
    let jar_sha256 = sha256::file_hex(&dir.join("server.jar"))
        .map_err(|e| format!("cannot read server.jar: {e}"))?;
    println!("server jar {jar_name}, sha256 {jar_sha256}");
    Ok((jar_name, jar_sha256))
}

/// Back to the state right after setup, then the scenario's settings and world.
fn reset_server(plan: &Plan) -> Result<(), String> {
    let dir = &plan.server_dir;
    let io = |what: &str, e: std::io::Error| format!("{what} in {}: {e}", dir.display());
    util::clear_dir_except(dir, KEPT).map_err(|e| io("cannot clear the server folder", e))?;
    let templates = plan.repo.join("tools").join("test-server").join("config");
    for entry in fs::read_dir(&templates).map_err(|e| io("cannot read the test settings", e))? {
        let entry = entry.map_err(|e| io("cannot read the test settings", e))?;
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            fs::copy(entry.path(), dir.join(entry.file_name()))
                .map_err(|e| io("cannot copy the test settings", e))?;
        }
    }
    let mut settings = vec![
        ("server-ip".to_owned(), "127.0.0.1".to_owned()),
        ("online-mode".to_owned(), "false".to_owned()),
        (
            "server-port".to_owned(),
            plan.scenario.server.port.to_string(),
        ),
    ];
    settings.extend(plan.scenario.properties.iter().cloned());
    let path = dir.join("server.properties");
    let text = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, apply_properties(&text, &settings))
        .map_err(|e| io("cannot write server.properties", e))?;
    if plan.scenario.server.gc_log {
        fs::create_dir_all(dir.join("logs")).map_err(|e| io("cannot create logs", e))?;
    }
    if let Some(world) = &plan.world {
        util::copy_dir(world, &dir.join("world")).map_err(|e| io("cannot copy the world", e))?;
    }
    Ok(())
}

fn write_static_files(plan: &Plan, results: &Path) -> Result<(), String> {
    let write = |name: &str, text: &str| {
        fs::write(results.join(name), text)
            .map_err(|e| format!("cannot write {}: {e}", results.join(name).display()))
    };
    let mut scenario = String::new();
    if !plan.overrides.is_empty() {
        let sets: Vec<String> = plan
            .overrides
            .iter()
            .map(|o| format!("--set {o}"))
            .collect();
        let _ = writeln!(scenario, "# Played with: {}", sets.join(" "));
    }
    scenario.push_str(&plan.scenario_text);
    write("scenario.toml", &scenario)?;
    write(
        "command.txt",
        &format!(
            "cd {}\n{}\n",
            shell_quote(&plan.server_dir.display().to_string()),
            command_line(plan)
        ),
    )?;
    write("java-version.txt", &plan.java_version)
}

/// Plays one repetition; whatever happens, the server is stopped and what exists is collected.
fn play(plan: &Plan, index: u32, rep_dir: &Path) -> Repetition {
    let mut rep = Repetition {
        index,
        started: util::unix_now(),
        ..Repetition::default()
    };
    let result = fs::create_dir_all(rep_dir)
        .map_err(|e| format!("cannot create {}: {e}", rep_dir.display()))
        .and_then(|()| reset_server(plan));
    if let Err(e) = result {
        rep.error = Some(e);
        rep.ended = util::unix_now();
        eprintln!("error: {}", rep.error.as_deref().unwrap_or(""));
        return rep;
    }

    match ServerProcess::start(
        &plan.java,
        &plan.jvm_args,
        &plan.server_dir,
        &rep_dir.join("console.log"),
    ) {
        Err(e) => rep.error = Some(e),
        Ok(mut server) => {
            println!("server started, pid {}", server.pid);
            let mut fleet = None;
            let result = drive(plan, &mut server, &mut fleet, &mut rep);
            if let Some(fleet) = fleet {
                save_bots(fleet, rep_dir);
            }
            if let Err(e) = result {
                eprintln!("error: {e}");
                eprintln!("last console lines:");
                for line in server.tail() {
                    eprintln!("  | {line}");
                }
                rep.error = Some(e);
            }
            println!("stopping the server ...");
            let exit = server.shutdown(plan.scenario.server.stop_timeout);
            println!("server {exit}");
            rep.server_exit = Some(exit);
        }
    }
    collect(plan, rep_dir, &mut rep);
    rep.ended = util::unix_now();
    rep
}

/// Waits `duration` while the bots run, failing if the server dies or the user interrupts.
fn pause(
    server: &mut ServerProcess,
    fleet: &mut Option<Fleet>,
    duration: Duration,
) -> Result<(), String> {
    let until = Instant::now() + duration;
    loop {
        if let Some(how) = server.poll() {
            return Err(format!("the server {how}"));
        }
        if signal::interrupted() {
            return Err("interrupted".into());
        }
        let now = Instant::now();
        if now >= until {
            return Ok(());
        }
        let step = until.min(now + Duration::from_secs(1));
        match fleet {
            Some(fleet) => {
                fleet.wait(step);
            }
            None => thread::sleep(step - now),
        }
    }
}

/// Waits until every bot is in game or gave up, `limit` at most.
fn settle(
    server: &mut ServerProcess,
    fleet: &mut Option<Fleet>,
    limit: Duration,
) -> Result<(), String> {
    let until = Instant::now() + limit;
    while Instant::now() < until && !fleet.as_ref().map_or(true, Fleet::settled) {
        pause(server, fleet, Duration::from_millis(200))?;
    }
    if let Some(fleet) = fleet {
        println!("{} bots in game", fleet.in_game());
    }
    Ok(())
}

/// `{bots}` in a console command becomes the names of the bots in game, separated by spaces:
/// `spreadplayers … @a` sent from this server's console moved no player and printed nothing.
/// `None` when the command needs names and no bot is in game.
fn expand(command: &str, names: &[String]) -> Option<String> {
    if !command.contains("{bots}") {
        return Some(command.to_owned());
    }
    (!names.is_empty()).then(|| command.replace("{bots}", &names.join(" ")))
}

fn drive(
    plan: &Plan,
    server: &mut ServerProcess,
    fleet: &mut Option<Fleet>,
    rep: &mut Repetition,
) -> Result<(), String> {
    let s = &plan.scenario;
    let boot = Instant::now();
    let timeout = s.server.startup_timeout;
    server
        .wait_for(timeout, |line| {
            line.contains("Done (") && line.contains("For help")
        })
        .map_err(|e| e.describe("waiting for the server to start", timeout))?;
    let startup = boot.elapsed().as_secs_f64();
    rep.startup_s = Some(startup);
    println!("server up in {startup:.1} s");

    for command in &s.at_start {
        server.send(command)?;
    }
    if s.bots.count > 0 {
        let mut bots = Fleet::connect(&s.bots)?;
        println!("starting {}, {} per second", bots.title, s.bots.rate);
        let arrived = bots.arrive(None);
        // Kept even when interrupted: the caller stops the bots that started.
        *fleet = Some(bots);
        if !arrived || signal::interrupted() {
            return Err("interrupted".into());
        }
        // Every bot in game or given up: the longest a bot may try, plus its retry pauses.
        let limit = (s.bots.join_timeout + Duration::from_secs(2)) * s.bots.attempts;
        settle(server, fleet, limit)?;
    }
    let names = fleet.as_ref().map(Fleet::in_game_names).unwrap_or_default();
    for command in &s.after_arrival {
        match expand(command, &names) {
            Some(command) => server.send(&command)?,
            None => eprintln!("warning: no bot in game, not sent: {command}"),
        }
    }

    println!("warm-up, {} s", s.warmup.as_secs_f64());
    pause(server, fleet, s.warmup)?;

    let name = format!("{}-rep{}", s.name, rep.index);
    let attribution = if s.attribution { " attribution" } else { "" };
    server.send(&format!("autothread record start {name}{attribution}"))?;
    let line = server
        .wait_for(COMMAND_TIMEOUT, |line| {
            line.contains("Recording every tick to")
                || line.contains("A recording is already running")
                || line.contains("Cannot start the recording")
                || line.contains("Unknown command")
        })
        .map_err(|e| e.describe("autothread record start", COMMAND_TIMEOUT))?;
    let Some((_, directory)) = line.split_once("Recording every tick to ") else {
        return Err(format!("autothread record start: {}", line.trim()));
    };
    let directory = directory
        .split(", with time per class")
        .next()
        .unwrap_or(directory)
        .trim();
    rep.recording = Some(directory.to_owned());
    rep.in_game_at_record_start = fleet.as_ref().map(Fleet::in_game);
    println!(
        "recording to {directory}{}; measure, {} s",
        rep.in_game_at_record_start
            .map_or(String::new(), |n| format!(", {n} bots in game")),
        s.measure.as_secs_f64()
    );
    pause(server, fleet, s.measure)?;

    rep.in_game_at_record_stop = fleet.as_ref().map(Fleet::in_game);
    server.send("autothread record stop")?;
    let line = server
        .wait_for(RECORD_STOP_TIMEOUT, |line| {
            line.contains("Recording stopped:") || line.contains("No recording is running")
        })
        .map_err(|e| e.describe("autothread record stop", RECORD_STOP_TIMEOUT))?;
    let Some((_, summary)) = line.split_once("Recording stopped: ") else {
        return Err(format!("autothread record stop: {}", line.trim()));
    };
    if let Some((_, files)) = summary.rsplit_once(". Files: ") {
        rep.recording = Some(files.trim().to_owned());
    }
    println!("recording stopped: {}", summary.trim());
    Ok(())
}

/// Stops the bots and writes `bots.csv` and `bots.txt` (the summary printed at the end).
fn save_bots(fleet: Fleet, rep_dir: &Path) {
    let title = fleet.title.clone();
    let (rows, elapsed) = fleet.finish();
    let summary = report::summary(&rows, elapsed, &title);
    print!("{summary}");
    if let Err(e) = fs::write(rep_dir.join("bots.txt"), &summary) {
        eprintln!("warning: cannot write bots.txt: {e}");
    }
    if let Err(e) = report::write_csv(&rep_dir.join("bots.csv"), &rows) {
        eprintln!("warning: cannot write bots.csv: {e}");
    }
}

/// Copies the recording, the server log, GC logs and crash reports into the repetition folder.
fn collect(plan: &Plan, rep_dir: &Path, rep: &mut Repetition) {
    let dir = &plan.server_dir;
    let copy = |from: PathBuf, to: PathBuf| {
        if let Err(e) = fs::copy(&from, &to) {
            eprintln!("warning: cannot copy {}: {e}", from.display());
        }
    };
    // The recording folder is relative to the server folder, with the server's separators.
    let recording = rep.recording.as_deref().map(|r| dir.join(r)).or_else(|| {
        let bench = dir.join("gammaengine").join("bench");
        let mut found: Vec<PathBuf> = fs::read_dir(bench)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        found.sort();
        found.pop()
    });
    if let Some(recording) = recording {
        if let Ok(entries) = fs::read_dir(&recording) {
            for entry in entries.flatten() {
                copy(entry.path(), rep_dir.join(entry.file_name()));
            }
        }
    }
    let logs = dir.join("logs");
    if let Ok(entries) = fs::read_dir(&logs) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "latest.log" || (name.starts_with("gc-") && name.ends_with(".log")) {
                copy(entry.path(), rep_dir.join(&name));
            }
        }
    }
    let crashes = dir.join("crash-reports");
    if crashes.is_dir() {
        if let Err(e) = util::copy_dir(&crashes, &rep_dir.join("crash-reports")) {
            eprintln!("warning: cannot copy the crash reports: {e}");
        }
    }
    if rep.error.is_none() && !rep_dir.join("summary.json").is_file() {
        rep.error = Some("the recording left no summary.json".into());
    }
}

/// One line per repetition for the end of the run.
fn rep_line(results: &Path, rep: &Repetition) -> String {
    let summary = fs::read_to_string(
        results
            .join(format!("rep{}", rep.index))
            .join("summary.json"),
    );
    let mut text = match summary.map(|t| Sample::from_summary(&t)) {
        Ok(Ok(s)) => {
            let f = |v: Option<f64>, d: usize| v.map_or("?".into(), |v| format!("{v:.d$}"));
            format!(
                "TPS {}, MSPT mean {} p95 {} max {} ms, players {}, {} ms per player, \
                 GC {} pauses {} ms",
                f(s.tps, 2),
                f(s.mspt_mean, 2),
                f(s.mspt_p95, 2),
                f(s.mspt_max, 1),
                f(s.players_mean, 1),
                f(s.cost_per_player_ms, 3),
                f(s.gc_pauses, 0),
                f(s.gc_pause_total_ms, 0)
            )
        }
        _ => "no recording".into(),
    };
    if let Some(error) = &rep.error {
        let _ = write!(text, " (error: {error})");
    }
    text
}

fn json_opt_string(value: Option<&str>) -> String {
    value.map_or("null".into(), json_string)
}

fn json_opt<T: ToString>(value: Option<T>) -> String {
    value.map_or("null".into(), |v| v.to_string())
}

fn write_run_json(
    plan: &Plan,
    results: &Path,
    started: i64,
    jar_name: &str,
    jar_sha256: &str,
    reps: &[Repetition],
) -> Result<(), String> {
    let s = &plan.scenario;
    let mut j = String::from("{\n");
    let mut field = |key: &str, value: String| {
        let _ = writeln!(j, "  {}: {value},", json_string(key));
    };
    field("format", "1".into());
    field("scenario", json_string(&s.name));
    field("description", json_string(&s.description));
    field(
        "scenario_file",
        json_string(&plan.scenario_path.display().to_string()),
    );
    field("overrides", util::json_strings(&plan.overrides));
    field("started", json_string(&util::iso(started)));
    field("ended", json_string(&util::iso(util::unix_now())));
    field("git_commit", json_string(&plan.commit));
    field("git_dirty", plan.dirty.to_string());
    field(
        "server_dir",
        json_string(&plan.server_dir.display().to_string()),
    );
    field("server_jar", json_string(jar_name));
    field("server_jar_sha256", json_string(jar_sha256));
    field("java", json_string(&plan.java));
    field(
        "java_version",
        json_string(plan.java_version.lines().next().unwrap_or("").trim()),
    );
    let mut command = vec![plan.java.clone()];
    command.extend(plan.jvm_args.iter().cloned());
    field("command", util::json_strings(&command));
    field("heap", json_string(&s.server.heap));
    field("gc", json_string(s.server.gc.name()));
    field("port", s.server.port.to_string());
    field(
        "world",
        match (&plan.world, &plan.world_sha256) {
            (Some(path), Some(hash)) => format!(
                "{{\"path\": {}, \"sha256\": {}}}",
                json_string(&path.display().to_string()),
                json_string(hash)
            ),
            _ => "null".into(),
        },
    );
    let properties: Vec<String> = s
        .properties
        .iter()
        .map(|(k, v)| format!("{}: {}", json_string(k), json_string(v)))
        .collect();
    field("properties", format!("{{{}}}", properties.join(", ")));
    let b = &s.bots;
    field(
        "bots",
        format!(
            "{{\"count\": {}, \"behaviour\": {}, \"rate\": {}, \"prefix\": {}, \"first\": {}, \
             \"seed\": {}, \"wander_radius\": {}, \"wander_height\": {}, \"fly_height\": {}}}",
            b.count,
            json_string(b.behaviour.name()),
            b.rate,
            json_string(&b.prefix),
            b.first,
            b.seed,
            b.wander_radius,
            b.wander_height,
            b.fly_height
        ),
    );
    field("warmup_s", s.warmup.as_secs_f64().to_string());
    field("measure_s", s.measure.as_secs_f64().to_string());
    field("attribution", s.attribution.to_string());
    field("at_start", util::json_strings(&s.at_start));
    field("after_arrival", util::json_strings(&s.after_arrival));
    field("repetitions_planned", s.repetitions.to_string());
    let reps: Vec<String> = reps
        .iter()
        .map(|r| {
            format!(
                "    {{\"index\": {}, \"dir\": \"rep{}\", \"started\": {}, \"ended\": {}, \
                 \"startup_s\": {}, \"recording\": {}, \"bots_in_game_at_record_start\": {}, \
                 \"bots_in_game_at_record_stop\": {}, \"server_exit\": {}, \"error\": {}}}",
                r.index,
                r.index,
                json_string(&util::iso(r.started)),
                json_string(&util::iso(r.ended)),
                json_opt(r.startup_s.map(|s| format!("{s:.1}"))),
                json_opt_string(r.recording.as_deref()),
                json_opt(r.in_game_at_record_start),
                json_opt(r.in_game_at_record_stop),
                json_opt_string(r.server_exit.as_deref()),
                json_opt_string(r.error.as_deref())
            )
        })
        .collect();
    let _ = write!(j, "  \"repetitions\": [\n{}\n  ]\n}}\n", reps.join(",\n"));
    fs::write(results.join("run.json"), j).map_err(|e| format!("cannot write run.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Option<Options>, String> {
        parse_options(line.split_whitespace().map(String::from))
    }

    #[test]
    fn options() {
        let o =
            parse("bench/scenarios/x.toml --set bots.count=50 --set server.heap=2G --accept-eula")
                .unwrap()
                .unwrap();
        assert_eq!(o.scenario, PathBuf::from("bench/scenarios/x.toml"));
        assert_eq!(o.overrides, ["bots.count=50", "server.heap=2G"]);
        assert!(o.accept_eula);
        assert!(!o.dry_run);
        assert!(parse("--help").unwrap().is_none());
        assert!(parse("").is_err());
        assert!(parse("a.toml b.toml").is_err());
        assert!(parse("a.toml --set").is_err());
        assert!(parse("a.toml --frobnicate").is_err());
    }

    #[test]
    fn properties_are_replaced_or_appended() {
        let text =
            "#comment\nmotd=GammaEngine test server\nserver-ip=127.0.0.1\nserver-port=25565\n\
                    online-mode=false\n";
        let settings = vec![
            ("server-port".to_owned(), "25570".to_owned()),
            ("view-distance".to_owned(), "6".to_owned()),
        ];
        assert_eq!(
            apply_properties(text, &settings),
            "#comment\nmotd=GammaEngine test server\nserver-ip=127.0.0.1\nserver-port=25570\n\
             online-mode=false\nview-distance=6\n"
        );
        // A commented key is not a setting.
        assert_eq!(
            apply_properties("#server-port=1\n", &settings[..1]),
            "#server-port=1\nserver-port=25570\n"
        );
    }

    #[test]
    fn bot_names_in_commands() {
        let names = vec!["bot1".to_owned(), "bot2".to_owned()];
        assert_eq!(
            expand("spreadplayers 0 0 64 384 false {bots}", &names).as_deref(),
            Some("spreadplayers 0 0 64 384 false bot1 bot2")
        );
        assert_eq!(
            expand("time set 6000", &[]).as_deref(),
            Some("time set 6000")
        );
        assert_eq!(expand("tp {bots} 0 100 0", &[]), None);
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("-Xmx1G"), "-Xmx1G");
        assert_eq!(
            shell_quote("@C:\\repo\\java9args.txt"),
            "@C:\\repo\\java9args.txt"
        );
        assert_eq!(
            shell_quote("C:\\Program Files\\java"),
            "\"C:\\Program Files\\java\""
        );
        assert_eq!(shell_quote(""), "\"\"");
    }

    #[test]
    fn repository_root() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo = find_repo(here).expect("tools/bench/bots is inside the repository");
        assert!(repo.join("bench").join("scenarios").is_dir());
        assert_eq!(
            in_repo(&repo, Path::new("test-server-orch")),
            repo.join("test-server-orch")
        );
    }

    #[test]
    fn run_json_is_valid() {
        let scenario = Scenario::parse("name = \"t\"\n", "t", &[]).unwrap();
        let plan = Plan {
            scenario,
            scenario_text: String::new(),
            scenario_path: PathBuf::from("bench/scenarios/t.toml"),
            overrides: vec!["bots.count=5".into()],
            repo: PathBuf::from("."),
            server_dir: PathBuf::from("test-server-orch"),
            jar: None,
            world: Some(PathBuf::from("w")),
            world_sha256: Some("ab".into()),
            java: "java".into(),
            java_version: "java version \"1.8.0_491\"\n".into(),
            jvm_args: vec!["-Xmx1G".into(), "nogui".into()],
            commit: "d6cc26cd".into(),
            dirty: false,
            accept_eula: false,
        };
        let dir = std::env::temp_dir().join(format!("gamma-bots-run-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let reps = [
            Repetition {
                index: 1,
                startup_s: Some(12.25),
                recording: Some("gammaengine\\bench\\t-rep1-20261008-150000".into()),
                in_game_at_record_start: Some(5),
                server_exit: Some("exited with code 0".into()),
                ..Repetition::default()
            },
            Repetition {
                index: 2,
                error: Some("the server \"crashed\"".into()),
                ..Repetition::default()
            },
        ];
        write_run_json(&plan, &dir, 0, "server.jar", "00", &reps).unwrap();
        let text = fs::read_to_string(dir.join("run.json")).unwrap();
        let json = gammaengine_protocol::json::Json::parse(&text).expect(&text);
        assert_eq!(json.get("scenario").and_then(|v| v.as_str()), Some("t"));
        assert_eq!(
            json.get("bots")
                .and_then(|b| b.get("count"))
                .and_then(|c| c.as_f64()),
            Some(20.0)
        );
        let reps = json.get("repetitions").and_then(|r| r.as_array()).unwrap();
        assert_eq!(reps.len(), 2);
        assert_eq!(
            reps[0].get("recording").and_then(|v| v.as_str()),
            Some("gammaengine\\bench\\t-rep1-20261008-150000")
        );
        assert_eq!(
            reps[1].get("error").and_then(|v| v.as_str()),
            Some("the server \"crashed\"")
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
