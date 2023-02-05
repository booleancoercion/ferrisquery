mod commands;
mod interface;
mod server_status;

use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::sync::Arc;

use once_cell::sync::OnceCell;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::*;
use serenity::prelude::*;
use tokio::io::AsyncWriteExt;

use crate::server_status::{OnlineServerStatus, ServerStatus};

const CACHE_FILE_NAME: &str = "ferrisquery_cache.toml";

struct Handler {
    interface: Arc<Mutex<interface::Interface>>,
    guild_id: GuildId,
    op_role_id: RoleId,
    list_channel_id: ChannelId,
    cache: Arc<Mutex<Option<Cache>>>,
    restart_scheduled: Arc<Mutex<bool>>,
    has_list_json: bool,
    server_directory: Box<str>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "run" => {
                    let Some(member) = &command.member else {
                        println!("/run has been executed outside of a guild!");
                        return
                    };

                    if member.roles.contains(&self.op_role_id) {
                        commands::run::run(&mut *self.interface.lock().await, &command.data.options)
                            .await
                    } else {
                        Cow::Borrowed("You're not an op!")
                    }
                }
                "source" => Cow::Borrowed("<https://github.com/booleancoercion/ferrisquery>"),
                "schedule_restart" => {
                    let Some(member) = &command.member else {
                        println!("/schedule_restart has been executed outside of a guild!");
                        return
                    };

                    if member.roles.contains(&self.op_role_id) {
                        commands::schedule_restart::run(
                            &mut *self.restart_scheduled.lock().await,
                            &command.data.options,
                        )
                        .await
                    } else {
                        Cow::Borrowed("You're not an op!")
                    }
                }
                "crash" => match commands::crash::run(&self.server_directory).await {
                    Ok(file) => {
                        if let Err(why) = command
                            .create_interaction_response(&ctx.http, |response| {
                                response
                                    .kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|message| {
                                        message.add_file(&file.0);
                                        message.content(format!(
                                            "This file was created <t:{}:R>.",
                                            file.1
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs()
                                        ))
                                    })
                            })
                            .await
                        {
                            println!("Cannot respond to slash command (/crash): {why}");
                        }
                        return;
                    }
                    Err(msg) => msg,
                },
                "whitelist" => {
                    let Some(member) = &command.member else {
                        println!("/whitelist has been executed outside of a guild!");
                        return
                    };

                    let is_op = member.roles.contains(&self.op_role_id);
                    commands::whitelist::run(
                        &self.server_directory,
                        is_op,
                        &mut *self.interface.lock().await,
                        &command.data.options,
                    )
                    .await
                }
                _ => Cow::Borrowed("not implemented :("),
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(content))
                })
                .await
            {
                println!("Cannot respond to slash command: {why}");
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let commands = GuildId::set_application_commands(&self.guild_id, &ctx.http, |commands| {
            commands.create_application_command(|command| commands::run::register(command));
            commands.create_application_command(|command| commands::source::register(command));
            commands.create_application_command(|command| commands::crash::register(command));
            commands.create_application_command(|command| commands::whitelist::register(command));
            commands
                .create_application_command(|command| commands::schedule_restart::register(command))
        })
        .await;

        println!("I now have the following guild slash commands: {commands:#?}");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            interval.tick().await;

            let status = server_status::get_server_status(
                &mut *self.interface.lock().await,
                self.has_list_json,
            )
            .await;

            match status {
                Ok(status) => {
                    let ServerStatus::Online(OnlineServerStatus {current_players, max_players, list, tps}) = status else {
                        self.set_list_text(&ctx, "The server is offline.").await;

                        // also clear any scheduled restarts
                        *self.restart_scheduled.lock().await = false;
                        continue;
                    };

                    let regex = TAG_REGEX.get().unwrap();
                    let naughty_regex = NAUGHTY_REGEX.get().unwrap();
                    let mut naughty = Vec::new();
                    let players = list
                        .iter()
                        .map(|data| {
                            if let Some(nick) = &data.nickname {
                                let nick = regex.replace_all(nick, "");
                                if naughty_regex.is_match(&nick) {
                                    naughty.push(&data.name);
                                    format!("{} ({})", data.name, NAUGHTY_NICKNAME)
                                } else {
                                    format!("{} ({})", data.name, nick)
                                }
                            } else {
                                data.name.to_string()
                            }
                        })
                        .collect::<Vec<_>>();

                    let mut text = format!(
                        "The server is online. There are {current_players}/{max_players} connected players"
                    );

                    if current_players > 0 {
                        write!(&mut text, ": ```\n{}```", players.join("\n")).unwrap();
                    } else {
                        writeln!(&mut text, ".").unwrap();
                    }

                    if let Some(tps) = tps {
                        write!(&mut text, "\nTPS info: ```\n5s    10s   1m    5m    15m  \n{:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2}```", tps[0], tps[1], tps[2], tps[3], tps[4]).unwrap();
                    }
                    self.set_list_text(&ctx, &text).await;

                    {
                        let mut interface = self.interface.lock().await;
                        for name in naughty {
                            let _ = interface
                                .exec(&format!("styled-nicknames set {name} {NAUGHTY_NICKNAME}"))
                                .await;
                            let _ = interface.exec(&format!("kick {name} nice try")).await;
                        }
                    }

                    // if a restart has been scheduled and there are no players online, do it
                    if current_players == 0 && *self.restart_scheduled.lock().await {
                        let _ = self.interface.lock().await.exec("stop").await;
                    }
                }
                Err(why) => {
                    self.set_list_text(&ctx, &why).await;
                }
            }
        }
    }
}

impl Handler {
    async fn set_list_text(&self, ctx: &Context, text: &str) {
        let cache = &mut *self.cache.lock().await;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let text = format!("{text}\n\nLast update: <t:{timestamp}:T>");

        if let Some(Cache {
            list_channel_id,
            list_message_id,
        }) = *cache
        {
            if let Err(why) = list_channel_id
                .edit_message(&ctx.http, list_message_id, |m| m.content(text))
                .await
            {
                eprintln!("Couldn't edit list message: {why}")
            }
        } else {
            let message = match self
                .list_channel_id
                .send_message(&ctx.http, |m| m.content(text))
                .await
            {
                Ok(message) => message,
                Err(why) => {
                    eprintln!("Couldn't send list message: {why}");
                    return;
                }
            };

            *cache = Some(Cache {
                list_channel_id: self.list_channel_id,
                list_message_id: message.id,
            });

            let mut file = match tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(CACHE_FILE_NAME)
                .await
            {
                Ok(file) => file,
                Err(why) => {
                    eprintln!("Couldn't save cache file: {why}");
                    return;
                }
            };

            file.write_all(toml::to_string_pretty(cache).unwrap().as_bytes())
                .await
                .unwrap();
        }
    }
}

static LIST_REGEX: OnceCell<Regex> = OnceCell::new();
static TAG_REGEX: OnceCell<Regex> = OnceCell::new();
static NAUGHTY_REGEX: OnceCell<Regex> = OnceCell::new();
static NAUGHTY_NICKNAME: &str = "I MADE BOOL SAD";

#[derive(Serialize, Deserialize, Copy, Clone)]
struct Cache {
    list_channel_id: ChannelId,
    list_message_id: MessageId,
}

#[tokio::main]
async fn main() {
    let addr = env::var("RCON_ADDR").expect("Expected RCON_ADDR in the environment");
    let password = env::var("RCON_PASS").expect("Expected RCON_PASS in the environment");

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in the environment");

    let guild_id = GuildId(
        env::var("GUILD_ID")
            .expect("Expected GUILD_ID in environment")
            .parse()
            .expect("GUILD_ID must be an integer"),
    );

    let op_role_id = RoleId(
        env::var("OP_ROLE_ID")
            .expect("Expected OP_ROLE_ID in environment")
            .parse()
            .expect("OP_ROLE_ID must be an integer"),
    );

    let list_channel_id = ChannelId(
        env::var("LIST_CHANNEL_ID")
            .expect("Expected LIST_CHANNEL_ID in environment")
            .parse()
            .expect("LIST_CHANNEL_ID must be an integer"),
    );

    let has_list_json = env::var("HAS_LIST_JSON").is_ok();

    let server_directory = env::var("SERVER_DIR").expect("Expected SERVER_DIR in the environment");

    LIST_REGEX
        .set(
            Regex::new(r"^There are (\d+) of a max of (\d+) players online:(?: ((?:\w+, )*\w+))?$")
                .unwrap(),
        )
        .unwrap();
    TAG_REGEX
        .set(
            Regex::new(r"</?(?:color|c|yellow|dark_blue|dark_purple|gold|red|aqua|gray|light_purple|white|dark_gray|green|dark_green|blue|dark_aqua|dark_green|black|gradient|gr|rainbow|rb|reset)(?::[^>]*)?>")
                .unwrap(),
        )
        .unwrap();
    NAUGHTY_REGEX
        .set(
            Regex::new(&format!(
                r"(?u)`{0}`{0}`|h{0}t{0}t{0}p{0}s?{0}:{0}/{0}/|d{0}i{0}s{0}c{0}o{0}r{0}d{0}\.{0}g{0}g",
                r"[^\w!-_a-~]*"
            ))
            .unwrap(),
        )
        .unwrap();

    let mut cache: Option<Cache> = std::fs::read_to_string(CACHE_FILE_NAME)
        .ok()
        .and_then(|string| toml::from_str(&string).ok());

    if let Some(Cache {
        list_channel_id: cached_channel_id,
        ..
    }) = cache
    {
        if cached_channel_id != list_channel_id {
            cache = None;
        }
    }

    // Build our client.
    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler {
            interface: Arc::new(Mutex::new(interface::Interface::new(addr, password))),
            guild_id,
            op_role_id,
            list_channel_id,
            cache: Arc::new(Mutex::new(cache)),
            restart_scheduled: Arc::new(Mutex::new(false)),
            has_list_json,
            server_directory: server_directory.into_boxed_str(),
        })
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
