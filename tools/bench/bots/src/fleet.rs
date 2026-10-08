//! A set of bots against one server: status ping, staggered arrival, progress lines, stop and
//! collection of the stats. Used by the plain command line and by the orchestrator.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gammaengine_protocol::frame::{self, FrameDecoder};
use gammaengine_protocol::ids::{self, status::clientbound as status_id};
use gammaengine_protocol::packets::handshake::{Handshake, NEXT_STATUS};
use gammaengine_protocol::packets::status::{Ping, ServerInfo, ServerQuery};
use gammaengine_protocol::packets::Packet;
use gammaengine_protocol::status::StatusResponse;

use crate::args::Args;
use crate::behaviour::MoveSettings;
use crate::bot::{self, BotSettings, BotSlot, BotStats};
use crate::signal;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Right after a heavy run, the test server was seen taking more than 5 s to answer the status.
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
/// How long stopped bots get to close their sockets before the report is written anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

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

pub struct Fleet {
    settings: Arc<BotSettings>,
    slots: Vec<Arc<BotSlot>>,
    handles: Vec<JoinHandle<()>>,
    rate: f64,
    start: Instant,
    next_progress: Instant,
    /// End of the run whatever the waits ask for: `--duration` of the command line. The
    /// orchestrator has none and stops the bots itself.
    deadline: Option<Instant>,
    pub title: String,
}

impl Fleet {
    /// Pings the server, prints what it answered and prepares the bots, without connecting them.
    pub fn connect(args: &Args) -> Result<Fleet, String> {
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
                println!(
                    "no FML mod list: not a Forge server, the bots will join as vanilla clients"
                )
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
        let now = Instant::now();
        Ok(Fleet {
            settings,
            handles: Vec::with_capacity(slots.len()),
            slots,
            rate: args.rate,
            start: now,
            next_progress: now + PROGRESS_EVERY,
            deadline: None,
            title: format!("{} bots, {}", args.count, args.behaviour.name()),
        })
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Starts the bots at the arrival rate, `duration` counted from now if given. Returns false
    /// if the run ended (stop request or deadline) before every bot started.
    pub fn arrive(&mut self, duration: Option<Duration>) -> bool {
        self.start = Instant::now();
        self.deadline = duration.map(|d| self.start + d);
        self.next_progress = self.start + PROGRESS_EVERY;
        for i in 0..self.slots.len() {
            let due = if self.rate > 0.0 {
                self.start + Duration::from_secs_f64(i as f64 / self.rate)
            } else {
                self.start
            };
            if !self.wait(due) {
                return false;
            }
            let (slot, settings) = (Arc::clone(&self.slots[i]), Arc::clone(&self.settings));
            let spawned = thread::Builder::new()
                .name(slot.name.clone())
                .stack_size(256 * 1024)
                .spawn(move || bot::run(slot, settings));
            match spawned {
                Ok(handle) => self.handles.push(handle),
                Err(e) => {
                    eprintln!("cannot start more bots: {e}");
                    return false;
                }
            }
        }
        true
    }

    /// Sleeps until `until`, a stop request or the deadline, printing progress on the way.
    /// Returns false when the run must end.
    pub fn wait(&mut self, until: Instant) -> bool {
        loop {
            let now = Instant::now();
            if signal::stop_requested() || self.deadline.is_some_and(|d| now >= d) {
                return false;
            }
            if now >= self.next_progress {
                progress(&self.slots, now - self.start);
                self.next_progress += PROGRESS_EVERY;
            }
            if now >= until {
                return true;
            }
            let mut wake = until.min(self.next_progress);
            if let Some(deadline) = self.deadline {
                wake = wake.min(deadline);
            }
            thread::sleep(
                wake.saturating_duration_since(now)
                    .min(Duration::from_millis(100)),
            );
        }
    }

    /// Names of the bots in game right now.
    pub fn in_game_names(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter(|slot| {
                let s = slot.stats();
                s.joined
                    && !slot.finished.load(Ordering::Acquire)
                    && s.kick_reason.is_none()
                    && s.error.is_none()
            })
            .map(|slot| slot.name.clone())
            .collect()
    }

    pub fn in_game(&self) -> usize {
        self.in_game_names().len()
    }

    /// True once every started bot is in game or gave up.
    pub fn settled(&self) -> bool {
        self.slots[..self.handles.len()]
            .iter()
            .all(|slot| slot.stats().joined || slot.finished.load(Ordering::Acquire))
    }

    /// Stops the bots, gives them [`SHUTDOWN_GRACE`] to close, and returns their stats with the
    /// time since the arrival started.
    pub fn finish(self) -> (Vec<(String, BotStats)>, Duration) {
        let elapsed = self.start.elapsed();
        signal::request_stop();

        let grace_end = Instant::now() + SHUTDOWN_GRACE;
        while self.handles.iter().any(|h| !h.is_finished()) && Instant::now() < grace_end {
            thread::sleep(Duration::from_millis(20));
        }
        let lingering = self.handles.iter().filter(|h| !h.is_finished()).count();
        if lingering > 0 {
            eprintln!(
                "warning: {lingering} bots did not close within {} s",
                SHUTDOWN_GRACE.as_secs()
            );
        }

        let rows = self
            .slots
            .iter()
            .map(|s| (s.name.clone(), s.stats().clone()))
            .collect();
        (rows, elapsed)
    }
}
