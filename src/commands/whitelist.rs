use std::borrow::Cow;
use std::fmt::Write;

use if_chain::if_chain;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::interaction::application_command::{
    CommandDataOption, CommandDataOptionValue,
};
use tokio::io::AsyncWriteExt;
use uuid_mc::PlayerUuid;

use crate::interface::Interface;

#[derive(Serialize, Deserialize)]
struct WhitelistEntry<'a> {
    name: Cow<'a, str>,
    uuid: PlayerUuid,
}

pub async fn run(
    server_directory: &str,
    is_op: bool,
    interface: &mut Interface,
    options: &[CommandDataOption],
) -> Cow<'static, str> {
    let filename = format!("{server_directory}/whitelist.json");
    let raw_json = match tokio::fs::read_to_string(&filename).await {
        Ok(string) => string,
        Err(why) => {
            eprintln!("Error reading whitelist file: {why:?}");
            return "Couldn't read whitelist file.".into();
        }
    };
    let mut whitelist: Vec<WhitelistEntry> = match serde_json::from_str(&raw_json) {
        Ok(whitelist) => whitelist,
        Err(why) => {
            eprintln!("Error parsing whitelist file: {why:?}");
            return "Couldn't parse whitelist file".into();
        }
    };

    whitelist.sort_unstable_by(|e1, e2| e1.name.cmp(&e2.name));

    let [CommandDataOption {
        name,
        kind: CommandOptionType::SubCommand,
        options,
        ..
    }, ..] = options else {
        return "no".into();
    };

    let ret = match name.as_str() {
        "add" => {
            if !is_op {
                return "You're not an op!".into();
            }

            let username = if_chain! {
                if let Some(username) = options.get(0);
                if username.name == "username";
                if let Some(resolved) = &username.resolved;
                if let CommandDataOptionValue::String(username) = resolved;
                then {
                    username
                } else {
                    return "no".into();
                }
            };

            if whitelist.iter().any(|entry| &entry.name == username) {
                return format!("The user {username} is already in the whitelist.").into();
            }

            let is_online = if_chain! {
                if let Some(is_online) = options.get(1);
                if is_online.name == "mode";
                if let Some(resolved) = &is_online.resolved;
                if let CommandDataOptionValue::String(is_online) = resolved;
                then {
                    match is_online.as_str() {
                        "online" => true,
                        "offline" => false,
                        _ => return "no".into()
                    }
                } else {
                    return "no".into();
                }
            };

            let uuid = if is_online {
                if let Ok(uuid) = PlayerUuid::new_with_online_username(username) {
                    uuid
                } else {
                    return "Error in adding user (perhaps this username doesn't exist, or the mojang authentication servers are down)".into();
                }
            } else {
                PlayerUuid::new_with_offline_username(username)
            };

            whitelist.push(WhitelistEntry {
                name: username.into(),
                uuid,
            });

            format!("Player {username} added to the whitelist.").into()
        }
        "remove" => {
            if !is_op {
                return "You're not an op!".into();
            }

            let username = if_chain! {
                if let Some(username) = options.get(0);
                if username.name == "username";
                if let Some(resolved) = &username.resolved;
                if let CommandDataOptionValue::String(username) = resolved;
                then {
                    username
                } else {
                    return "no".into();
                }
            };

            let len = whitelist.len();
            whitelist.retain(|entry| &entry.name != username);
            if whitelist.len() < len {
                format!("Player {username} removed from the whitelist.").into()
            } else {
                return format!("The player {username} is not in the whitelist.").into();
            }
        }
        "list" => {
            let mut result = format!("There are {} whitelisted players", whitelist.len());
            if whitelist.is_empty() {
                write!(&mut result, ".").unwrap();
            } else {
                #[allow(unstable_name_collisions)]
                write!(
                    &mut result,
                    ":\n```\n{}\n```",
                    whitelist
                        .iter()
                        .map(|entry| &*entry.name)
                        .intersperse(", ")
                        .collect::<String>()
                )
                .unwrap();
            }

            return result.into();
        }
        _ => return "no".into(),
    };

    let raw_json = serde_json::to_string_pretty(&whitelist).unwrap(); // this serialization cannot fail
    let mut file = match tokio::fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(&filename)
        .await
    {
        Ok(file) => file,
        Err(why) => {
            eprintln!("Error opening whitelist file for writing: {why:?}");
            return "Couldn't open whitelist file for writing".into();
        }
    };
    if let Err(why) = file.write_all(raw_json.as_bytes()).await {
        eprintln!("Error writing to whitelist file: {why:?}");
        return "Couldn't write to whitelist file.".into();
    }

    // we don't care if the command succeeds, because then that means the server
    // is offline and so the whitelist will be reloaded anyway when it comes online.
    let _ = interface.exec("whitelist reload").await;

    ret
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
    command
        .name("whitelist")
        .description(
            "Make changes to the whitelist that also modify related data, like the user database.",
        )
        .create_option(|option| {
            option
                .name("add")
                .description("Add a user to the whitelist.")
                .kind(CommandOptionType::SubCommand)
                .create_sub_option(|option| {
                    option
                        .name("username")
                        .description("The user to be added.")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
                .create_sub_option(|option| {
                    option
                        .name("mode")
                        .description("Whether the user uses online or offline mode.")
                        .kind(CommandOptionType::String)
                        .add_string_choice("Online", "online")
                        .add_string_choice("Offline", "offline")
                        .required(true)
                })
        })
        .create_option(|option| {
            option
                .name("remove")
                .description("Remove a player from the whitelist.")
                .kind(CommandOptionType::SubCommand)
                .create_sub_option(|option| {
                    option
                        .name("username")
                        .description("The user to be removed.")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .create_option(|option| {
            option
                .name("list")
                .description("Return the list of whitelisted players.")
                .kind(CommandOptionType::SubCommand)
        })
}
