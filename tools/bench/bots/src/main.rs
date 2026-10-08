//! `gamma-bots`: connects load bots to a Minecraft 1.7.10 Forge server in offline mode, keeps
//! them in game with a movement behaviour, and reports what each one saw.

mod args;
mod behaviour;
mod bot;
mod report;
mod signal;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gammaengine_protocol::frame::{self, FrameDecoder};
use gammaengine_protocol::ids::{self, status::clientbound as status_id};
use gammaengine_protocol::packets::handshake::{Handshake, NEXT_STATUS};
use gammaengine_protocol::packets::status::{Ping, ServerInfo, ServerQuery};
use gammaengine_protocol::packets::Packet;
use gammaengine_protocol::status::StatusResponse;

use args::{Args, Command};
use behaviour::MoveSettings;
use bot::{BotSettings, BotSlot};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Right after a heavy run, the test server was seen taking more than 5 s to answer the status.
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
/// How long stopped bots get to close their sockets before the report is written anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

fn main() -> ExitCode {
    let args = match args::parse(std::env::args().skip(1)) {
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

fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {host}"))
}

/// Server list ping: the status JSON (first of the two the server sends) and the ping time.
fn query_status(
    addr: SocketAddr,
    host: &str,
    port: u16,
) -> Result<(StatusResponse, Duration), String> {
    let io = |e: std::io::Error| format!("status: {e}");
    let proto = |e: gammaengine_protocol::Error| format!("status: {e}");
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(io)?;
    stream.set_read_timeout(Some(STATUS_TIMEOUT)).map_err(io)?;
    stream.set_write_timeout(Some(STATUS_TIMEOUT)).map_err(io)?;
    let mut out = Vec::new();
    let handshake = Handshake {
        protocol_version: ids::PROTOCOL_VERSION,
        server_address: host.to_owned(),
        server_port: port,
        next_state: NEXT_STATUS,
    };
    frame::write_packet(&mut out, &handshake).map_err(proto)?;
    frame::write_packet(&mut out, &ServerQuery).map_err(proto)?;
    stream.write_all(&out).map_err(io)?;

    let mut decoder = FrameDecoder::new();
    let mut buf = [0u8; 16 * 1024];
    let mut status = None;
    let mut ping_sent = None;
    loop {
        let n = stream.read(&mut buf).map_err(io)?;
        if n == 0 {
            return Err("status: connection closed before the answer".into());
        }
        decoder.feed(&buf[..n]);
        while let Some(f) = decoder.next_frame().map_err(proto)? {
            match f.id {
                status_id::SERVER_INFO if status.is_none() => {
                    let info = ServerInfo::decode_body(f.body).map_err(proto)?;
                    status = Some(StatusResponse::parse(&info.json).map_err(proto)?);
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as i64);
                    let ping = frame::encode_packet(&Ping { payload: stamp }).map_err(proto)?;
                    stream.write_all(&ping).map_err(io)?;
                    ping_sent = Some(Instant::now());
                }
                status_id::PONG => {
                    if let (Some(status), Some(sent)) = (status.take(), ping_sent) {
                        return Ok((status, sent.elapsed()));
                    }
                }
                _ => {}
            }
        }
    }
}

fn wait_for_status(args: &Args, addr: SocketAddr) -> Result<(StatusResponse, Duration), String> {
    let deadline = Instant::now() + args.wait;
    loop {
        match query_status(addr, &args.host, args.port) {
            Ok(found) => return Ok(found),
            Err(e) if Instant::now() < deadline && !signal::stop_requested() => {
                eprintln!("{e}; retrying");
                thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(format!("{e} ({}:{})", args.host, args.port)),
        }
    }
}

fn progress(slots: &[Arc<BotSlot>], elapsed: Duration) {
    let (mut started, mut in_game, mut kicked, mut errors, mut chunks, mut bytes) =
        (0, 0, 0, 0, 0u64, 0u64);
    for slot in slots {
        let finished = slot.finished.load(Ordering::Acquire);
        let s = slot.stats();
        started += usize::from(s.started);
        in_game +=
            usize::from(s.joined && !finished && s.kick_reason.is_none() && s.error.is_none());
        kicked += usize::from(s.kick_reason.is_some());
        errors += usize::from(s.error.is_some());
        chunks += s.chunk_packets;
        bytes += s.bytes_received;
    }
    println!(
        "[{:>5.0}s] started {started}/{}, in game {in_game}, kicked {kicked}, errors {errors}, \
         {chunks} chunk packets, {:.1} MiB",
        elapsed.as_secs_f64(),
        slots.len(),
        bytes as f64 / 1048576.0
    );
}

/// Sleeps until `until`, a stop request or the run deadline, printing progress on the way.
/// Returns false when the run must end.
fn wait_until(
    until: Instant,
    deadline: Instant,
    start: Instant,
    next_progress: &mut Instant,
    slots: &[Arc<BotSlot>],
) -> bool {
    loop {
        let now = Instant::now();
        if signal::stop_requested() || now >= deadline {
            return false;
        }
        if now >= *next_progress {
            progress(slots, now - start);
            *next_progress += PROGRESS_EVERY;
        }
        if now >= until {
            return true;
        }
        let wake = until.min(deadline).min(*next_progress);
        thread::sleep(
            wake.saturating_duration_since(now)
                .min(Duration::from_millis(100)),
        );
    }
}

/// Returns whether every bot joined and stayed until the end.
fn run(args: &Args) -> Result<bool, String> {
    let addr = resolve(&args.host, args.port)?;
    let (status, ping) = wait_for_status(args, addr)?;
    let mods = status.mod_list.clone().unwrap_or_default();
    println!(
        "server {}:{}: \"{}\", {} {}, {}/{} players, status ping {} ms",
        args.host,
        args.port,
        status.description,
        status.version_name.as_deref().unwrap_or("?"),
        status
            .protocol
            .map_or("protocol ?".to_owned(), |p| format!("protocol {p}")),
        status.players_online.unwrap_or(0),
        status.players_max.unwrap_or(0),
        ping.as_millis()
    );
    match &status.mod_list {
        Some(list) => println!(
            "FML mod list ({}): {}",
            list.len(),
            list.iter()
                .map(|m| format!("{}@{}", m.modid, m.version))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => {
            println!("no FML mod list: not a Forge server, the bots will join as vanilla clients")
        }
    }
    if let Some(protocol) = status.protocol {
        if protocol != i64::from(ids::PROTOCOL_VERSION) {
            return Err(format!(
                "the server speaks protocol {protocol}, the bots speak {}",
                ids::PROTOCOL_VERSION
            ));
        }
    }

    let settings = Arc::new(BotSettings {
        host: args.host.clone(),
        port: args.port,
        addr,
        movement: MoveSettings {
            behaviour: args.behaviour,
            seed: args.seed,
            wander_radius: args.wander_radius,
            wander_height: args.wander_height,
            fly_height: args.fly_height,
        },
        mods,
        connect_timeout: CONNECT_TIMEOUT,
        join_timeout: args.join_timeout,
        attempts: args.attempts,
        trace: args.trace,
        trace_index: args.first,
    });
    let slots: Vec<Arc<BotSlot>> = (0..args.count)
        .map(|i| Arc::new(BotSlot::new(args.name(i), args.first + i)))
        .collect();
    let title = format!("{} bots, {}", args.count, args.behaviour.name());
    println!(
        "starting {title}, {} per second, for {} s",
        if args.rate > 0.0 {
            args.rate.to_string()
        } else {
            "all".into()
        },
        args.duration.as_secs_f64()
    );

    let start = Instant::now();
    let deadline = start + args.duration;
    let mut next_progress = start + PROGRESS_EVERY;
    let mut handles = Vec::with_capacity(slots.len());
    for (i, slot) in slots.iter().enumerate() {
        let due = if args.rate > 0.0 {
            start + Duration::from_secs_f64(i as f64 / args.rate)
        } else {
            start
        };
        if !wait_until(due, deadline, start, &mut next_progress, &slots) {
            break;
        }
        let (slot, settings) = (Arc::clone(slot), Arc::clone(&settings));
        let spawned = thread::Builder::new()
            .name(slot.name.clone())
            .stack_size(256 * 1024)
            .spawn(move || bot::run(slot, settings));
        match spawned {
            Ok(handle) => handles.push(handle),
            Err(e) => {
                eprintln!("cannot start more bots: {e}");
                break;
            }
        }
    }
    wait_until(deadline, deadline, start, &mut next_progress, &slots);
    let elapsed = start.elapsed();
    signal::request_stop();

    let grace_end = Instant::now() + SHUTDOWN_GRACE;
    while handles.iter().any(|h| !h.is_finished()) && Instant::now() < grace_end {
        thread::sleep(Duration::from_millis(20));
    }
    let lingering = handles.iter().filter(|h| !h.is_finished()).count();
    if lingering > 0 {
        eprintln!(
            "warning: {lingering} bots did not close within {} s",
            SHUTDOWN_GRACE.as_secs()
        );
    }

    let rows: Vec<(String, bot::BotStats)> = slots
        .iter()
        .map(|s| (s.name.clone(), s.stats().clone()))
        .collect();
    print!("{}", report::summary(&rows, elapsed, &title));
    if let Some(path) = &args.report {
        report::write_csv(path, &rows)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!("report: {}", path.display());
    }
    Ok(rows.iter().all(|(_, s)| s.connected_at_end))
}
