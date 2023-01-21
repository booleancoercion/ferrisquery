use serde::Deserialize;
use uuid_mc::PlayerUuid;

use crate::interface::Interface;

#[derive(Deserialize)]
pub struct PlayerData {
    pub name: String,
    pub nickname: Option<String>,
    pub uuid: Option<PlayerUuid>,
}

#[derive(Deserialize)]
pub struct OnlineServerStatus {
    pub current_players: i32,
    pub max_players: i32,
    pub list: Vec<PlayerData>,
    pub tps: Option<[f32; 5]>,
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
                        uuid: None,
                    })
                    .collect(),
                tps: None,
            }))
        } else {
            // if there's an error, it can't be a CommandTooLong. therefore, the server must be offline.
            Ok(ServerStatus::Offline)
        }
    }
}
