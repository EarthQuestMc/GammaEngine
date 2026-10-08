//! The server list response and chat components.
//!
//! A Forge server adds `"modinfo":{"type":"FML","modList":[{"modid":…,"version":…},…]}` to its
//! status response. A client must send that exact list back in the `FML|HS` handshake, so the
//! bench reads it once before connecting its bots.

use crate::json::Json;
use crate::Result;

pub use crate::fml::ModEntry;

/// The fields of `S00PacketServerInfo` a client cares about.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusResponse {
    pub version_name: Option<String>,
    pub protocol: Option<i64>,
    pub players_online: Option<i64>,
    pub players_max: Option<i64>,
    /// The message of the day, as plain text.
    pub description: String,
    /// `modinfo.type`, `"FML"` on a Forge server.
    pub mod_type: Option<String>,
    /// `modinfo.modList`; `None` when the server is not a Forge server.
    pub mod_list: Option<Vec<ModEntry>>,
}

impl StatusResponse {
    pub fn parse(json: &str) -> Result<StatusResponse> {
        let doc = Json::parse(json)?;
        let version = doc.get("version");
        let players = doc.get("players");
        let modinfo = doc.get("modinfo");
        let mod_list = modinfo
            .and_then(|m| m.get("modList"))
            .and_then(Json::as_array)
            .map(|mods| {
                mods.iter()
                    .filter_map(|entry| {
                        Some(ModEntry {
                            modid: entry.get("modid")?.as_str()?.to_owned(),
                            version: entry.get("version")?.as_str()?.to_owned(),
                        })
                    })
                    .collect()
            });
        Ok(StatusResponse {
            version_name: version
                .and_then(|v| v.get("name"))
                .and_then(Json::as_str)
                .map(str::to_owned),
            protocol: version
                .and_then(|v| v.get("protocol"))
                .and_then(Json::as_i64),
            players_online: players.and_then(|p| p.get("online")).and_then(Json::as_i64),
            players_max: players.and_then(|p| p.get("max")).and_then(Json::as_i64),
            description: doc
                .get("description")
                .map(component_text)
                .unwrap_or_default(),
            mod_type: modinfo
                .and_then(|m| m.get("type"))
                .and_then(Json::as_str)
                .map(str::to_owned),
            mod_list,
        })
    }
}

/// Plain text of a serialized chat component (kick reasons, MOTD), without `§` formatting
/// codes. Text that is not JSON is returned as it is.
pub fn chat_to_plain(serialized: &str) -> String {
    match Json::parse(serialized) {
        Ok(doc) => strip_formatting(&component_text(&doc)),
        Err(_) => strip_formatting(serialized),
    }
}

/// Removes `§x` formatting codes.
pub fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

fn component_text(component: &Json) -> String {
    let mut out = String::new();
    append_component(component, &mut out, 0);
    out
}

fn append_component(component: &Json, out: &mut String, depth: usize) {
    if depth > 32 {
        return;
    }
    match component {
        Json::String(text) => out.push_str(text),
        Json::Number(n) => out.push_str(&n.to_string()),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Array(parts) => {
            for part in parts {
                append_component(part, out, depth + 1);
            }
        }
        Json::Object(_) => {
            if let Some(text) = component.get("text").and_then(Json::as_str) {
                out.push_str(text);
            } else if let Some(key) = component.get("translate").and_then(Json::as_str) {
                // No translation table here: keep the key and its arguments.
                out.push_str(key);
                if let Some(args) = component.get("with").and_then(Json::as_array) {
                    out.push_str(" [");
                    for (index, arg) in args.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        append_component(arg, out, depth + 1);
                    }
                    out.push(']');
                }
            }
            if let Some(extra) = component.get("extra").and_then(Json::as_array) {
                for part in extra {
                    append_component(part, out, depth + 1);
                }
            }
        }
        Json::Null => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORGE_STATUS: &str = r#"{"description":"GammaEngine test server","players":{"max":500,"online":3},"version":{"name":"1.7.10","protocol":5},"modinfo":{"type":"FML","modList":[{"modid":"mcp","version":"9.05"},{"modid":"FML","version":"7.10.99.99"},{"modid":"Forge","version":"10.13.4.1614"},{"modid":"ic2","version":"2.2.827=experimental"}]}}"#;

    #[test]
    fn forge_status_with_mods() {
        let status = StatusResponse::parse(FORGE_STATUS).unwrap();
        assert_eq!(status.version_name.as_deref(), Some("1.7.10"));
        assert_eq!(status.protocol, Some(5));
        assert_eq!(status.players_online, Some(3));
        assert_eq!(status.players_max, Some(500));
        assert_eq!(status.description, "GammaEngine test server");
        assert_eq!(status.mod_type.as_deref(), Some("FML"));
        assert_eq!(
            status.mod_list.unwrap(),
            vec![
                ModEntry::new("mcp", "9.05"),
                ModEntry::new("FML", "7.10.99.99"),
                ModEntry::new("Forge", "10.13.4.1614"),
                ModEntry::new("ic2", "2.2.827=experimental"),
            ]
        );
    }

    #[test]
    fn forge_status_without_mods() {
        let json = r#"{"description":{"text":"A ","extra":[{"text":"§aserver","bold":true}]},"players":{"max":20,"online":0},"version":{"name":"1.7.10","protocol":5},"modinfo":{"type":"FML","modList":[]}}"#;
        let status = StatusResponse::parse(json).unwrap();
        assert_eq!(status.mod_list, Some(vec![]));
        assert_eq!(status.description, "A §aserver");
        assert_eq!(
            chat_to_plain(r#"{"text":"A ","extra":[{"text":"§aserver"}]}"#),
            "A server"
        );
    }

    #[test]
    fn vanilla_status_has_no_mod_list() {
        let json = r#"{"description":"x","players":{"max":20,"online":0},"version":{"name":"1.7.10","protocol":5},"favicon":"data:image/png;base64,AAAA"}"#;
        let status = StatusResponse::parse(json).unwrap();
        assert_eq!(status.mod_list, None);
        assert_eq!(status.mod_type, None);
    }

    #[test]
    fn malformed_entries_are_dropped() {
        let json =
            r#"{"modinfo":{"type":"FML","modList":[{"modid":"a"},{"modid":"b","version":"1"},3]}}"#;
        let status = StatusResponse::parse(json).unwrap();
        assert_eq!(status.mod_list, Some(vec![ModEntry::new("b", "1")]));
        assert_eq!(status.protocol, None);
        assert!(StatusResponse::parse("{\"modinfo\":").is_err());
    }

    #[test]
    fn chat_components() {
        assert_eq!(
            chat_to_plain("\"You logged in from another location\""),
            "You logged in from another location"
        );
        assert_eq!(
            chat_to_plain(r#"{"translate":"disconnect.spam","with":[]}"#),
            "disconnect.spam []"
        );
        assert_eq!(
            chat_to_plain(
                r#"{"translate":"multiplayer.player.left","with":["bot1",{"text":"x"}]}"#
            ),
            "multiplayer.player.left [bot1, x]"
        );
        assert_eq!(chat_to_plain("not json"), "not json");
        assert_eq!(chat_to_plain("[\"a\",{\"text\":\"b\"}]"), "ab");
    }
}
