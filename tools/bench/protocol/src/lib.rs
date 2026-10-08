//! Wire format of Minecraft 1.7.10 (protocol 5) as spoken by a Forge 10.13.4 server.
//!
//! The crate only knows bytes: it has no sockets, threads or timers, so the same code serves a
//! load bot, a proxy or a capture decoder.
//!
//! * [`varint`]: VarInt and VarLong.
//! * [`buf`]: big-endian primitives and length-prefixed strings over a byte slice.
//! * [`frame`]: `VarInt length | VarInt id | body` framing, with an incremental decoder that can
//!   drop the bodies it is told to skip without buffering them.
//! * [`ids`]: packet identifiers per state and direction.
//! * [`packets`]: typed packets for the parts of the protocol a client must understand.
//! * [`fml`]: the Forge `FML|HS` handshake channel and its client state machine.
//! * [`json`] and [`status`]: the server list response (`modinfo.modList`) and chat components.

pub mod buf;
pub mod error;
pub mod fml;
pub mod frame;
pub mod ids;
pub mod json;
pub mod packets;
pub mod status;
pub mod varint;

pub use error::{Error, Result};
