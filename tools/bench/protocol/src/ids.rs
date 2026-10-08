//! Packet identifiers of protocol 5, from `EnumConnectionState` of Minecraft 1.7.10.
//!
//! Identifiers are only meaningful together with a connection state and a direction: `0x00` is a
//! handshake, a status query, a login start or a keep-alive depending on both.

/// Protocol version sent in the handshake and announced by the status response.
pub const PROTOCOL_VERSION: i32 = 5;

/// Game version that speaks [`PROTOCOL_VERSION`].
pub const GAME_VERSION: &str = "1.7.10";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Handshaking,
    Status,
    Login,
    Play,
}

impl State {
    /// State requested by the `next_state` field of the handshake.
    pub fn from_next_state(next: i32) -> Option<State> {
        match next {
            1 => Some(State::Status),
            2 => Some(State::Login),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Server to client (`S..Packet` classes).
    Clientbound,
    /// Client to server (`C..Packet` classes).
    Serverbound,
}

macro_rules! packet_ids {
    ($( $name:ident = $id:literal => $label:literal ),* $(,)?) => {
        $( pub const $name: i32 = $id; )*
        pub(crate) const NAMES: &[(i32, &str)] = &[ $( ($id, $label) ),* ];
    };
}

pub mod handshaking {
    pub mod serverbound {
        packet_ids! {
            HANDSHAKE = 0x00 => "Handshake",
        }
    }
}

pub mod status {
    pub mod clientbound {
        packet_ids! {
            SERVER_INFO = 0x00 => "ServerInfo",
            PONG = 0x01 => "Pong",
        }
    }
    pub mod serverbound {
        packet_ids! {
            SERVER_QUERY = 0x00 => "ServerQuery",
            PING = 0x01 => "Ping",
        }
    }
}

pub mod login {
    pub mod clientbound {
        packet_ids! {
            DISCONNECT = 0x00 => "Disconnect",
            ENCRYPTION_REQUEST = 0x01 => "EncryptionRequest",
            LOGIN_SUCCESS = 0x02 => "LoginSuccess",
        }
    }
    pub mod serverbound {
        packet_ids! {
            LOGIN_START = 0x00 => "LoginStart",
            ENCRYPTION_RESPONSE = 0x01 => "EncryptionResponse",
        }
    }
}

pub mod play {
    pub mod clientbound {
        packet_ids! {
            KEEP_ALIVE = 0x00 => "KeepAlive",
            JOIN_GAME = 0x01 => "JoinGame",
            CHAT = 0x02 => "Chat",
            TIME_UPDATE = 0x03 => "TimeUpdate",
            ENTITY_EQUIPMENT = 0x04 => "EntityEquipment",
            SPAWN_POSITION = 0x05 => "SpawnPosition",
            UPDATE_HEALTH = 0x06 => "UpdateHealth",
            RESPAWN = 0x07 => "Respawn",
            PLAYER_POS_LOOK = 0x08 => "PlayerPosLook",
            HELD_ITEM_CHANGE = 0x09 => "HeldItemChange",
            USE_BED = 0x0a => "UseBed",
            ANIMATION = 0x0b => "Animation",
            SPAWN_PLAYER = 0x0c => "SpawnPlayer",
            COLLECT_ITEM = 0x0d => "CollectItem",
            SPAWN_OBJECT = 0x0e => "SpawnObject",
            SPAWN_MOB = 0x0f => "SpawnMob",
            SPAWN_PAINTING = 0x10 => "SpawnPainting",
            SPAWN_EXPERIENCE_ORB = 0x11 => "SpawnExperienceOrb",
            ENTITY_VELOCITY = 0x12 => "EntityVelocity",
            DESTROY_ENTITIES = 0x13 => "DestroyEntities",
            ENTITY = 0x14 => "Entity",
            ENTITY_REL_MOVE = 0x15 => "EntityRelMove",
            ENTITY_LOOK = 0x16 => "EntityLook",
            ENTITY_LOOK_MOVE = 0x17 => "EntityLookMove",
            ENTITY_TELEPORT = 0x18 => "EntityTeleport",
            ENTITY_HEAD_LOOK = 0x19 => "EntityHeadLook",
            ENTITY_STATUS = 0x1a => "EntityStatus",
            ENTITY_ATTACH = 0x1b => "EntityAttach",
            ENTITY_METADATA = 0x1c => "EntityMetadata",
            ENTITY_EFFECT = 0x1d => "EntityEffect",
            REMOVE_ENTITY_EFFECT = 0x1e => "RemoveEntityEffect",
            SET_EXPERIENCE = 0x1f => "SetExperience",
            ENTITY_PROPERTIES = 0x20 => "EntityProperties",
            CHUNK_DATA = 0x21 => "ChunkData",
            MULTI_BLOCK_CHANGE = 0x22 => "MultiBlockChange",
            BLOCK_CHANGE = 0x23 => "BlockChange",
            BLOCK_ACTION = 0x24 => "BlockAction",
            BLOCK_BREAK_ANIM = 0x25 => "BlockBreakAnim",
            MAP_CHUNK_BULK = 0x26 => "MapChunkBulk",
            EXPLOSION = 0x27 => "Explosion",
            EFFECT = 0x28 => "Effect",
            SOUND_EFFECT = 0x29 => "SoundEffect",
            PARTICLES = 0x2a => "Particles",
            CHANGE_GAME_STATE = 0x2b => "ChangeGameState",
            SPAWN_GLOBAL_ENTITY = 0x2c => "SpawnGlobalEntity",
            OPEN_WINDOW = 0x2d => "OpenWindow",
            CLOSE_WINDOW = 0x2e => "CloseWindow",
            SET_SLOT = 0x2f => "SetSlot",
            WINDOW_ITEMS = 0x30 => "WindowItems",
            WINDOW_PROPERTY = 0x31 => "WindowProperty",
            CONFIRM_TRANSACTION = 0x32 => "ConfirmTransaction",
            UPDATE_SIGN = 0x33 => "UpdateSign",
            MAPS = 0x34 => "Maps",
            UPDATE_TILE_ENTITY = 0x35 => "UpdateTileEntity",
            SIGN_EDITOR_OPEN = 0x36 => "SignEditorOpen",
            STATISTICS = 0x37 => "Statistics",
            PLAYER_LIST_ITEM = 0x38 => "PlayerListItem",
            PLAYER_ABILITIES = 0x39 => "PlayerAbilities",
            TAB_COMPLETE = 0x3a => "TabComplete",
            SCOREBOARD_OBJECTIVE = 0x3b => "ScoreboardObjective",
            UPDATE_SCORE = 0x3c => "UpdateScore",
            DISPLAY_SCOREBOARD = 0x3d => "DisplayScoreboard",
            TEAMS = 0x3e => "Teams",
            CUSTOM_PAYLOAD = 0x3f => "CustomPayload",
            DISCONNECT = 0x40 => "Disconnect",
        }
    }
    pub mod serverbound {
        packet_ids! {
            KEEP_ALIVE = 0x00 => "KeepAlive",
            CHAT_MESSAGE = 0x01 => "ChatMessage",
            USE_ENTITY = 0x02 => "UseEntity",
            PLAYER = 0x03 => "Player",
            PLAYER_POSITION = 0x04 => "PlayerPosition",
            PLAYER_LOOK = 0x05 => "PlayerLook",
            PLAYER_POS_LOOK = 0x06 => "PlayerPosLook",
            PLAYER_DIGGING = 0x07 => "PlayerDigging",
            PLAYER_BLOCK_PLACEMENT = 0x08 => "PlayerBlockPlacement",
            HELD_ITEM_CHANGE = 0x09 => "HeldItemChange",
            ANIMATION = 0x0a => "Animation",
            ENTITY_ACTION = 0x0b => "EntityAction",
            INPUT = 0x0c => "Input",
            CLOSE_WINDOW = 0x0d => "CloseWindow",
            CLICK_WINDOW = 0x0e => "ClickWindow",
            CONFIRM_TRANSACTION = 0x0f => "ConfirmTransaction",
            CREATIVE_INVENTORY_ACTION = 0x10 => "CreativeInventoryAction",
            ENCHANT_ITEM = 0x11 => "EnchantItem",
            UPDATE_SIGN = 0x12 => "UpdateSign",
            PLAYER_ABILITIES = 0x13 => "PlayerAbilities",
            TAB_COMPLETE = 0x14 => "TabComplete",
            CLIENT_SETTINGS = 0x15 => "ClientSettings",
            CLIENT_STATUS = 0x16 => "ClientStatus",
            CUSTOM_PAYLOAD = 0x17 => "CustomPayload",
        }
    }
}

/// Name of a packet, for logs and diagnostics. `None` for an id the state does not define.
pub fn packet_name(state: State, direction: Direction, id: i32) -> Option<&'static str> {
    let table = match (state, direction) {
        (State::Handshaking, Direction::Serverbound) => handshaking::serverbound::NAMES,
        (State::Handshaking, Direction::Clientbound) => &[],
        (State::Status, Direction::Clientbound) => status::clientbound::NAMES,
        (State::Status, Direction::Serverbound) => status::serverbound::NAMES,
        (State::Login, Direction::Clientbound) => login::clientbound::NAMES,
        (State::Login, Direction::Serverbound) => login::serverbound::NAMES,
        (State::Play, Direction::Clientbound) => play::clientbound::NAMES,
        (State::Play, Direction::Serverbound) => play::serverbound::NAMES,
    };
    table
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_tables_are_dense_and_ordered() {
        // EnumConnectionState registers ids 0..=64 clientbound and 0..=23 serverbound, no gaps.
        for (table, last) in [
            (play::clientbound::NAMES, 0x40),
            (play::serverbound::NAMES, 0x17),
        ] {
            assert_eq!(table.len(), last as usize + 1);
            for (index, (id, _)) in table.iter().enumerate() {
                assert_eq!(*id, index as i32);
            }
        }
    }

    #[test]
    fn names_depend_on_state_and_direction() {
        assert_eq!(
            packet_name(State::Play, Direction::Clientbound, 0x26),
            Some("MapChunkBulk")
        );
        assert_eq!(
            packet_name(State::Play, Direction::Serverbound, 0x06),
            Some("PlayerPosLook")
        );
        assert_eq!(
            packet_name(State::Login, Direction::Clientbound, 0x00),
            Some("Disconnect")
        );
        assert_eq!(
            packet_name(State::Status, Direction::Serverbound, 0x00),
            Some("ServerQuery")
        );
        assert_eq!(packet_name(State::Play, Direction::Clientbound, 0x41), None);
        assert_eq!(
            packet_name(State::Handshaking, Direction::Clientbound, 0x00),
            None
        );
    }

    #[test]
    fn next_state_mapping() {
        assert_eq!(State::from_next_state(1), Some(State::Status));
        assert_eq!(State::from_next_state(2), Some(State::Login));
        assert_eq!(State::from_next_state(3), None);
    }
}
