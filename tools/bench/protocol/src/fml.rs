//! Forge's `FML|HS` handshake (FML protocol 2, Forge 10.13.4).
//!
//! The handshake runs in the play state inside custom payload packets: `S3F` from the server,
//! whose length Forge turned into a VarShort, and `C17` from the client, whose length is still a
//! plain short. The first payload byte is a discriminator. Inside the messages, integers and
//! strings use Forge's own size-limited VarInt (`ByteBufUtils`).

use crate::buf::{Reader, Writer};
use crate::varint;
use crate::{Error, Result};

pub const HANDSHAKE_CHANNEL: &str = "FML|HS";
pub const REGISTER_CHANNEL: &str = "REGISTER";
pub const UNREGISTER_CHANNEL: &str = "UNREGISTER";
/// Longest channel name custom payload packets accept.
pub const MAX_CHANNEL_LEN: usize = 20;
/// `NetworkRegistry.FML_PROTOCOL` of Forge 10.13.4.
pub const FML_PROTOCOL_VERSION: i8 = 2;
/// Largest length a VarShort can carry: 15 bits plus one extra byte.
pub const MAX_VARSHORT: usize = 0x7f_ffff;
/// Forge strings have a length VarInt of at most two bytes.
pub const MAX_FORGE_STRING_BYTES: usize = 0x3fff;

pub const SERVER_HELLO: u8 = 0x00;
pub const CLIENT_HELLO: u8 = 0x01;
pub const MOD_LIST: u8 = 0x02;
pub const MOD_ID_DATA: u8 = 0x03;
pub const HANDSHAKE_ACK: u8 = 0xff;
pub const HANDSHAKE_RESET: u8 = 0xfe;

/// `ByteBufUtils.readVarShort`: an unsigned short, plus a byte for bits 15 to 22 when the high
/// bit of the short is set.
pub fn read_varshort(r: &mut Reader<'_>) -> Result<usize> {
    let low = usize::from(r.read_u16()?);
    if low & 0x8000 == 0 {
        return Ok(low);
    }
    let high = usize::from(r.read_u8()?);
    Ok((high << 15) | (low & 0x7fff))
}

pub fn write_varshort(w: &mut Writer, value: usize) -> Result<()> {
    if value > MAX_VARSHORT {
        return Err(Error::PayloadTooLarge {
            len: value,
            max: MAX_VARSHORT,
        });
    }
    let low = (value & 0x7fff) as u16;
    let high = (value >> 15) as u8;
    if high == 0 {
        w.write_u16(low);
    } else {
        w.write_u16(low | 0x8000);
        w.write_u8(high);
    }
    Ok(())
}

/// `ByteBufUtils.writeVarInt(buf, value, max_bytes)`: a VarInt that must fit `max_bytes`.
pub fn write_forge_varint(w: &mut Writer, value: i32, max_bytes: usize) -> Result<()> {
    if varint::varint_len(value) > max_bytes {
        return Err(Error::VarIntTooLong);
    }
    w.write_varint(value);
    Ok(())
}

/// `ByteBufUtils.readUTF8String`: a two-byte-at-most VarInt length, then UTF-8.
pub fn read_forge_string(r: &mut Reader<'_>) -> Result<String> {
    let len = r.read_varint_max(2)?;
    if len < 0 {
        return Err(Error::NegativeLength(i64::from(len)));
    }
    let bytes = r.read_bytes(len as usize)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
}

pub fn write_forge_string(w: &mut Writer, text: &str) -> Result<()> {
    if text.len() > MAX_FORGE_STRING_BYTES {
        return Err(Error::StringTooLong {
            max: MAX_FORGE_STRING_BYTES,
            len: text.len(),
        });
    }
    w.write_varint(text.len() as i32);
    w.write_bytes(text.as_bytes());
    Ok(())
}

/// One entry of a mod list, as the status response and the `ModList` message carry it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModEntry {
    pub modid: String,
    pub version: String,
}

impl ModEntry {
    pub fn new(modid: impl Into<String>, version: impl Into<String>) -> Self {
        ModEntry {
            modid: modid.into(),
            version: version.into(),
        }
    }
}

/// A message of the `FML|HS` channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeMessage {
    /// Server, first message. Protocol 2 adds the dimension the player will log into.
    ServerHello {
        protocol_version: i8,
        override_dimension: Option<i32>,
    },
    ClientHello {
        protocol_version: i8,
    },
    /// Sent by both sides. The server rejects a client list that lacks one of its mods or
    /// carries another version of it.
    ModList(Vec<ModEntry>),
    /// Server block and item id table, kept undecoded: a client without the game registry has
    /// no use for it, and a relay forwards it as it is.
    ModIdData(Vec<u8>),
    /// `phase` is the ordinal of the sender's handshake state; receivers do not read it.
    HandshakeAck {
        phase: i8,
    },
    HandshakeReset,
}

impl HandshakeMessage {
    pub fn discriminator(&self) -> u8 {
        match self {
            HandshakeMessage::ServerHello { .. } => SERVER_HELLO,
            HandshakeMessage::ClientHello { .. } => CLIENT_HELLO,
            HandshakeMessage::ModList(_) => MOD_LIST,
            HandshakeMessage::ModIdData(_) => MOD_ID_DATA,
            HandshakeMessage::HandshakeAck { .. } => HANDSHAKE_ACK,
            HandshakeMessage::HandshakeReset => HANDSHAKE_RESET,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            HandshakeMessage::ServerHello { .. } => "ServerHello",
            HandshakeMessage::ClientHello { .. } => "ClientHello",
            HandshakeMessage::ModList(_) => "ModList",
            HandshakeMessage::ModIdData(_) => "ModIdData",
            HandshakeMessage::HandshakeAck { .. } => "HandshakeAck",
            HandshakeMessage::HandshakeReset => "HandshakeReset",
        }
    }

    /// Decodes the data of an `FML|HS` custom payload. Like FML, ignores bytes after the fields.
    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let discriminator = r.read_u8()?;
        Ok(match discriminator {
            SERVER_HELLO => {
                let protocol_version = r.read_i8()?;
                let override_dimension = if protocol_version > 1 {
                    Some(r.read_i32()?)
                } else {
                    None
                };
                HandshakeMessage::ServerHello {
                    protocol_version,
                    override_dimension,
                }
            }
            CLIENT_HELLO => HandshakeMessage::ClientHello {
                protocol_version: r.read_i8()?,
            },
            MOD_LIST => {
                let count = r.read_varint_max(2)?;
                if count < 0 {
                    return Err(Error::NegativeLength(i64::from(count)));
                }
                let mut mods = Vec::with_capacity((count as usize).min(r.remaining()));
                for _ in 0..count {
                    let modid = read_forge_string(&mut r)?;
                    let version = read_forge_string(&mut r)?;
                    mods.push(ModEntry { modid, version });
                }
                HandshakeMessage::ModList(mods)
            }
            MOD_ID_DATA => HandshakeMessage::ModIdData(r.read_rest().to_vec()),
            HANDSHAKE_ACK => HandshakeMessage::HandshakeAck {
                phase: r.read_i8()?,
            },
            HANDSHAKE_RESET => HandshakeMessage::HandshakeReset,
            other => return Err(Error::UnknownDiscriminator(other)),
        })
    }

    /// The data of the `FML|HS` custom payload carrying this message.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut w = Writer::new();
        w.write_u8(self.discriminator());
        match self {
            HandshakeMessage::ServerHello {
                protocol_version,
                override_dimension,
            } => {
                w.write_i8(*protocol_version);
                if *protocol_version > 1 {
                    w.write_i32(override_dimension.unwrap_or(0));
                }
            }
            HandshakeMessage::ClientHello { protocol_version } => w.write_i8(*protocol_version),
            HandshakeMessage::ModList(mods) => {
                let count = i32::try_from(mods.len()).map_err(|_| Error::VarIntTooLong)?;
                write_forge_varint(&mut w, count, 2)?;
                for entry in mods {
                    write_forge_string(&mut w, &entry.modid)?;
                    write_forge_string(&mut w, &entry.version)?;
                }
            }
            HandshakeMessage::ModIdData(raw) => w.write_bytes(raw),
            HandshakeMessage::HandshakeAck { phase } => w.write_i8(*phase),
            HandshakeMessage::HandshakeReset => {}
        }
        Ok(w.into_inner())
    }
}

/// Data of a `REGISTER` (or `UNREGISTER`) payload: channel names separated by NUL bytes.
pub fn register_payload<'a>(channels: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    channels
        .into_iter()
        .collect::<Vec<_>>()
        .join("\0")
        .into_bytes()
}

/// Channel names of a `REGISTER` payload.
pub fn parse_register(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// States of `FMLHandshakeClientState` after `START`. The ordinal of each is the phase of the
/// acknowledgement the client sends from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientPhase {
    Hello = 1,
    WaitingServerData = 2,
    WaitingServerComplete = 3,
    PendingComplete = 4,
    Complete = 5,
    Done = 6,
}

/// A custom payload the client has to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub channel: &'static str,
    pub data: Vec<u8>,
}

/// Client side of the handshake, replaying what a Forge 1.7.10 client sends:
///
/// | Server sends | Client answers |
/// |---|---|
/// | ServerHello | `REGISTER`, ClientHello, ModList |
/// | ModList | Ack(2) |
/// | ModIdData | Ack(3) |
/// | Ack(2) | Ack(4) |
/// | Ack(3) | Ack(5) |
///
/// The server only needs the ModList and the next two messages; it joins the player when the
/// third arrives and ignores the rest.
#[derive(Debug, Clone)]
pub struct ClientHandshake {
    mods: Vec<ModEntry>,
    channels: Vec<String>,
    phase: ClientPhase,
    override_dimension: Option<i32>,
}

impl ClientHandshake {
    /// `mods` must equal the server's own list (`modinfo.modList` of its status response).
    pub fn new(mods: Vec<ModEntry>) -> Self {
        ClientHandshake {
            mods,
            channels: Vec::new(),
            phase: ClientPhase::Hello,
            override_dimension: None,
        }
    }

    /// Extra channels to register after `FML|HS` and `FML`, as client mods would.
    pub fn with_channels(mut self, channels: Vec<String>) -> Self {
        self.channels = channels;
        self
    }

    /// Replaces the extra channels; they are sent with the answer to the server hello.
    pub fn set_channels(&mut self, channels: Vec<String>) {
        self.channels = channels;
    }

    pub fn phase(&self) -> ClientPhase {
        self.phase
    }

    pub fn is_done(&self) -> bool {
        self.phase == ClientPhase::Done
    }

    /// Dimension announced by the server hello.
    pub fn override_dimension(&self) -> Option<i32> {
        self.override_dimension
    }

    fn ack(&self) -> Result<Outgoing> {
        let message = HandshakeMessage::HandshakeAck {
            phase: self.phase as i8,
        };
        Ok(Outgoing {
            channel: HANDSHAKE_CHANNEL,
            data: message.encode()?,
        })
    }

    /// Advances on one message from the server and returns the payloads to send, in order.
    pub fn handle(&mut self, message: &HandshakeMessage) -> Result<Vec<Outgoing>> {
        use ClientPhase::*;
        use HandshakeMessage as M;
        let out = match (self.phase, message) {
            (
                Hello,
                M::ServerHello {
                    override_dimension, ..
                },
            ) => {
                self.override_dimension = *override_dimension;
                let channels = [HANDSHAKE_CHANNEL, "FML"]
                    .into_iter()
                    .chain(self.channels.iter().map(String::as_str));
                let hello = M::ClientHello {
                    protocol_version: FML_PROTOCOL_VERSION,
                };
                let mods = M::ModList(self.mods.clone());
                self.phase = WaitingServerData;
                vec![
                    Outgoing {
                        channel: REGISTER_CHANNEL,
                        data: register_payload(channels),
                    },
                    Outgoing {
                        channel: HANDSHAKE_CHANNEL,
                        data: hello.encode()?,
                    },
                    Outgoing {
                        channel: HANDSHAKE_CHANNEL,
                        data: mods.encode()?,
                    },
                ]
            }
            (WaitingServerData, M::ModList(_)) | (WaitingServerComplete, M::ModIdData(_)) => {
                let ack = self.ack()?;
                self.phase = if self.phase == WaitingServerData {
                    WaitingServerComplete
                } else {
                    PendingComplete
                };
                vec![ack]
            }
            (PendingComplete, _) => {
                let ack = self.ack()?;
                self.phase = Complete;
                vec![ack]
            }
            (Complete, _) => {
                let ack = self.ack()?;
                self.phase = Done;
                vec![ack]
            }
            (Done, M::HandshakeReset) => {
                self.phase = Hello;
                Vec::new()
            }
            (Done, _) => Vec::new(),
            (_, other) => return Err(Error::UnexpectedMessage(other.name())),
        };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(out: &[Outgoing]) -> Vec<(&'static str, Option<HandshakeMessage>)> {
        out.iter()
            .map(|o| {
                let message = (o.channel == HANDSHAKE_CHANNEL)
                    .then(|| HandshakeMessage::decode(&o.data).unwrap());
                (o.channel, message)
            })
            .collect()
    }

    #[test]
    fn varshort() {
        for (value, bytes) in [
            (0usize, vec![0x00, 0x00]),
            (0x7fff, vec![0x7f, 0xff]),
            (0x8000, vec![0x80, 0x00, 0x01]),
            (0x12345, vec![0xa3, 0x45, 0x02]),
            (MAX_VARSHORT, vec![0xff, 0xff, 0xff]),
        ] {
            let mut w = Writer::new();
            write_varshort(&mut w, value).unwrap();
            assert_eq!(w.as_slice(), &bytes[..], "encoding {value:#x}");
            assert_eq!(read_varshort(&mut Reader::new(&bytes)), Ok(value));
        }
        assert!(write_varshort(&mut Writer::new(), MAX_VARSHORT + 1).is_err());
    }

    #[test]
    fn forge_strings() {
        let mut w = Writer::new();
        write_forge_string(&mut w, "Forge").unwrap();
        assert_eq!(w.as_slice(), &[5, b'F', b'o', b'r', b'g', b'e']);
        assert_eq!(
            read_forge_string(&mut Reader::new(w.as_slice())).as_deref(),
            Ok("Forge")
        );
        assert!(write_forge_string(&mut Writer::new(), &"x".repeat(0x4000)).is_err());
        // A length on three bytes is refused, as by ByteBufUtils.readVarInt(buf, 2).
        assert_eq!(
            read_forge_string(&mut Reader::new(&[0x80, 0x80, 0x01])),
            Err(Error::VarIntTooLong)
        );
        assert!(write_forge_varint(&mut Writer::new(), 0x4000, 2).is_err());
    }

    #[test]
    fn known_message_bytes() {
        let hello = HandshakeMessage::decode(&[0x00, 0x02, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(
            hello,
            HandshakeMessage::ServerHello {
                protocol_version: 2,
                override_dimension: Some(0)
            }
        );
        // Protocol 1 servers send no dimension.
        assert_eq!(
            HandshakeMessage::decode(&[0x00, 0x01]).unwrap(),
            HandshakeMessage::ServerHello {
                protocol_version: 1,
                override_dimension: None
            }
        );
        assert_eq!(
            HandshakeMessage::ClientHello {
                protocol_version: 2
            }
            .encode()
            .unwrap(),
            vec![0x01, 0x02]
        );
        assert_eq!(
            HandshakeMessage::HandshakeAck { phase: 2 }
                .encode()
                .unwrap(),
            vec![0xff, 0x02]
        );
        assert_eq!(
            HandshakeMessage::decode(&[0xff, 0x03]).unwrap(),
            HandshakeMessage::HandshakeAck { phase: 3 }
        );
        assert_eq!(
            HandshakeMessage::decode(&[0xfe]).unwrap(),
            HandshakeMessage::HandshakeReset
        );
        assert_eq!(
            HandshakeMessage::decode(&[0x07]),
            Err(Error::UnknownDiscriminator(7))
        );
        assert_eq!(HandshakeMessage::decode(&[]), Err(Error::UnexpectedEof));
    }

    #[test]
    fn mod_list_round_trip() {
        let mods = vec![
            ModEntry::new("mcp", "9.05"),
            ModEntry::new("FML", "7.10.99.99"),
            ModEntry::new("Forge", "10.13.4.1614"),
        ];
        let message = HandshakeMessage::ModList(mods.clone());
        let bytes = message.encode().unwrap();
        assert_eq!(&bytes[..6], &[0x02, 0x03, 0x03, b'm', b'c', b'p']);
        assert_eq!(HandshakeMessage::decode(&bytes).unwrap(), message);

        let empty = HandshakeMessage::ModList(Vec::new()).encode().unwrap();
        assert_eq!(empty, vec![0x02, 0x00]);
        assert_eq!(
            HandshakeMessage::decode(&empty).unwrap(),
            HandshakeMessage::ModList(vec![])
        );
    }

    #[test]
    fn mod_id_data_is_kept_raw() {
        let bytes = [
            0x03, 0x01, 0x05, b'a', b':', b'b', b'c', b'd', 0x10, 0x00, 0x00,
        ];
        let message = HandshakeMessage::decode(&bytes).unwrap();
        assert_eq!(message, HandshakeMessage::ModIdData(bytes[1..].to_vec()));
        assert_eq!(message.encode().unwrap(), bytes);
    }

    #[test]
    fn register_payloads() {
        assert_eq!(register_payload(["FML|HS", "FML"]), b"FML|HS\0FML".to_vec());
        assert_eq!(
            parse_register(b"FML|HS\0FML\0\0FORGE"),
            vec!["FML|HS", "FML", "FORGE"]
        );
        assert!(parse_register(b"").is_empty());
    }

    #[test]
    fn client_handshake_replays_a_forge_client() {
        let mods = vec![
            ModEntry::new("mcp", "9.05"),
            ModEntry::new("Forge", "10.13.4.1614"),
        ];
        let mut client = ClientHandshake::new(mods.clone());
        assert_eq!(client.phase(), ClientPhase::Hello);

        let out = client
            .handle(&HandshakeMessage::ServerHello {
                protocol_version: 2,
                override_dimension: Some(-1),
            })
            .unwrap();
        assert_eq!(client.override_dimension(), Some(-1));
        assert_eq!(
            out[0],
            Outgoing {
                channel: REGISTER_CHANNEL,
                data: b"FML|HS\0FML".to_vec()
            }
        );
        assert_eq!(
            decode_all(&out[1..]),
            vec![
                (
                    HANDSHAKE_CHANNEL,
                    Some(HandshakeMessage::ClientHello {
                        protocol_version: 2
                    })
                ),
                (HANDSHAKE_CHANNEL, Some(HandshakeMessage::ModList(mods))),
            ]
        );

        let steps = [
            (
                HandshakeMessage::ModList(vec![]),
                2,
                ClientPhase::WaitingServerComplete,
            ),
            (
                HandshakeMessage::ModIdData(vec![0]),
                3,
                ClientPhase::PendingComplete,
            ),
            (
                HandshakeMessage::HandshakeAck { phase: 2 },
                4,
                ClientPhase::Complete,
            ),
            (
                HandshakeMessage::HandshakeAck { phase: 3 },
                5,
                ClientPhase::Done,
            ),
        ];
        for (incoming, phase, next) in steps {
            let out = client.handle(&incoming).unwrap();
            assert_eq!(
                decode_all(&out),
                vec![(
                    HANDSHAKE_CHANNEL,
                    Some(HandshakeMessage::HandshakeAck { phase })
                )]
            );
            assert_eq!(client.phase(), next);
        }
        assert!(client.is_done());
        assert!(client
            .handle(&HandshakeMessage::HandshakeAck { phase: 3 })
            .unwrap()
            .is_empty());
        assert!(client
            .handle(&HandshakeMessage::HandshakeReset)
            .unwrap()
            .is_empty());
        assert_eq!(client.phase(), ClientPhase::Hello);
    }

    #[test]
    fn client_handshake_rejects_out_of_order_messages() {
        let mut client = ClientHandshake::new(Vec::new());
        assert_eq!(
            client.handle(&HandshakeMessage::ModIdData(vec![])),
            Err(Error::UnexpectedMessage("ModIdData"))
        );
        let mut client = ClientHandshake::new(Vec::new()).with_channels(vec!["MyMod".into()]);
        let out = client
            .handle(&HandshakeMessage::ServerHello {
                protocol_version: 2,
                override_dimension: Some(0),
            })
            .unwrap();
        assert_eq!(out[0].data, b"FML|HS\0FML\0MyMod".to_vec());
        assert_eq!(
            client.handle(&HandshakeMessage::HandshakeAck { phase: 2 }),
            Err(Error::UnexpectedMessage("HandshakeAck"))
        );
    }
}
