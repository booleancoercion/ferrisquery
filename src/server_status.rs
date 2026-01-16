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
) -> Result<ServerStatus, crate::Error> {
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
        use chumsky::prelude::*;

        fn parser<'a>() -> impl Parser<'a, &'a str, OnlineServerStatus, extra::Err<Rich<'a, char>>>
        {
            let hex = |n| {
                any()
                    .filter(char::is_ascii_hexdigit)
                    .repeated()
                    .exactly(n)
                    .to_slice()
                    .try_map(|s: &str, span| {
                        u128::from_str_radix(s, 16).map_err(|e| Rich::custom(span, e))
                    })
            };

            group((
                just("There are ").ignored(),
                text::int(10)
                    .try_map(|s: &str, span| s.parse::<i32>().map_err(|e| Rich::custom(span, e))),
                just(" of a max of ").ignored(),
                text::int(10)
                    .try_map(|s: &str, span| s.parse::<i32>().map_err(|e| Rich::custom(span, e))),
                just(" players online: ").ignored(),
                group((
                    any()
                        .filter(|b| *b != ' ')
                        .repeated()
                        .at_least(1)
                        .to_slice(),
                    just(' ').ignored(),
                    group((
                        hex(8),
                        just('-').ignored(),
                        hex(4),
                        just('-').ignored(),
                        hex(4),
                        just('-').ignored(),
                        hex(4),
                        just('-').ignored(),
                        hex(12),
                    ))
                    .try_map(|(a, _, b, _, c, _, d, _, e), span| {
                        let mut uuid = a;
                        uuid <<= 16;
                        uuid += b;
                        uuid <<= 16;
                        uuid += c;
                        uuid <<= 16;
                        uuid += d;
                        uuid <<= 48;
                        uuid += e;

                        PlayerUuid::new_with_uuid(uuid_mc::Uuid::from_u128(uuid))
                            .map_err(|e| Rich::custom(span, e))
                    })
                    .delimited_by(just('('), just(')')),
                ))
                .map(|(name, _, uuid): (&str, (), _)| PlayerData {
                    name: name.into(),
                    nickname: None,
                    uuid: Some(uuid),
                })
                .separated_by(just(' '))
                .collect::<Vec<_>>(),
            ))
            .map(
                |(_, current_players, _, max_players, _, list)| OnlineServerStatus {
                    current_players,
                    max_players,
                    list,
                    tps: None,
                },
            )
        }

        let list = interface.exec("list uuids").await;
        if let Ok(list) = list {
            let result = parser().parse(&list).into_result();
            result
                .map(ServerStatus::Online)
                .map_err(|e| format!("Failed to parse output of `/list uuids`: {:?}", e).into())
        } else {
            // if there's an error, it can't be a CommandTooLong. therefore, the server must be offline.
            Ok(ServerStatus::Offline)
        }
    }
}
