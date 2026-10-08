//! Where a bot goes on each tick.
//!
//! The server trusts the client's position but checks each move, and sends the player back with
//! an `S08` when a packet moves more than 10 blocks ("moved too quickly" in its log), when the new
//! position overlaps a block (silently), or when a collision leaves the player more than 0.25
//! block away horizontally from where it claimed to be ("moved wrongly"). A bot that never reads
//! chunks cannot see terrain, so it keeps clear of it: steps stay under one block, horizontal legs
//! run at a height above the obstacles, and each setback raises that height and makes the bot
//! hold still for a while, so that a stuck bot does not cost the server a teleport per tick.

use std::f64::consts::TAU;

pub const TICKS_PER_SECOND: f64 = 20.0;
/// Vanilla walking speed, 4.317 blocks per second.
pub const WALK_STEP: f64 = 4.317 / TICKS_PER_SECOND;
/// Sprinting speed, 5.6 blocks per second.
pub const EXPLORE_STEP: f64 = 5.6 / TICKS_PER_SECOND;
pub const CLIMB_STEP: f64 = 1.0;
/// Hard cap on one packet's displacement, below the server's 10 blocks.
pub const MAX_STEP: f64 = 9.0;
/// Bots never go above this feet height (the world is 256 blocks high).
pub const MAX_FLY_Y: f64 = 250.0;
/// Successive multiples spread headings evenly around the circle, whatever the bot count.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;
/// A wanderer waits up to this many ticks at each destination.
const MAX_PAUSE_TICKS: u64 = 40;
/// Height added after a setback at cruising height: a tree for wanderers, a hill for explorers.
const WANDER_RAISE: f64 = 2.0;
const EXPLORE_RAISE: f64 = 8.0;
/// Sideways walk after a climb hit something overhead, before climbing again: 2 s, about
/// 8.6 blocks, enough to leave a tree crown.
const ESCAPE_TICKS: u32 = 40;
/// Hold after a setback: 0.5 s, doubled for each further setback within 5 s, at most 30 s.
const HOLD_TICKS: u64 = 10;
const MAX_HOLD_TICKS: u64 = 600;
const CALM_TICKS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Stays put and sends `C03` (on ground) every tick.
    Idle,
    /// Walks to random points within a radius of its spawn, at constant height.
    Wander,
    /// Flies in a straight line, each bot in its own direction, at a fixed height.
    Explore,
}

impl Behaviour {
    pub fn parse(text: &str) -> Option<Behaviour> {
        match text {
            "idle" => Some(Behaviour::Idle),
            "wander" => Some(Behaviour::Wander),
            "explore" => Some(Behaviour::Explore),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Behaviour::Idle => "idle",
            Behaviour::Wander => "wander",
            Behaviour::Explore => "explore",
        }
    }
}

/// A player position; `y` is the feet height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Position {
    pub fn distance(self, other: Position) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }

    /// `to`, pulled back towards `self` if it is more than `max` away.
    pub fn clamp_step(self, to: Position, max: f64) -> Position {
        let d = self.distance(to);
        if !d.is_finite() {
            return self;
        }
        if d <= max {
            return to;
        }
        let k = max / d;
        Position {
            x: self.x + (to.x - self.x) * k,
            y: self.y + (to.y - self.y) * k,
            z: self.z + (to.z - self.z) * k,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    Stay { on_ground: bool },
    Move { to: Position, on_ground: bool },
}

/// SplitMix64: enough randomness for movement, and reproducible from the seed.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Movement parameters shared by every bot of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveSettings {
    pub behaviour: Behaviour,
    pub seed: u64,
    pub wander_radius: f64,
    /// Wanderers walk this many blocks above their spawn point; 0 keeps them on its level.
    pub wander_height: f64,
    /// Explorers fly this many blocks above the world spawn point.
    pub fly_height: f64,
}

impl Default for MoveSettings {
    fn default() -> Self {
        MoveSettings {
            behaviour: Behaviour::Idle,
            seed: 1,
            wander_radius: 16.0,
            wander_height: 0.0,
            fly_height: 100.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Mover {
    settings: MoveSettings,
    rng: Rng,
    heading: f64,
    anchor: Option<Position>,
    world_spawn_y: Option<f64>,
    target: Option<(f64, f64)>,
    pause: u64,
    /// Height gained from setbacks since the last spawn.
    raised: f64,
    /// Heading and remaining ticks of a sideways escape.
    escape: Option<(f64, u32)>,
    hold: u64,
    streak: u32,
    calm: u64,
}

impl Mover {
    pub fn new(settings: MoveSettings, index: u32) -> Mover {
        // One offset for the whole run, so the headings keep their even spread.
        let offset = Rng::new(settings.seed).next_f64() * TAU;
        Mover {
            settings,
            rng: Rng::new(settings.seed ^ u64::from(index).wrapping_mul(0x2545_f491_4f6c_dd1d)),
            heading: (f64::from(index) * GOLDEN_ANGLE + offset) % TAU,
            anchor: None,
            world_spawn_y: None,
            target: None,
            pause: 0,
            raised: 0.0,
            escape: None,
            hold: 0,
            streak: 0,
            calm: CALM_TICKS,
        }
    }

    #[cfg(test)]
    fn heading(&self) -> f64 {
        self.heading
    }

    /// Height of the world spawn point (`S05`), the reference of the explorers' flight height.
    pub fn set_world_spawn_y(&mut self, y: f64) {
        self.world_spawn_y = Some(y);
    }

    /// The bot was placed: first position, respawn or teleport. It becomes the new anchor.
    pub fn on_spawn(&mut self, at: Position) {
        self.anchor = Some(at);
        self.target = None;
        self.pause = 0;
        self.raised = 0.0;
        self.escape = None;
        self.hold = 0;
    }

    /// The server refused a move and put the bot back at `at`.
    pub fn on_setback(&mut self, at: Position) {
        self.target = None;
        self.streak = if self.calm >= CALM_TICKS {
            1
        } else {
            self.streak + 1
        };
        self.calm = 0;
        self.hold = (HOLD_TICKS << (self.streak - 1).min(6)).min(MAX_HOLD_TICKS);
        let raise = match self.settings.behaviour {
            Behaviour::Idle => return,
            Behaviour::Wander => WANDER_RAISE,
            Behaviour::Explore => EXPLORE_RAISE,
        };
        if at.y < self.level(at) - 1e-9 {
            // The climb hit something overhead: slide sideways before climbing again.
            // A new direction each time: the last one may lead into the same trunk.
            self.escape = Some((self.rng.next_f64() * TAU, ESCAPE_TICKS));
        } else {
            // Something stands at cruising height: go over it.
            self.raised += raise;
        }
    }

    /// Feet height of the horizontal legs.
    pub fn level(&self, current: Position) -> f64 {
        let base = match self.settings.behaviour {
            Behaviour::Idle => return current.y,
            Behaviour::Wander => {
                self.anchor.map_or(current.y, |a| a.y) + self.settings.wander_height
            }
            Behaviour::Explore => {
                self.world_spawn_y
                    .or(self.anchor.map(|a| a.y))
                    .unwrap_or(current.y)
                    + self.settings.fly_height
            }
        };
        (base + self.raised).min(MAX_FLY_Y)
    }

    fn on_ground(&self) -> bool {
        match self.settings.behaviour {
            Behaviour::Idle => true,
            Behaviour::Wander => self.settings.wander_height + self.raised == 0.0,
            Behaviour::Explore => false,
        }
    }

    pub fn step(&mut self, pos: Position) -> Step {
        self.calm = self.calm.saturating_add(1);
        if self.hold > 0 {
            self.hold -= 1;
            return Step::Stay {
                on_ground: self.on_ground(),
            };
        }
        match self.settings.behaviour {
            Behaviour::Idle => Step::Stay { on_ground: true },
            Behaviour::Wander => self.wander(pos),
            Behaviour::Explore => self.explore(pos),
        }
    }

    /// Sideways escape or vertical move towards the cruising height, both before any
    /// horizontal leg.
    fn climb(&mut self, pos: Position) -> Option<Step> {
        let on_ground = self.on_ground();
        if let Some((heading, ticks)) = self.escape {
            // At the height where the climb stopped: just under a tree crown, above the small
            // steps of the ground, with only trunks in the way.
            self.escape = (ticks > 1).then_some((heading, ticks - 1));
            let to = Position {
                x: pos.x + heading.cos() * WALK_STEP,
                z: pos.z + heading.sin() * WALK_STEP,
                ..pos
            };
            return Some(Step::Move { to, on_ground });
        }
        let dy = self.level(pos) - pos.y;
        (dy.abs() > 1e-9).then(|| Step::Move {
            to: Position {
                y: pos.y + dy.clamp(-CLIMB_STEP, CLIMB_STEP),
                ..pos
            },
            on_ground,
        })
    }

    fn wander(&mut self, pos: Position) -> Step {
        let anchor = *self.anchor.get_or_insert(pos);
        if let Some(step) = self.climb(pos) {
            return step;
        }
        let on_ground = self.on_ground();
        if self.pause > 0 {
            self.pause -= 1;
            return Step::Stay { on_ground };
        }
        let (tx, tz) = match self.target {
            Some(target) => target,
            None => {
                // sqrt gives a uniform density over the disc.
                let r = self.settings.wander_radius * self.rng.next_f64().sqrt();
                let a = self.rng.next_f64() * TAU;
                let target = (anchor.x + r * a.cos(), anchor.z + r * a.sin());
                self.target = Some(target);
                target
            }
        };
        let (dx, dz) = (tx - pos.x, tz - pos.z);
        let d = dx.hypot(dz);
        let to = if d <= WALK_STEP {
            self.target = None;
            self.pause = self.rng.next_u64() % (MAX_PAUSE_TICKS + 1);
            Position {
                x: tx,
                z: tz,
                ..pos
            }
        } else {
            Position {
                x: pos.x + dx / d * WALK_STEP,
                z: pos.z + dz / d * WALK_STEP,
                ..pos
            }
        };
        Step::Move { to, on_ground }
    }

    fn explore(&mut self, pos: Position) -> Step {
        if let Some(step) = self.climb(pos) {
            return step;
        }
        let to = Position {
            x: pos.x + self.heading.cos() * EXPLORE_STEP,
            z: pos.z + self.heading.sin() * EXPLORE_STEP,
            ..pos
        };
        Step::Move {
            to,
            on_ground: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPAWN: Position = Position {
        x: 100.5,
        y: 70.0,
        z: -20.5,
    };

    fn settings(behaviour: Behaviour) -> MoveSettings {
        MoveSettings {
            behaviour,
            ..MoveSettings::default()
        }
    }

    fn mover_with(settings: MoveSettings, index: u32) -> Mover {
        let mut mover = Mover::new(settings, index);
        mover.on_spawn(SPAWN);
        mover
    }

    fn run(mover: &mut Mover, mut pos: Position, ticks: usize) -> Vec<Position> {
        let mut path = Vec::new();
        for _ in 0..ticks {
            if let Step::Move { to, .. } = mover.step(pos) {
                assert!(
                    pos.distance(to) <= 1.0 + 1e-9,
                    "step too long: {pos:?} -> {to:?}"
                );
                pos = to;
            }
            path.push(pos);
        }
        path
    }

    #[test]
    fn idle_never_moves() {
        let mut idle = mover_with(settings(Behaviour::Idle), 1);
        assert_eq!(idle.step(SPAWN), Step::Stay { on_ground: true });
        idle.on_setback(SPAWN);
        assert_eq!(idle.step(SPAWN), Step::Stay { on_ground: true });
    }

    #[test]
    fn wander_stays_in_radius_at_constant_height() {
        let mut wanderer = mover_with(
            MoveSettings {
                seed: 7,
                ..settings(Behaviour::Wander)
            },
            3,
        );
        let path = run(&mut wanderer, SPAWN, 20 * 600);
        let mut moved = 0.0;
        let mut prev = SPAWN;
        for p in &path {
            assert_eq!(p.y, SPAWN.y);
            assert!((p.x - SPAWN.x).hypot(p.z - SPAWN.z) <= 16.0 + 1e-9);
            assert!(prev.distance(*p) <= WALK_STEP + 1e-9);
            moved += prev.distance(*p);
            prev = *p;
        }
        // Walking most of the time, pausing sometimes.
        assert!(moved > 4.317 * 600.0 * 0.5, "moved only {moved}");
        assert!(matches!(
            wanderer.step(path[path.len() - 1]),
            Step::Move {
                on_ground: true,
                ..
            } | Step::Stay { on_ground: true }
        ));
    }

    #[test]
    fn wander_above_spawn_climbs_first() {
        let mut wanderer = mover_with(
            MoveSettings {
                wander_height: 3.5,
                ..settings(Behaviour::Wander)
            },
            1,
        );
        let path = run(&mut wanderer, SPAWN, 200);
        assert_eq!(path[0], Position { y: 71.0, ..SPAWN });
        assert_eq!(path[3], Position { y: 73.5, ..SPAWN });
        assert!(path[4..].iter().all(|p| p.y == 73.5));
        assert!(path[4..].iter().any(|p| p.x != SPAWN.x));
        assert!(matches!(
            wanderer.step(path[199]),
            Step::Move {
                on_ground: false,
                ..
            } | Step::Stay { on_ground: false }
        ));
    }

    #[test]
    fn wander_is_reproducible() {
        let wander = |index| {
            run(
                &mut mover_with(
                    MoveSettings {
                        seed: 42,
                        ..settings(Behaviour::Wander)
                    },
                    index,
                ),
                SPAWN,
                500,
            )
        };
        assert_eq!(wander(5), wander(5));
        assert_ne!(wander(5), wander(6));
    }

    #[test]
    fn setback_at_cruising_height_raises_and_holds() {
        let mut wanderer = mover_with(settings(Behaviour::Wander), 1);
        let pos = Position { x: 103.0, ..SPAWN };
        wanderer.on_setback(pos);
        for _ in 0..HOLD_TICKS {
            assert_eq!(wanderer.step(pos), Step::Stay { on_ground: false });
        }
        let path = run(&mut wanderer, pos, 3);
        assert_eq!(path[0], Position { y: 71.0, ..pos });
        assert_eq!(path[1], Position { y: 72.0, ..pos });
        assert_eq!(path[2].y, 72.0);
        assert_ne!(path[2].x, pos.x);
    }

    #[test]
    fn repeated_setbacks_back_off() {
        let mut wanderer = mover_with(settings(Behaviour::Wander), 1);
        let mut holds = Vec::new();
        for _ in 0..9 {
            wanderer.on_setback(SPAWN);
            holds.push(wanderer.hold);
        }
        assert_eq!(holds, vec![10, 20, 40, 80, 160, 320, 600, 600, 600]);
        for _ in 0..(600 + CALM_TICKS) {
            wanderer.step(SPAWN);
        }
        wanderer.on_setback(SPAWN);
        assert_eq!(
            wanderer.hold, HOLD_TICKS,
            "a calm period resets the backoff"
        );
    }

    #[test]
    fn blocked_climb_escapes_sideways_at_that_height() {
        let mut wanderer = mover_with(
            MoveSettings {
                wander_height: 8.0,
                ..settings(Behaviour::Wander)
            },
            4,
        );
        let blocked = Position {
            y: SPAWN.y + 2.0,
            ..SPAWN
        };
        wanderer.on_setback(blocked);
        let hold = HOLD_TICKS as usize;
        let escape = ESCAPE_TICKS as usize;
        let path = run(&mut wanderer, blocked, hold + escape + 1);
        assert!(
            path[..hold].iter().all(|p| *p == blocked),
            "holds still first"
        );
        let escaped = path[hold + escape - 1];
        assert!(path[hold..hold + escape].iter().all(|p| p.y == blocked.y));
        let walked = (escaped.x - blocked.x).hypot(escaped.z - blocked.z);
        assert!((walked - f64::from(ESCAPE_TICKS) * WALK_STEP).abs() < 1e-9);
        assert_eq!(
            path[hold + escape],
            Position {
                y: blocked.y + 1.0,
                ..escaped
            },
            "then climbs again"
        );
        assert_eq!(
            wanderer.level(SPAWN),
            SPAWN.y + 8.0,
            "no raise for a blocked climb"
        );
    }

    #[test]
    fn each_escape_takes_a_new_direction() {
        let mut explorer = mover_with(settings(Behaviour::Explore), 2);
        explorer.set_world_spawn_y(64.0);
        let mut headings = Vec::new();
        for _ in 0..3 {
            explorer.on_setback(SPAWN);
            headings.push(explorer.escape.map(|(h, _)| h).unwrap());
        }
        assert!(headings[0] != headings[1] && headings[1] != headings[2]);
    }

    #[test]
    fn explore_climbs_then_flies_straight() {
        let mut explorer = mover_with(settings(Behaviour::Explore), 2);
        explorer.set_world_spawn_y(64.0);
        let cruise = 164.0;
        let path = run(&mut explorer, SPAWN, 20 * 60);
        let top = path.iter().position(|p| p.y == cruise).unwrap();
        assert_eq!(top, 93, "94 one-block climbs from 70 to 164");
        for p in &path[..=top] {
            assert_eq!((p.x, p.z), (SPAWN.x, SPAWN.z));
        }
        let last = path.last().unwrap();
        let flown = (last.x - SPAWN.x).hypot(last.z - SPAWN.z);
        let expected = EXPLORE_STEP * (path.len() - top - 1) as f64;
        assert!(
            (flown - expected).abs() < 1e-6,
            "flew {flown}, expected {expected}"
        );
        assert!(path[top..].iter().all(|p| p.y == cruise));
    }

    #[test]
    fn heights_are_capped() {
        let explorer = mover_with(
            MoveSettings {
                fly_height: 500.0,
                ..settings(Behaviour::Explore)
            },
            1,
        );
        assert_eq!(explorer.level(SPAWN), MAX_FLY_Y);
    }

    #[test]
    fn explorers_spread_out() {
        let mut headings: Vec<f64> = (1..=50)
            .map(|i| mover_with(settings(Behaviour::Explore), i).heading())
            .collect();
        headings.sort_by(f64::total_cmp);
        let wrap = headings[0] + TAU - headings[49];
        let widest_gap = headings
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(wrap, f64::max);
        assert!(widest_gap < TAU / 50.0 * 3.0, "gap {widest_gap}");
    }

    #[test]
    fn clamp_step_limits_distance() {
        let from = Position {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        };
        let far = Position {
            x: 30.0,
            y: 64.0,
            z: 40.0,
        };
        let clamped = from.clamp_step(far, MAX_STEP);
        assert!((from.distance(clamped) - MAX_STEP).abs() < 1e-9);
        assert_eq!(from.clamp_step(from, MAX_STEP), from);
        let nan = Position {
            x: f64::NAN,
            y: 0.0,
            z: 0.0,
        };
        assert_eq!(from.clamp_step(nan, MAX_STEP), from);
    }
}
