//! One bot: a TCP connection, a reader thread that answers the server, and a ticker thread that
//! moves every 50 ms.
//!
//! Two blocking threads per bot instead of read timeouts: on Windows a receive that times out
//! leaves the socket in an undefined state, which a stream protocol cannot afford.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use gammaengine_protocol::fml::{self, ClientHandshake, HandshakeMessage, ModEntry};
use gammaengine_protocol::frame::{self, Decoded, FrameDecoder};
use gammaengine_protocol::ids::{
    self, login::clientbound as login_id, play::clientbound as play_id, Direction, State,
};
use gammaengine_protocol::packets::handshake::{Handshake, NEXT_LOGIN};
use gammaengine_protocol::packets::play_clientbound as cb;
use gammaengine_protocol::packets::play_serverbound as sb;
use gammaengine_protocol::packets::{login, Packet};
use gammaengine_protocol::status::chat_to_plain;

use crate::behaviour::{MoveSettings, Mover, Position, Step, MAX_STEP};
use crate::signal;

/// What the server adds to the feet height in `S08`: the float 1.62 widened to a double.
pub const EYE_HEIGHT: f64 = 1.62f32 as f64;
pub const TICK: Duration = Duration::from_millis(50);
/// A teleport longer than this is a new spawn point, not a setback.
const TELEPORT_DISTANCE: f64 = 16.0;
/// A respawn request that got no answer is sent again after this long.
const RESPAWN_RETRY: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a stopping bot waits for the server to close its side.
const CLOSE_GRACE: Duration = Duration::from_secs(3);
/// Pause before connecting again after a failed join.
const RETRY_DELAY: Duration = Duration::from_secs(1);
const THREAD_STACK: usize = 256 * 1024;

pub struct BotSettings {
    /// Sent in the handshake, as a client would.
    pub host: String,
    pub port: u16,
    pub addr: SocketAddr,
    pub movement: MoveSettings,
    pub mods: Vec<ModEntry>,
    pub connect_timeout: Duration,
    /// A connection that has not reached `S01 JoinGame` after this long is dropped.
    pub join_timeout: Duration,
    /// Connections a bot may open before it joins.
    pub attempts: u32,
    /// The bot numbered `trace_index` prints its first `trace` received packets.
    pub trace: u32,
    pub trace_index: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BotStats {
    pub started: bool,
    /// Connections opened, 1 unless joining failed.
    pub attempts: u32,
    /// Why the first connection failed, when the bot had to connect again.
    pub first_failure: Option<String>,
    pub joined: bool,
    /// From the TCP connection to `S01 JoinGame`.
    pub join_ms: Option<u64>,
    /// From `S01 JoinGame` to the first chunk packet.
    pub first_chunk_ms: Option<u64>,
    pub in_game_ms: u64,
    pub connected_at_end: bool,
    pub kick_reason: Option<String>,
    pub error: Option<String>,
    pub keepalives: u64,
    pub keepalive_interval_total_ms: f64,
    pub keepalive_intervals: u64,
    /// The server sends a keep-alive every 40 ticks: the longest gap shows its worst stall.
    pub keepalive_interval_max_ms: f64,
    pub bytes_received: u64,
    pub chunk_packets: u64,
    pub s08_received: u64,
    /// `S08` that sent the bot back near where it was: a move the server refused.
    pub setbacks: u64,
    pub deaths: u64,
    /// Last position, feet height.
    pub position: Option<Position>,
}

impl BotStats {
    pub fn keepalive_interval_ms(&self) -> Option<f64> {
        (self.keepalive_intervals > 0)
            .then(|| self.keepalive_interval_total_ms / self.keepalive_intervals as f64)
    }
}

/// Shared between the bot's threads and the main thread.
pub struct BotSlot {
    pub name: String,
    pub index: u32,
    pub stats: Mutex<BotStats>,
    pub finished: AtomicBool,
}

impl BotSlot {
    pub fn new(name: String, index: u32) -> BotSlot {
        BotSlot {
            name,
            index,
            stats: Mutex::new(BotStats::default()),
            finished: AtomicBool::new(false),
        }
    }

    pub fn stats(&self) -> MutexGuard<'_, BotStats> {
        lock(&self.stats)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn millis(d: Duration) -> u64 {
    d.as_millis().min(u128::from(u64::MAX)) as u64
}

/// Protocol state shared by the reader and the ticker. It produces bytes and never touches the
/// socket, so the tests drive it without one.
struct Session {
    play: bool,
    connected_at: Instant,
    joined_at: Option<Instant>,
    /// Last position confirmed to the server or sent since.
    position: Option<Position>,
    dead_since: Option<Instant>,
    /// The next `S08` places the bot (after `S07 Respawn`).
    expect_spawn: bool,
    settle: Settle,
    /// Ground flag of the last movement packet.
    on_ground: bool,
    last_keepalive: Option<Instant>,
    join_timeout: Duration,
    /// Set by the ticker when it gave up waiting for `S01 JoinGame`.
    join_timed_out: bool,
    /// Received packets still to print (`--trace`).
    trace: u32,
    mover: Mover,
    fml: ClientHandshake,
}

enum Flow {
    Continue,
    /// The server said goodbye.
    Close,
}

/// Where the bot stands in the server's handling of moves after a teleport.
///
/// After an `S08`, CraftBukkit's `NetHandlerPlayServer.processPlayer` keeps `justTeleported`
/// set. The second movement packet that reaches its `PlayerMoveEvent` block (a move of more than
/// `sqrt(2/256)` block) finds the player's recorded position one move behind, returns early and
/// clears the flag: that packet is dropped without an answer. The next one is then measured
/// from two moves back, and if it hits a block the double distance trips "moved wrongly". So the
/// packet after the first real move repeats its position: dropping it costs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settle {
    Done,
    AwaitFirstMove,
    RepeatNext,
}

/// Whether the server's `PlayerMoveEvent` block sees this move (squared distance above 2/256).
fn moves_enough_for_event(from: Position, to: Position) -> bool {
    let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
    dx * dx + dy * dy + dz * dz > f64::from(2f32 / 256.0)
}

fn wanted(play: bool, id: i32) -> bool {
    !play
        || matches!(
            id,
            play_id::KEEP_ALIVE
                | play_id::JOIN_GAME
                | play_id::SPAWN_POSITION
                | play_id::UPDATE_HEALTH
                | play_id::RESPAWN
                | play_id::PLAYER_POS_LOOK
                | play_id::CUSTOM_PAYLOAD
                | play_id::DISCONNECT
        )
}

type Handled = Result<Flow, String>;

fn decode<P: Packet>(body: &[u8]) -> Result<P, String> {
    P::decode_body(body).map_err(|e| {
        let name = ids::packet_name(P::STATE, P::DIRECTION, P::ID).unwrap_or("?");
        format!("bad {name} packet: {e}")
    })
}

fn send<P: Packet>(out: &mut Vec<u8>, packet: &P) -> Result<(), String> {
    frame::write_packet(out, packet)
        .map_err(|e| format!("cannot encode packet {:#04x}: {e}", P::ID))
}

impl Session {
    fn handle(&mut self, id: i32, body: &[u8], stats: &mut BotStats, out: &mut Vec<u8>) -> Handled {
        if self.play {
            self.handle_play(id, body, stats, out)
        } else {
            self.handle_login(id, body, stats)
        }
    }

    fn handle_login(&mut self, id: i32, body: &[u8], stats: &mut BotStats) -> Handled {
        match id {
            login_id::LOGIN_SUCCESS => {
                decode::<login::LoginSuccess>(body)?;
                self.play = true;
                Ok(Flow::Continue)
            }
            login_id::DISCONNECT => {
                let reason = decode::<login::Disconnect>(body)?.reason;
                stats.kick_reason = Some(chat_to_plain(&reason));
                Ok(Flow::Close)
            }
            login_id::ENCRYPTION_REQUEST => {
                Err("the server asks for encryption: it is in online mode".into())
            }
            other => Err(format!("unexpected login packet {other:#04x}")),
        }
    }

    fn handle_play(
        &mut self,
        id: i32,
        body: &[u8],
        stats: &mut BotStats,
        out: &mut Vec<u8>,
    ) -> Handled {
        let now = Instant::now();
        match id {
            play_id::KEEP_ALIVE => {
                let keepalive = decode::<cb::KeepAlive>(body)?;
                send(out, &sb::KeepAlive { id: keepalive.id })?;
                stats.keepalives += 1;
                if let Some(previous) = self.last_keepalive {
                    let interval = (now - previous).as_secs_f64() * 1000.0;
                    stats.keepalive_interval_total_ms += interval;
                    stats.keepalive_interval_max_ms = stats.keepalive_interval_max_ms.max(interval);
                    stats.keepalive_intervals += 1;
                }
                self.last_keepalive = Some(now);
            }
            play_id::JOIN_GAME => {
                let join = decode::<cb::JoinGame>(body)?;
                self.joined_at = Some(now);
                stats.joined = true;
                stats.join_ms = Some(millis(now - self.connected_at));
                // The 1.7.10 server ignores the view distance; the rest mirrors a default client.
                send(
                    out,
                    &sb::ClientSettings {
                        locale: "en_US".into(),
                        view_distance: 8,
                        chat_visibility: 0,
                        chat_colors: true,
                        difficulty: join.difficulty as i8,
                        show_cape: true,
                    },
                )?;
            }
            play_id::SPAWN_POSITION => {
                let spawn = decode::<cb::SpawnPosition>(body)?;
                self.mover.set_world_spawn_y(f64::from(spawn.y));
            }
            play_id::UPDATE_HEALTH => {
                let health = decode::<cb::UpdateHealth>(body)?.health;
                if health <= 0.0 {
                    if self.dead_since.is_none() {
                        self.dead_since = Some(now);
                        stats.deaths += 1;
                        send(
                            out,
                            &sb::ClientStatus {
                                action: sb::STATUS_RESPAWN,
                            },
                        )?;
                    }
                } else {
                    self.dead_since = None;
                }
            }
            play_id::RESPAWN => {
                decode::<cb::Respawn>(body)?;
                self.expect_spawn = true;
            }
            play_id::PLAYER_POS_LOOK => {
                let look = decode::<cb::PlayerPosLook>(body)?;
                stats.s08_received += 1;
                if self.confirm_position(&look, out)? {
                    stats.setbacks += 1;
                }
            }
            play_id::CUSTOM_PAYLOAD => {
                let payload = decode::<cb::CustomPayload>(body)?;
                if payload.channel == fml::REGISTER_CHANNEL && !self.fml.is_done() {
                    // A client with the server's mods registers the same channels.
                    let channels = fml::parse_register(&payload.data)
                        .into_iter()
                        .filter(|c| c != fml::HANDSHAKE_CHANNEL && c != "FML")
                        .collect();
                    self.fml.set_channels(channels);
                } else if payload.channel == fml::HANDSHAKE_CHANNEL {
                    let message = HandshakeMessage::decode(&payload.data)
                        .map_err(|e| format!("bad FML|HS message: {e}"))?;
                    let replies = self
                        .fml
                        .handle(&message)
                        .map_err(|e| format!("FML handshake: {e}"))?;
                    for reply in replies {
                        send(
                            out,
                            &sb::CustomPayload {
                                channel: reply.channel.into(),
                                data: reply.data,
                            },
                        )?;
                    }
                }
            }
            play_id::DISCONNECT => {
                let reason = decode::<cb::Disconnect>(body)?.reason;
                stats.kick_reason = Some(chat_to_plain(&reason));
                return Ok(Flow::Close);
            }
            _ => {}
        }
        Ok(Flow::Continue)
    }

    /// Answers `S08` with the same x and z, bit for bit, and the feet height: until it does, the
    /// server ignores every move and sends the position again once a second. Returns whether
    /// the `S08` undid one of our moves.
    fn confirm_position(
        &mut self,
        look: &cb::PlayerPosLook,
        out: &mut Vec<u8>,
    ) -> Result<bool, String> {
        let feet = look.y - EYE_HEIGHT;
        send(
            out,
            &sb::PlayerPosLook {
                x: look.x,
                y: feet,
                stance: look.y,
                z: look.z,
                yaw: look.yaw,
                pitch: look.pitch,
                on_ground: false,
            },
        )?;
        let at = Position {
            x: look.x,
            y: feet,
            z: look.z,
        };
        if self.position == Some(at) && !self.expect_spawn {
            // The same teleport again: the server resends it while our moves sent before the
            // first copy arrive. Confirming is all it needs.
            self.settle = Settle::AwaitFirstMove;
            return Ok(false);
        }
        let setback = match self.position {
            Some(previous) => !self.expect_spawn && previous.distance(at) <= TELEPORT_DISTANCE,
            None => false,
        };
        if setback {
            self.mover.on_setback(at);
        } else {
            self.mover.on_spawn(at);
        }
        self.position = Some(at);
        self.expect_spawn = false;
        self.settle = Settle::AwaitFirstMove;
        Ok(setback)
    }

    /// Prints a received packet to stderr while the trace budget lasts.
    fn trace(&mut self, name: &str, id: i32, len: usize, body: Option<&[u8]>) {
        if self.trace == 0 {
            return;
        }
        self.trace -= 1;
        let state = if self.play { State::Play } else { State::Login };
        let packet = ids::packet_name(state, Direction::Clientbound, id).unwrap_or("?");
        let mut detail = String::new();
        if let (State::Play, play_id::CUSTOM_PAYLOAD, Some(body)) = (state, id, body) {
            if let Ok(payload) = cb::CustomPayload::decode_body(body) {
                detail = format!(" {}", payload.channel);
                if payload.channel == fml::HANDSHAKE_CHANNEL {
                    if let Ok(message) = HandshakeMessage::decode(&payload.data) {
                        detail = format!("{detail} {}", message.name());
                    }
                }
            }
        }
        eprintln!("[{name}] <- {id:#04x} {packet}{detail}, {len} bytes");
    }

    /// How far the connection got, for bots that never joined.
    fn stage(&self) -> String {
        if !self.play {
            "no LoginSuccess".into()
        } else if self.joined_at.is_none() {
            format!("no JoinGame, FML handshake at {:?}", self.fml.phase())
        } else {
            "in game".into()
        }
    }

    fn on_skipped(&mut self, id: i32, stats: &mut BotStats) {
        if self.play && (id == play_id::CHUNK_DATA || id == play_id::MAP_CHUNK_BULK) {
            stats.chunk_packets += 1;
            if stats.first_chunk_ms.is_none() {
                stats.first_chunk_ms = self.joined_at.map(|at| millis(at.elapsed()));
            }
        }
    }

    /// The packet of this tick, if any.
    fn tick(&mut self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        if self.joined_at.is_none() {
            return Ok(out);
        }
        if let Some(since) = self.dead_since {
            if since.elapsed() >= RESPAWN_RETRY {
                self.dead_since = Some(Instant::now());
                send(
                    &mut out,
                    &sb::ClientStatus {
                        action: sb::STATUS_RESPAWN,
                    },
                )?;
            }
            return Ok(out);
        }
        let Some(position) = self.position else {
            return Ok(out);
        };
        let (to, on_ground) = if self.settle == Settle::RepeatNext {
            // This packet is the one the server drops: it must not carry any progress.
            self.settle = Settle::Done;
            (position, self.on_ground)
        } else {
            match self.mover.step(position) {
                Step::Stay { on_ground } => {
                    self.on_ground = on_ground;
                    send(&mut out, &sb::Player { on_ground })?;
                    return Ok(out);
                }
                Step::Move { to, on_ground } => {
                    let to = position.clamp_step(to, MAX_STEP);
                    if self.settle == Settle::AwaitFirstMove && moves_enough_for_event(position, to)
                    {
                        self.settle = Settle::RepeatNext;
                    }
                    (to, on_ground)
                }
            }
        };
        self.position = Some(to);
        self.on_ground = on_ground;
        send(
            &mut out,
            &sb::PlayerPosition {
                x: to.x,
                y: to.y,
                stance: to.y + EYE_HEIGHT,
                z: to.z,
                on_ground,
            },
        )?;
        Ok(out)
    }
}

/// Runs one bot until the server drops it or [`signal::STOP`] is raised. A bot that fails to join
/// connects again, up to `attempts` connections in all. Network errors end up in the stats.
pub fn run(slot: Arc<BotSlot>, settings: Arc<BotSettings>) {
    slot.stats().started = true;
    let mut attempt = 0;
    loop {
        attempt += 1;
        slot.stats().attempts = attempt;
        let mut ending = Ending::default();
        let result = connect_and_play(&slot, &settings, &mut ending);
        let mut stats = slot.stats();
        let retry = result.is_err()
            && !stats.joined
            && stats.kick_reason.is_none()
            && attempt < settings.attempts
            && !signal::stop_requested();
        if retry {
            if let Err(error) = &result {
                stats.first_failure.get_or_insert_with(|| error.clone());
            }
            drop(stats);
            let resume = Instant::now() + RETRY_DELAY;
            while Instant::now() < resume && !signal::stop_requested() {
                thread::sleep(Duration::from_millis(50));
            }
            continue;
        }
        stats.in_game_ms = ending.joined_at.map_or(0, |at| millis(at.elapsed()));
        stats.position = ending.position;
        match result {
            Ok(()) => {
                let ended_by_us = signal::stop_requested() && stats.kick_reason.is_none();
                stats.connected_at_end = ended_by_us && stats.joined;
                if ended_by_us && !stats.joined && stats.error.is_none() {
                    stats.error =
                        Some(format!("not in game when the run ended ({})", ending.stage));
                }
            }
            Err(error) => {
                if stats.kick_reason.is_none() {
                    stats.error = Some(error);
                }
            }
        }
        break;
    }
    slot.finished.store(true, Ordering::Release);
}

/// What the session knew when it ended.
#[derive(Default)]
struct Ending {
    joined_at: Option<Instant>,
    position: Option<Position>,
    stage: String,
}

fn connect_and_play(
    slot: &BotSlot,
    settings: &BotSettings,
    ending: &mut Ending,
) -> Result<(), String> {
    let fail = |what: &str, e: io::Error| format!("{what}: {e}");
    let connected_at = Instant::now();
    let mut stream = TcpStream::connect_timeout(&settings.addr, settings.connect_timeout)
        .map_err(|e| fail("connect", e))?;
    stream.set_nodelay(true).map_err(|e| fail("socket", e))?;
    stream
        .set_write_timeout(Some(WRITE_TIMEOUT))
        .map_err(|e| fail("socket", e))?;
    let mut reader = stream.try_clone().map_err(|e| fail("socket", e))?;

    let mut hello = Vec::new();
    let handshake = Handshake {
        protocol_version: ids::PROTOCOL_VERSION,
        server_address: settings.host.clone(),
        server_port: settings.port,
        next_state: NEXT_LOGIN,
    };
    send(&mut hello, &handshake)?;
    send(
        &mut hello,
        &login::LoginStart {
            name: slot.name.clone(),
        },
    )?;
    stream.write_all(&hello).map_err(|e| fail("send", e))?;

    let link = Arc::new(Mutex::new(Link {
        session: Session {
            play: false,
            connected_at,
            joined_at: None,
            position: None,
            dead_since: None,
            expect_spawn: false,
            settle: Settle::Done,
            on_ground: true,
            last_keepalive: None,
            join_timeout: settings.join_timeout,
            join_timed_out: false,
            trace: if slot.index == settings.trace_index {
                settings.trace
            } else {
                0
            },
            mover: Mover::new(settings.movement, slot.index),
            fml: ClientHandshake::new(settings.mods.clone()),
        },
        out: stream,
    }));
    let closed = Arc::new(AtomicBool::new(false));
    let ticker = {
        let link = Arc::clone(&link);
        let closed = Arc::clone(&closed);
        thread::Builder::new()
            .name(format!("{}-tick", slot.name))
            .stack_size(THREAD_STACK)
            .spawn(move || tick_loop(&link, &closed))
    };
    let ticker = match ticker {
        Ok(handle) => handle,
        Err(e) => {
            let _ = lock(&link).out.shutdown(Shutdown::Both);
            return Err(fail("thread", e));
        }
    };

    let result = read_loop(&mut reader, &link, slot);
    closed.store(true, Ordering::Relaxed);
    let _ = lock(&link).out.shutdown(Shutdown::Both);
    let _ = ticker.join();
    let s = &lock(&link).session;
    ending.joined_at = s.joined_at;
    ending.position = s.position;
    ending.stage = s.stage();
    if s.join_timed_out {
        return Err(format!(
            "join timeout after {} s ({})",
            s.join_timeout.as_secs(),
            ending.stage
        ));
    }
    result
}

/// The session and the socket half it writes to, behind one lock so that packets leave in
/// the order the session produced them.
struct Link {
    session: Session,
    out: TcpStream,
}

fn tick_loop(link: &Mutex<Link>, closed: &AtomicBool) {
    let mut next = Instant::now() + TICK;
    loop {
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        }
        if signal::stop_requested() || closed.load(Ordering::Relaxed) {
            break;
        }
        next += TICK;
        let now = Instant::now();
        if now > next + Duration::from_secs(1) {
            // The machine stalled: resume the cadence instead of sending a burst of moves.
            next = now + TICK;
        }
        let mut guard = lock(link);
        let Link { session, out } = &mut *guard;
        if session.joined_at.is_none() && session.connected_at.elapsed() >= session.join_timeout {
            session.join_timed_out = true;
            break;
        }
        let sent = match session.tick() {
            Ok(bytes) if bytes.is_empty() => Ok(()),
            Ok(bytes) => out.write_all(&bytes).map_err(|_| ()),
            Err(_) => Err(()),
        };
        if sent.is_err() {
            break;
        }
    }
    if signal::stop_requested() && !closed.load(Ordering::Relaxed) {
        // A FIN rather than a reset: the server reads the end of the stream, logs a plain
        // "Disconnected" and closes, while the reader drains what was still in flight.
        let _ = lock(link).out.shutdown(Shutdown::Write);
        let deadline = Instant::now() + CLOSE_GRACE;
        while !closed.load(Ordering::Relaxed) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
    // Wakes the reader up if the server has not closed its side.
    let _ = lock(link).out.shutdown(Shutdown::Both);
}

fn read_loop(reader: &mut TcpStream, link: &Mutex<Link>, slot: &BotSlot) -> Result<(), String> {
    let mut decoder = FrameDecoder::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::with_capacity(512);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => return closed_by_peer(slot, "connection closed by the server"),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return closed_by_peer(slot, &format!("read: {e}")),
        };
        if signal::stop_requested() {
            // Draining until the server closes; nothing is answered any more.
            continue;
        }
        decoder.feed(&buf[..n]);
        let mut guard = lock(link);
        let Link {
            session,
            out: socket,
        } = &mut *guard;
        let mut stats = slot.stats();
        stats.bytes_received += n as u64;
        let mut flow = Flow::Continue;
        loop {
            let play = session.play;
            match decoder
                .next_filtered(|id| wanted(play, id))
                .map_err(|e| format!("protocol: {e}"))?
            {
                None => break,
                Some(Decoded::Skipped { id, len }) => {
                    session.trace(&slot.name, id, len, None);
                    session.on_skipped(id, &mut stats);
                }
                Some(Decoded::Frame(f)) => {
                    session.trace(&slot.name, f.id, f.body.len() + 1, Some(f.body));
                    flow = session.handle(f.id, f.body, &mut stats, &mut out)?;
                    if matches!(flow, Flow::Close) {
                        break;
                    }
                }
            }
        }
        drop(stats);
        if !out.is_empty() {
            let written = socket.write_all(&out);
            out.clear();
            if let Err(e) = written {
                return closed_by_peer(slot, &format!("send: {e}"));
            }
        }
        if matches!(flow, Flow::Close) {
            return Ok(());
        }
    }
}

/// End of stream: expected after a kick or when we stopped, an error otherwise.
fn closed_by_peer(slot: &BotSlot, what: &str) -> Result<(), String> {
    if signal::stop_requested() || slot.stats().kick_reason.is_some() {
        Ok(())
    } else {
        Err(what.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviour::Behaviour;

    #[test]
    fn eye_height_matches_the_server_constant() {
        // NetHandlerPlayServer adds 1.6200000047683716D to the feet height.
        assert_eq!(EYE_HEIGHT, 1.6200000047683716);
    }

    fn session(behaviour: Behaviour) -> Session {
        Session {
            play: true,
            connected_at: Instant::now(),
            joined_at: Some(Instant::now()),
            position: None,
            dead_since: None,
            expect_spawn: false,
            settle: Settle::Done,
            on_ground: true,
            last_keepalive: None,
            join_timeout: Duration::from_secs(30),
            join_timed_out: false,
            trace: 0,
            mover: Mover::new(
                MoveSettings {
                    behaviour,
                    ..MoveSettings::default()
                },
                1,
            ),
            fml: ClientHandshake::new(Vec::new()),
        }
    }

    fn frames(bytes: &[u8]) -> Vec<(i32, Vec<u8>)> {
        let mut decoder = FrameDecoder::new();
        decoder.feed(bytes);
        let mut out = Vec::new();
        while let Some(f) = decoder.next_frame().unwrap() {
            out.push((f.id, f.body.to_vec()));
        }
        out
    }

    #[test]
    fn s08_is_confirmed_bit_for_bit_before_moving() {
        let mut s = session(Behaviour::Wander);
        let mut stats = BotStats::default();
        let mut out = Vec::new();
        let look = cb::PlayerPosLook {
            x: 211.30000001,
            y: 72.62000000476837,
            z: -0.6999999,
            yaw: 12.5,
            pitch: 3.0,
            on_ground: false,
        };
        s.handle(
            play_id::PLAYER_POS_LOOK,
            &look.encode_body().unwrap(),
            &mut stats,
            &mut out,
        )
        .unwrap();
        let sent = frames(&out);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 0x06);
        let c06 = sb::PlayerPosLook::decode_body(&sent[0].1).unwrap();
        assert_eq!(c06.x.to_bits(), look.x.to_bits());
        assert_eq!(c06.z.to_bits(), look.z.to_bits());
        assert_eq!(c06.stance.to_bits(), look.y.to_bits());
        assert!((c06.y - 71.0).abs() < 1e-9);
        assert_eq!(stats.s08_received, 1);

        // The first move starts from the confirmed position and stays short.
        let moved = frames(&s.tick().unwrap());
        let c04 = sb::PlayerPosition::decode_body(&moved[0].1).unwrap();
        let from = Position {
            x: look.x,
            y: c06.y,
            z: look.z,
        };
        let first = Position {
            x: c04.x,
            y: c04.y,
            z: c04.z,
        };
        assert!(from.distance(first) < 0.25);
        assert!(moves_enough_for_event(from, first));
        assert_eq!(c04.stance - c04.y, EYE_HEIGHT);

        // The server drops the next one: it repeats the position, then walking resumes.
        let c04 = |bytes: Vec<u8>| sb::PlayerPosition::decode_body(&frames(&bytes)[0].1).unwrap();
        let repeat = c04(s.tick().unwrap());
        assert_eq!((repeat.x, repeat.y, repeat.z), (first.x, first.y, first.z));
        let next = c04(s.tick().unwrap());
        assert!(
            first.distance(Position {
                x: next.x,
                y: next.y,
                z: next.z
            }) > 0.1
        );
    }

    #[test]
    fn setbacks_are_told_apart_from_repeats_and_teleports() {
        let mut s = session(Behaviour::Wander);
        let mut stats = BotStats::default();
        let mut out = Vec::new();
        let at = |x: f64| {
            cb::PlayerPosLook {
                x,
                y: 65.62000000476837,
                z: 0.5,
                yaw: 0.0,
                pitch: 0.0,
                on_ground: false,
            }
            .encode_body()
            .unwrap()
        };
        let mut s08 = |s: &mut Session, x: f64| {
            s.handle(play_id::PLAYER_POS_LOOK, &at(x), &mut stats, &mut out)
                .unwrap();
        };
        s08(&mut s, 0.5);
        s08(&mut s, 0.5);
        s.tick().unwrap();
        s08(&mut s, 0.5);
        s.tick().unwrap();
        s08(&mut s, 100.5);
        assert_eq!(stats.s08_received, 4);
        assert_eq!(stats.setbacks, 1, "spawn, repeat, setback, then a teleport");
        assert_eq!(frames(&out).iter().filter(|(id, _)| *id == 0x06).count(), 4);
    }

    #[test]
    fn keepalive_is_echoed_and_death_triggers_respawn() {
        let mut s = session(Behaviour::Idle);
        let mut stats = BotStats::default();
        let mut out = Vec::new();
        s.handle(
            play_id::KEEP_ALIVE,
            &[0x12, 0x34, 0x56, 0x78],
            &mut stats,
            &mut out,
        )
        .unwrap();
        assert_eq!(frames(&out), vec![(0x00, vec![0x12, 0x34, 0x56, 0x78])]);
        assert_eq!(stats.keepalives, 1);

        out.clear();
        let dead = cb::UpdateHealth {
            health: 0.0,
            food: 20,
            saturation: 0.0,
        }
        .encode_body()
        .unwrap();
        s.handle(play_id::UPDATE_HEALTH, &dead, &mut stats, &mut out)
            .unwrap();
        s.handle(play_id::UPDATE_HEALTH, &dead, &mut stats, &mut out)
            .unwrap();
        assert_eq!(
            frames(&out),
            vec![(0x16, vec![0x00])],
            "one respawn request per death"
        );
        assert_eq!(stats.deaths, 1);
        assert!(s.tick().unwrap().is_empty(), "no movement while dead");
    }

    #[test]
    fn disconnect_reason_is_recorded() {
        let mut s = session(Behaviour::Idle);
        let mut stats = BotStats::default();
        let body = cb::Disconnect {
            reason: "\"Mod rejections [x]\"".into(),
        }
        .encode_body()
        .unwrap();
        let flow = s
            .handle(play_id::DISCONNECT, &body, &mut stats, &mut Vec::new())
            .unwrap();
        assert!(matches!(flow, Flow::Close));
        assert_eq!(stats.kick_reason.as_deref(), Some("Mod rejections [x]"));
    }

    #[test]
    fn only_needed_packets_are_decoded() {
        assert!(wanted(false, 0x26));
        assert!(wanted(true, play_id::PLAYER_POS_LOOK));
        assert!(!wanted(true, play_id::MAP_CHUNK_BULK));
        assert!(!wanted(true, play_id::CHUNK_DATA));
        assert!(!wanted(true, play_id::ENTITY_REL_MOVE));
    }
}
