mod commands;
mod interface;

use std::borrow::Cow;
use std::env;
use std::sync::Arc;

use serenity::async_trait;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::RoleId;
use serenity::prelude::*;

struct Handler {
    interface: Arc<Mutex<interface::Interface>>,
    guild_id: GuildId,
    op_role_id: RoleId,
}

impl Handler {
    pub fn new(
        addr: impl ToString,
        pass: impl ToString,
        guild_id: GuildId,
        op_role_id: RoleId,
    ) -> Self {
        Self {
            interface: Arc::new(Mutex::new(interface::Interface::new(addr, pass))),
            guild_id,
            op_role_id,
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
    }
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

    // Build our client.
    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler::new(addr, password, guild_id, op_role_id))
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
