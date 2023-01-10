mod commands;
mod interface;
mod server_status;

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
}

impl Handler {
    pub fn new(
        addr: impl ToString,
        pass: impl ToString,
        guild_id: GuildId,
        op_role_id: RoleId,
        list_channel_id: ChannelId,
        cache: Option<Cache>,
        has_list_json: bool,
    ) -> Self {
        Self {
            interface: Arc::new(Mutex::new(interface::Interface::new(addr, pass))),
            guild_id,
            op_role_id,
            list_channel_id,
            cache: Arc::new(Mutex::new(cache)),
            restart_scheduled: Arc::new(Mutex::new(false)),
            has_list_json,
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
            commands
                .create_application_command(|command| commands::schedule_restart::register(command))
        })
        .await;

        println!(
            "I now have the following guild slash commands: {:#?}",
            commands
        );

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
                    let ServerStatus::Online(OnlineServerStatus {current_players, max_players, list}) = status else {
                        self.set_list_text(&ctx, "The server is offline.").await;

                        // also clear any scheduled restarts
                        *self.restart_scheduled.lock().await = false;
                        continue;
                    };

                    let text = format!(
                        "The server is online. There are {current_players}/{max_players} connected players: ```\n{}```", list.iter().map(|data| &*data.name).collect::<Vec<_>>().join("\n")
                    );
                    self.set_list_text(&ctx, &text).await;

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

    LIST_REGEX
        .set(
            Regex::new(r"^There are (\d+) of a max of (\d+) players online:(?: ((?:\w+, )*\w+))?$")
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
            has_list_json,
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