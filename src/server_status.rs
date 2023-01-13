use serde::Deserialize;
use uuid_mc::PlayerUuid;

use crate::interface::Interface;

#[derive(Deserialize)]
pub struct PlayerData {
    pub name: String,
    pub nickname: Option<String>,
    pub nickname_styled: Option<azalea_chat::Component>,
    pub uuid: Option<PlayerUuid>,
}

#[derive(Deserialize)]
pub struct OnlineServerStatus {
    pub current_players: i32,
    pub max_players: i32,
    pub list: Vec<PlayerData>,
}

pub enum ServerStatus {
    Offline,
    Online(OnlineServerStatus),
}

pub async fn get_server_status(
    interface: &mut Interface,
    uses_list_json: bool,
) -> Result<ServerStatus, std::borrow::Cow<'static, str>> {
    if uses_list_json {
        let list = interface.exec("list json").await;
        if let Ok(list) = list {
            let status = serde_json::from_str::<OnlineServerStatus>(&list);
            match status {
                Ok(status) => Ok(ServerStatus::Online(status)),
                Err(why) => {
                    eprintln!("Deserialization error: {why}. Response from server: {list}");
                    Err("Deserialization error (this is a bug)".into())
                }
            }
        } else {
            // if there's an error, it can't be a CommandTooLong. therefore, the server must be offline.
            Ok(ServerStatus::Offline)
        }
    } else {
        let regex = super::LIST_REGEX.get().unwrap();
        let list = interface.exec("list").await;
        if let Ok(list) = list {
            let list = list.trim();
            let Some(captures) = regex.captures(list) else {
                eprintln!("Regex error. Response from server: {list}");
                return Err("Regex error (this is a bug)".into());
            };

            let mut captures = captures.iter().flatten().map(|mat| mat.as_str());
            captures.next().unwrap(); // this is the entire text, not interesting

            // these unwraps are ok, because we know we're getting a well-formatted response from the server
            let current_players: i32 = captures.next().unwrap().parse().unwrap();
            let max_players: i32 = captures.next().unwrap().parse().unwrap();

            let mut players: Vec<&str> = captures.next().unwrap_or_default().split(", ").collect();
            players.sort_unstable();

            Ok(ServerStatus::Online(OnlineServerStatus {
                current_players,
                max_players,
                list: players
                    .into_iter()
                    .map(|name| PlayerData {
                        name: name.to_string(),
                        nickname: None,
                        nickname_styled: None,
                        uuid: None,
                    })
                    .collect(),
            }))
        } else {
            // if there's an error, it can't be a CommandTooLong. therefore, the server must be offline.
            Ok(ServerStatus::Offline)
        }
    }
}

#[test]
fn test() {
    let list = r##"{"current_players":1,"max_players":8,"list":[{"name":"Player839","nickname":"<rainbow>test","nickname_styled":{"extra":[{"extra":[{"extra":[{"color":"#FF0000","text":"t"},{"color":"#CBFF00","text":"e"},{"color":"#00FF66","text":"s"},{"color":"#0065FF","text":"t"}],"text":""}],"text":""}],"text":"#"},"uuid":"66397f00-f974-3e3d-944b-5f58f7613e27"}]}"##;
    let status = serde_json::from_str::<OnlineServerStatus>(list).unwrap();
    assert_eq!(status.list.first().unwrap().nickname_styled.as_ref().unwrap().to_ansi(), "#\u{1b}[38;2;255;0;0mt\u{1b}[38;2;203;255;0me\u{1b}[38;2;0;255;102ms\u{1b}[38;2;0;101;255mt\u{1b}[m");
}
