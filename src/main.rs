mod commands;
mod interface;

use std::borrow::Cow;
use std::env;
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

const CACHE_FILE_NAME: &str = "ferrisquery_cache.toml";

struct Handler {
    interface: Arc<Mutex<interface::Interface>>,
    guild_id: GuildId,
    op_role_id: RoleId,
    list_channel_id: ChannelId,
    cache: Arc<Mutex<Option<Cache>>>,
}

impl Handler {
    pub fn new(
        addr: impl ToString,
        pass: impl ToString,
        guild_id: GuildId,
        op_role_id: RoleId,
        list_channel_id: ChannelId,
        cache: Option<Cache>,
    ) -> Self {
        Self {
            interface: Arc::new(Mutex::new(interface::Interface::new(addr, pass))),
            guild_id,
            op_role_id,
            list_channel_id,
            cache: Arc::new(Mutex::new(cache)),
        }
    }
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
            commands.create_application_command(|command| commands::source::register(command))
        })
        .await;

        println!(
            "I now have the following guild slash commands: {:#?}",
            commands
        );

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        let regex = LIST_REGEX.get().unwrap();
        loop {
            interval.tick().await;

            let list = self.interface.lock().await.exec("list").await;
            if let Ok(list) = list {
                let list = list.trim();
                let Some(captures) = regex.captures(list) else {
                    self.set_list_text(&ctx, "Regex error (this is a bug)").await;
                    eprintln!("Regex error. Response from server: {list}");
                    continue
                };

                let mut captures = captures.iter().flatten().map(|mat| mat.as_str());
                captures.next().unwrap(); // this is the entire text, not interesting

                // these unwraps are ok, because we know we're getting a well-formatted response from the server
                let online_players: i32 = captures.next().unwrap().parse().unwrap();
                let max_players: i32 = captures.next().unwrap().parse().unwrap();

                let mut players: Vec<&str> = captures.collect();
                players.sort_unstable();

                let text = format!(
                    "The server is online. There are {online_players}/{max_players} connected players: ```\n{}```", players.join("\n")
                );

                self.set_list_text(&ctx, &text).await;
            } else {
                // if there's an error, it can't be a CommandTooLong. therefore, the server must be offline.
                self.set_list_text(&ctx, "The server is offline.").await;
            }
        }
    }
}

impl Handler {
    async fn set_list_text(&self, ctx: &Context, text: &str) {
        let cache = &mut *self.cache.lock().await;

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

#[derive(Serialize, Deserialize, Copy, Clone)]
struct Cache {
    list_channel_id: ChannelId,
    list_message_id: MessageId,
}

#[tokio::main]
async fn main() {
    let addr = env::var("RCON_ADDR").expect("Expected an rcon address in the environment");
    let password = env::var("RCON_PASS").expect("Expected an rcon password in the environment");

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

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

    LIST_REGEX
        .set(
            Regex::new(
                r"^There are (\d+) of a max of (\d+) players online:(?: (?:(\w+), )*(\w+))?$",
            )
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
        .event_handler(Handler::new(
            addr,
            password,
            guild_id,
            op_role_id,
            list_channel_id,
            cache,
        ))
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
