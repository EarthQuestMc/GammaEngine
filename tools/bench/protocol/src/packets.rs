//! Typed packets: the ones a client has to read or write to log in, pass the Forge handshake,
//! stay connected and move. Everything else can be skipped by its frame length.
//!
//! Field order and limits follow the `readPacketData` methods of the 1.7.10 server, including
//! Forge's change to `S3FPacketCustomPayload` (VarShort length).

use crate::buf::{Reader, Writer};
use crate::ids::{self, Direction, State};
use crate::{Error, Result};

/// A packet with a fixed state, direction and id.
pub trait Packet: Sized {
    const STATE: State;
    const DIRECTION: Direction;
    const ID: i32;

    fn write(&self, w: &mut Writer) -> Result<()>;
    fn read(r: &mut Reader<'_>) -> Result<Self>;

    fn encode_body(&self) -> Result<Vec<u8>> {
        let mut w = Writer::new();
        self.write(&mut w)?;
        Ok(w.into_inner())
    }

    /// Decodes a whole body. Unread bytes are an error, as in the game's own decoder.
    fn decode_body(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let packet = Self::read(&mut r)?;
        r.finish()?;
        Ok(packet)
    }
}

macro_rules! packet_meta {
    ($state:ident, $dir:ident, $id:expr) => {
        const STATE: State = State::$state;
        const DIRECTION: Direction = Direction::$dir;
        const ID: i32 = $id;
    };
}

/// A byte array prefixed by a signed short, as `Packet.readBlob` reads it.
fn read_short_blob(r: &mut Reader<'_>) -> Result<Vec<u8>> {
    let len = r.read_i16()?;
    if len < 0 {
        return Err(Error::NegativeLength(i64::from(len)));
    }
    Ok(r.read_bytes(len as usize)?.to_vec())
}

fn write_short_blob(w: &mut Writer, bytes: &[u8]) -> Result<()> {
    let len = i16::try_from(bytes.len()).map_err(|_| Error::PayloadTooLarge {
        len: bytes.len(),
        max: i16::MAX as usize,
    })?;
    w.write_i16(len);
    w.write_bytes(bytes);
    Ok(())
}

pub mod handshake {
    use super::*;

    /// `next_state` value that opens the status exchange.
    pub const NEXT_STATUS: i32 = 1;
    /// `next_state` value that opens the login.
    pub const NEXT_LOGIN: i32 = 2;

    /// `C00Handshake`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Handshake {
        pub protocol_version: i32,
        pub server_address: String,
        pub server_port: u16,
        pub next_state: i32,
    }

    impl Packet for Handshake {
        packet_meta!(
            Handshaking,
            Serverbound,
            ids::handshaking::serverbound::HANDSHAKE
        );

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_varint(self.protocol_version);
            w.write_string(&self.server_address)?;
            w.write_u16(self.server_port);
            w.write_varint(self.next_state);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Handshake {
                protocol_version: r.read_varint()?,
                // Spigot raised the limit from 255 for BungeeCord forwarding.
                server_address: r.read_string(i16::MAX as usize)?,
                server_port: r.read_u16()?,
                next_state: r.read_varint()?,
            })
        }
    }
}

pub mod status {
    use super::*;

    /// `C00PacketServerQuery`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ServerQuery;

    impl Packet for ServerQuery {
        packet_meta!(Status, Serverbound, ids::status::serverbound::SERVER_QUERY);

        fn write(&self, _: &mut Writer) -> Result<()> {
            Ok(())
        }

        fn read(_: &mut Reader<'_>) -> Result<Self> {
            Ok(ServerQuery)
        }
    }

    /// `S00PacketServerInfo`: the status JSON, see [`crate::status`].
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ServerInfo {
        pub json: String,
    }

    impl Packet for ServerInfo {
        packet_meta!(Status, Clientbound, ids::status::clientbound::SERVER_INFO);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.json)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(ServerInfo {
                json: r.read_string(32767)?,
            })
        }
    }

    /// `C01PacketPing`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Ping {
        pub payload: i64,
    }

    impl Packet for Ping {
        packet_meta!(Status, Serverbound, ids::status::serverbound::PING);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i64(self.payload);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Ping {
                payload: r.read_i64()?,
            })
        }
    }

    /// `S01PacketPong`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Pong {
        pub payload: i64,
    }

    impl Packet for Pong {
        packet_meta!(Status, Clientbound, ids::status::clientbound::PONG);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i64(self.payload);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Pong {
                payload: r.read_i64()?,
            })
        }
    }
}

pub mod login {
    use super::*;

    /// Longest player name the server accepts in `C00PacketLoginStart`.
    pub const MAX_NAME_LEN: usize = 16;

    /// `C00PacketLoginStart`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LoginStart {
        pub name: String,
    }

    impl Packet for LoginStart {
        packet_meta!(Login, Serverbound, ids::login::serverbound::LOGIN_START);

        fn write(&self, w: &mut Writer) -> Result<()> {
            let len = self.name.encode_utf16().count();
            if len > MAX_NAME_LEN {
                return Err(Error::StringTooLong {
                    max: MAX_NAME_LEN,
                    len,
                });
            }
            w.write_string(&self.name)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(LoginStart {
                name: r.read_string(MAX_NAME_LEN)?,
            })
        }
    }

    /// `S00PacketDisconnect` of the login state: the reason is a JSON chat component.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Disconnect {
        pub reason: String,
    }

    impl Packet for Disconnect {
        packet_meta!(Login, Clientbound, ids::login::clientbound::DISCONNECT);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.reason)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Disconnect {
                reason: r.read_string(32767)?,
            })
        }
    }

    /// `S01PacketEncryptionRequest`, only sent in online mode.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EncryptionRequest {
        pub server_id: String,
        pub public_key: Vec<u8>,
        pub verify_token: Vec<u8>,
    }

    impl Packet for EncryptionRequest {
        packet_meta!(
            Login,
            Clientbound,
            ids::login::clientbound::ENCRYPTION_REQUEST
        );

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.server_id)?;
            write_short_blob(w, &self.public_key)?;
            write_short_blob(w, &self.verify_token)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(EncryptionRequest {
                server_id: r.read_string(20)?,
                public_key: read_short_blob(r)?,
                verify_token: read_short_blob(r)?,
            })
        }
    }

    /// `S02PacketLoginSuccess`. The connection is in the play state right after it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LoginSuccess {
        /// With dashes, as `UUID.toString()` writes it.
        pub uuid: String,
        pub name: String,
    }

    impl Packet for LoginSuccess {
        packet_meta!(Login, Clientbound, ids::login::clientbound::LOGIN_SUCCESS);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.uuid)?;
            w.write_string(&self.name)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(LoginSuccess {
                uuid: r.read_string(36)?,
                name: r.read_string(16)?,
            })
        }
    }
}

/// Server to client packets of the play state.
pub mod play_clientbound {
    use super::*;
    use crate::ids::play::clientbound as id;

    /// `S00PacketKeepAlive`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeepAlive {
        pub id: i32,
    }

    impl Packet for KeepAlive {
        packet_meta!(Play, Clientbound, id::KEEP_ALIVE);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i32(self.id);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(KeepAlive { id: r.read_i32()? })
        }
    }

    /// `S01PacketJoinGame`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct JoinGame {
        pub entity_id: i32,
        /// 0 survival, 1 creative, 2 adventure.
        pub game_mode: u8,
        pub hardcore: bool,
        pub dimension: i8,
        pub difficulty: u8,
        pub max_players: u8,
        pub level_type: String,
    }

    impl Packet for JoinGame {
        packet_meta!(Play, Clientbound, id::JOIN_GAME);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i32(self.entity_id);
            w.write_u8(self.game_mode | if self.hardcore { 0x08 } else { 0 });
            w.write_i8(self.dimension);
            w.write_u8(self.difficulty);
            w.write_u8(self.max_players);
            w.write_string(&self.level_type)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            let entity_id = r.read_i32()?;
            let mode = r.read_u8()?;
            Ok(JoinGame {
                entity_id,
                game_mode: mode & !0x08,
                hardcore: mode & 0x08 != 0,
                dimension: r.read_i8()?,
                difficulty: r.read_u8()?,
                max_players: r.read_u8()?,
                level_type: r.read_string(16)?,
            })
        }
    }

    /// `S05PacketSpawnPosition`: the world spawn point, in block coordinates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SpawnPosition {
        pub x: i32,
        pub y: i32,
        pub z: i32,
    }

    impl Packet for SpawnPosition {
        packet_meta!(Play, Clientbound, id::SPAWN_POSITION);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i32(self.x);
            w.write_i32(self.y);
            w.write_i32(self.z);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(SpawnPosition {
                x: r.read_i32()?,
                y: r.read_i32()?,
                z: r.read_i32()?,
            })
        }
    }

    /// `S06PacketUpdateHealth`.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct UpdateHealth {
        pub health: f32,
        pub food: i16,
        pub saturation: f32,
    }

    impl Packet for UpdateHealth {
        packet_meta!(Play, Clientbound, id::UPDATE_HEALTH);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_f32(self.health);
            w.write_i16(self.food);
            w.write_f32(self.saturation);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(UpdateHealth {
                health: r.read_f32()?,
                food: r.read_i16()?,
                saturation: r.read_f32()?,
            })
        }
    }

    /// `S07PacketRespawn`. A new [`PlayerPosLook`] follows.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Respawn {
        pub dimension: i32,
        pub difficulty: u8,
        pub game_mode: u8,
        pub level_type: String,
    }

    impl Packet for Respawn {
        packet_meta!(Play, Clientbound, id::RESPAWN);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i32(self.dimension);
            w.write_u8(self.difficulty);
            w.write_u8(self.game_mode);
            w.write_string(&self.level_type)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Respawn {
                dimension: r.read_i32()?,
                difficulty: r.read_u8()?,
                game_mode: r.read_u8()?,
                level_type: r.read_string(16)?,
            })
        }
    }

    /// `S08PacketPlayerPosLook`. `y` is the eye height: feet + 1.62.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerPosLook {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub yaw: f32,
        pub pitch: f32,
        pub on_ground: bool,
    }

    impl Packet for PlayerPosLook {
        packet_meta!(Play, Clientbound, id::PLAYER_POS_LOOK);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_f64(self.x);
            w.write_f64(self.y);
            w.write_f64(self.z);
            w.write_f32(self.yaw);
            w.write_f32(self.pitch);
            w.write_bool(self.on_ground);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(PlayerPosLook {
                x: r.read_f64()?,
                y: r.read_f64()?,
                z: r.read_f64()?,
                yaw: r.read_f32()?,
                pitch: r.read_f32()?,
                on_ground: r.read_bool()?,
            })
        }
    }

    /// `S38PacketPlayerListItem`. The server broadcasts each player's keep-alive ping with it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PlayerListItem {
        pub name: String,
        pub online: bool,
        /// Milliseconds, smoothed by the server as `(ping * 3 + rtt) / 4`.
        pub ping: i16,
    }

    impl Packet for PlayerListItem {
        packet_meta!(Play, Clientbound, id::PLAYER_LIST_ITEM);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.name)?;
            w.write_bool(self.online);
            w.write_i16(self.ping);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(PlayerListItem {
                name: r.read_string(16)?,
                online: r.read_bool()?,
                ping: r.read_i16()?,
            })
        }
    }

    /// `S3FPacketCustomPayload` as patched by Forge: the data length is a VarShort.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CustomPayload {
        pub channel: String,
        pub data: Vec<u8>,
    }

    impl Packet for CustomPayload {
        packet_meta!(Play, Clientbound, id::CUSTOM_PAYLOAD);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.channel)?;
            crate::fml::write_varshort(w, self.data.len())?;
            w.write_bytes(&self.data);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            let channel = r.read_string(crate::fml::MAX_CHANNEL_LEN)?;
            let len = crate::fml::read_varshort(r)?;
            Ok(CustomPayload {
                channel,
                data: r.read_bytes(len)?.to_vec(),
            })
        }
    }

    /// `S40PacketDisconnect`: the reason is a JSON chat component.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Disconnect {
        pub reason: String,
    }

    impl Packet for Disconnect {
        packet_meta!(Play, Clientbound, id::DISCONNECT);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_string(&self.reason)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Disconnect {
                reason: r.read_string(32767)?,
            })
        }
    }
}

/// Client to server packets of the play state.
pub mod play_serverbound {
    use super::*;
    use crate::ids::play::serverbound as id;

    /// `C00PacketKeepAlive`: echoes the id of the server's keep-alive.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeepAlive {
        pub id: i32,
    }

    impl Packet for KeepAlive {
        packet_meta!(Play, Serverbound, id::KEEP_ALIVE);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_i32(self.id);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(KeepAlive { id: r.read_i32()? })
        }
    }

    /// `C01PacketChatMessage`. A leading `/` makes it a command.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChatMessage {
        pub message: String,
    }

    /// Longest chat message the server reads.
    pub const MAX_CHAT_LEN: usize = 100;

    impl Packet for ChatMessage {
        packet_meta!(Play, Serverbound, id::CHAT_MESSAGE);

        fn write(&self, w: &mut Writer) -> Result<()> {
            let len = self.message.encode_utf16().count();
            if len > MAX_CHAT_LEN {
                return Err(Error::StringTooLong {
                    max: MAX_CHAT_LEN,
                    len,
                });
            }
            w.write_string(&self.message)
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(ChatMessage {
                message: r.read_string(MAX_CHAT_LEN)?,
            })
        }
    }

    /// `C03PacketPlayer`: no position, only the ground flag.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Player {
        pub on_ground: bool,
    }

    impl Packet for Player {
        packet_meta!(Play, Serverbound, id::PLAYER);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_bool(self.on_ground);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Player {
                on_ground: r.read_bool()?,
            })
        }
    }

    /// `C04PacketPlayerPosition`. `y` is the feet height and `stance` the eye height; the order
    /// on the wire is x, y, stance, z.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerPosition {
        pub x: f64,
        pub y: f64,
        pub stance: f64,
        pub z: f64,
        pub on_ground: bool,
    }

    impl Packet for PlayerPosition {
        packet_meta!(Play, Serverbound, id::PLAYER_POSITION);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_f64(self.x);
            w.write_f64(self.y);
            w.write_f64(self.stance);
            w.write_f64(self.z);
            w.write_bool(self.on_ground);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(PlayerPosition {
                x: r.read_f64()?,
                y: r.read_f64()?,
                stance: r.read_f64()?,
                z: r.read_f64()?,
                on_ground: r.read_bool()?,
            })
        }
    }

    /// `C05PacketPlayerLook`.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerLook {
        pub yaw: f32,
        pub pitch: f32,
        pub on_ground: bool,
    }

    impl Packet for PlayerLook {
        packet_meta!(Play, Serverbound, id::PLAYER_LOOK);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_f32(self.yaw);
            w.write_f32(self.pitch);
            w.write_bool(self.on_ground);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(PlayerLook {
                yaw: r.read_f32()?,
                pitch: r.read_f32()?,
                on_ground: r.read_bool()?,
            })
        }
    }

    /// `C06PacketPlayerPosLook`, the answer a client owes to every
    /// [`super::play_clientbound::PlayerPosLook`].
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerPosLook {
        pub x: f64,
        pub y: f64,
        pub stance: f64,
        pub z: f64,
        pub yaw: f32,
        pub pitch: f32,
        pub on_ground: bool,
    }

    impl Packet for PlayerPosLook {
        packet_meta!(Play, Serverbound, id::PLAYER_POS_LOOK);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_f64(self.x);
            w.write_f64(self.y);
            w.write_f64(self.stance);
            w.write_f64(self.z);
            w.write_f32(self.yaw);
            w.write_f32(self.pitch);
            w.write_bool(self.on_ground);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(PlayerPosLook {
                x: r.read_f64()?,
                y: r.read_f64()?,
                stance: r.read_f64()?,
                z: r.read_f64()?,
                yaw: r.read_f32()?,
                pitch: r.read_f32()?,
                on_ground: r.read_bool()?,
            })
        }
    }

    /// `C15PacketClientSettings`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClientSettings {
        /// At most 7 characters, such as `en_US`.
        pub locale: String,
        pub view_distance: i8,
        /// 0 everything, 1 commands only, 2 hidden.
        pub chat_visibility: i8,
        pub chat_colors: bool,
        pub difficulty: i8,
        pub show_cape: bool,
    }

    impl Packet for ClientSettings {
        packet_meta!(Play, Serverbound, id::CLIENT_SETTINGS);

        fn write(&self, w: &mut Writer) -> Result<()> {
            let len = self.locale.encode_utf16().count();
            if len > 7 {
                return Err(Error::StringTooLong { max: 7, len });
            }
            w.write_string(&self.locale)?;
            w.write_i8(self.view_distance);
            w.write_i8(self.chat_visibility);
            w.write_bool(self.chat_colors);
            w.write_i8(self.difficulty);
            w.write_bool(self.show_cape);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(ClientSettings {
                locale: r.read_string(7)?,
                view_distance: r.read_i8()?,
                chat_visibility: r.read_i8()?,
                chat_colors: r.read_bool()?,
                difficulty: r.read_i8()?,
                show_cape: r.read_bool()?,
            })
        }
    }

    /// `action` of [`ClientStatus`] that asks for a respawn after death.
    pub const STATUS_RESPAWN: u8 = 0;

    /// `C16PacketClientStatus`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ClientStatus {
        pub action: u8,
    }

    impl Packet for ClientStatus {
        packet_meta!(Play, Serverbound, id::CLIENT_STATUS);

        fn write(&self, w: &mut Writer) -> Result<()> {
            w.write_u8(self.action);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            Ok(ClientStatus {
                action: r.read_u8()?,
            })
        }
    }

    /// `C17PacketCustomPayload`. Unlike the clientbound one, its length stays a plain short and
    /// the server only reads data whose length is in ]0, 32767[.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CustomPayload {
        pub channel: String,
        pub data: Vec<u8>,
    }

    /// Largest data a `C17PacketCustomPayload` carries.
    pub const MAX_CUSTOM_PAYLOAD_LEN: usize = 32766;

    impl Packet for CustomPayload {
        packet_meta!(Play, Serverbound, id::CUSTOM_PAYLOAD);

        fn write(&self, w: &mut Writer) -> Result<()> {
            if self.data.len() > MAX_CUSTOM_PAYLOAD_LEN {
                return Err(Error::PayloadTooLarge {
                    len: self.data.len(),
                    max: MAX_CUSTOM_PAYLOAD_LEN,
                });
            }
            w.write_string(&self.channel)?;
            w.write_i16(self.data.len() as i16);
            w.write_bytes(&self.data);
            Ok(())
        }

        fn read(r: &mut Reader<'_>) -> Result<Self> {
            let channel = r.read_string(crate::fml::MAX_CHANNEL_LEN)?;
            let len = r.read_i16()?;
            let data = if len > 0 && len < i16::MAX {
                r.read_bytes(len as usize)?.to_vec()
            } else {
                Vec::new()
            };
            Ok(CustomPayload { channel, data })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::play_clientbound as cb;
    use super::play_serverbound as sb;
    use super::*;
    use crate::frame::{encode_packet, FrameDecoder};

    fn round_trip<P: Packet + PartialEq + std::fmt::Debug>(packet: P) {
        let frame = encode_packet(&packet).unwrap();
        let mut decoder = FrameDecoder::new();
        decoder.feed(&frame);
        let decoded = decoder.next_frame().unwrap().unwrap();
        assert_eq!(decoded.id, P::ID);
        assert_eq!(P::decode_body(decoded.body).unwrap(), packet);
    }

    #[test]
    fn handshake_bytes() {
        let packet = handshake::Handshake {
            protocol_version: 5,
            server_address: "localhost".into(),
            server_port: 25565,
            next_state: handshake::NEXT_LOGIN,
        };
        let frame = encode_packet(&packet).unwrap();
        let mut expected = vec![0x0f, 0x00, 0x05, 0x09];
        expected.extend_from_slice(b"localhost");
        expected.extend_from_slice(&[0x63, 0xdd, 0x02]);
        assert_eq!(frame, expected);
        round_trip(packet);
    }

    #[test]
    fn s08_and_c06_share_coordinates() {
        let s08 = cb::PlayerPosLook {
            x: 152.5,
            y: 65.62000000476837,
            z: -301.5,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: false,
        };
        let mut body = Vec::new();
        for v in [s08.x, s08.y, s08.z] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&90.0f32.to_be_bytes());
        body.extend_from_slice(&0.0f32.to_be_bytes());
        body.push(0);
        assert_eq!(s08.encode_body().unwrap(), body);
        assert_eq!(cb::PlayerPosLook::decode_body(&body).unwrap(), s08);

        let c06 = sb::PlayerPosLook {
            x: s08.x,
            y: 64.0,
            stance: s08.y,
            z: s08.z,
            yaw: s08.yaw,
            pitch: s08.pitch,
            on_ground: false,
        };
        let encoded = c06.encode_body().unwrap();
        assert_eq!(encoded.len(), 8 * 4 + 4 * 2 + 1);
        assert_eq!(&encoded[0..8], &s08.x.to_be_bytes());
        assert_eq!(&encoded[16..24], &s08.y.to_be_bytes());
        assert_eq!(&encoded[24..32], &s08.z.to_be_bytes());
        round_trip(c06);
    }

    #[test]
    fn round_trips() {
        round_trip(status::ServerQuery);
        round_trip(status::ServerInfo {
            json: "{\"description\":\"x\"}".into(),
        });
        round_trip(status::Ping { payload: -42 });
        round_trip(status::Pong {
            payload: 1_700_000_000_000,
        });
        round_trip(login::LoginStart {
            name: "bot123".into(),
        });
        round_trip(login::Disconnect {
            reason: "\"bye\"".into(),
        });
        round_trip(login::EncryptionRequest {
            server_id: String::new(),
            public_key: vec![1, 2, 3],
            verify_token: vec![4; 4],
        });
        round_trip(login::LoginSuccess {
            uuid: "c3b53b56-ab0e-3f6c-8c5b-3e5e5d1d0b8a".into(),
            name: "bot1".into(),
        });
        round_trip(cb::KeepAlive { id: -7 });
        round_trip(cb::JoinGame {
            entity_id: 181,
            game_mode: 0,
            hardcore: true,
            dimension: -1,
            difficulty: 1,
            max_players: 60,
            level_type: "default".into(),
        });
        round_trip(cb::SpawnPosition {
            x: -4,
            y: 64,
            z: 250,
        });
        round_trip(cb::UpdateHealth {
            health: 0.0,
            food: 20,
            saturation: 5.0,
        });
        round_trip(cb::Respawn {
            dimension: 0,
            difficulty: 1,
            game_mode: 0,
            level_type: "flat".into(),
        });
        round_trip(cb::PlayerListItem {
            name: "bot7".into(),
            online: true,
            ping: 12,
        });
        round_trip(cb::CustomPayload {
            channel: "FML|HS".into(),
            data: vec![0, 2, 0, 0, 0, 0],
        });
        round_trip(cb::CustomPayload {
            channel: "FML|HS".into(),
            data: vec![3; 40_000],
        });
        round_trip(cb::Disconnect {
            reason: "{\"text\":\"kicked\"}".into(),
        });
        round_trip(sb::KeepAlive { id: 123_456 });
        round_trip(sb::ChatMessage {
            message: "/list".into(),
        });
        round_trip(sb::Player { on_ground: true });
        round_trip(sb::PlayerPosition {
            x: 0.5,
            y: 64.0,
            stance: 65.62,
            z: -0.5,
            on_ground: true,
        });
        round_trip(sb::PlayerLook {
            yaw: 180.0,
            pitch: -10.0,
            on_ground: false,
        });
        round_trip(sb::ClientSettings {
            locale: "en_US".into(),
            view_distance: 8,
            chat_visibility: 0,
            chat_colors: true,
            difficulty: 1,
            show_cape: true,
        });
        round_trip(sb::ClientStatus {
            action: sb::STATUS_RESPAWN,
        });
        round_trip(sb::CustomPayload {
            channel: "REGISTER".into(),
            data: b"FML|HS\0FML".to_vec(),
        });
    }

    #[test]
    fn join_game_mode_byte() {
        let body = [
            0, 0, 0, 1, 0x09, 0, 2, 20, 7, b'f', b'l', b'a', b't', b'x', b'y', b'z',
        ];
        let join = cb::JoinGame::decode_body(&body).unwrap();
        assert_eq!(join.game_mode, 1);
        assert!(join.hardcore);
        assert_eq!(join.level_type, "flatxyz");
    }

    #[test]
    fn custom_payload_length_encodings() {
        // Serverbound: plain short.
        let c17 = sb::CustomPayload {
            channel: "FML|HS".into(),
            data: vec![1, 2],
        };
        assert_eq!(
            c17.encode_body().unwrap(),
            [&[6][..], b"FML|HS", &[0, 2, 1, 2]].concat()
        );
        assert!(sb::CustomPayload {
            channel: "x".into(),
            data: vec![0; 32767]
        }
        .encode_body()
        .is_err());
        // An empty C17 carries no data.
        let empty = sb::CustomPayload::decode_body(&[1, b'x', 0, 0]).unwrap();
        assert!(empty.data.is_empty());

        // Clientbound: VarShort, an extra byte once 0x8000 is reached.
        let s3f = cb::CustomPayload {
            channel: "A".into(),
            data: vec![0; 0x8000],
        };
        let body = s3f.encode_body().unwrap();
        assert_eq!(&body[..5], &[1, b'A', 0x80, 0x00, 0x01]);
    }

    #[test]
    fn limits_are_enforced() {
        assert!(login::LoginStart {
            name: "abcdefghijklmnopq".into()
        }
        .encode_body()
        .is_err());
        assert!(sb::ChatMessage {
            message: "x".repeat(101)
        }
        .encode_body()
        .is_err());
        assert_eq!(
            cb::KeepAlive::decode_body(&[0, 0, 0, 1, 9]),
            Err(Error::TrailingBytes(1))
        );
        assert_eq!(
            cb::KeepAlive::decode_body(&[0, 0, 1]),
            Err(Error::UnexpectedEof)
        );
    }
}
