use std::borrow::Cow;

use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::interaction::application_command::{
    CommandDataOption, CommandDataOptionValue,
};

use crate::interface::Interface;

const MAXLEN: usize = 2000 - "Success:\n```\n\n```".len();
const CUTOFF_SUFFIX: &str = "...";

pub async fn run(interface: &mut Interface, options: &[CommandDataOption]) -> Cow<'static, str> {
    let [CommandDataOption { resolved: Some(CommandDataOptionValue::String(command)), .. }] = options else {
        return Cow::Borrowed("expected an argument, and it must be a string")
    };

    match interface.exec(command).await {
        Ok(response) => {
            let response = if response.chars().count() > MAXLEN {
                response
                    .chars()
                    .take(MAXLEN - CUTOFF_SUFFIX.len())
                    .chain(CUTOFF_SUFFIX.chars())
                    .collect()
            } else {
                response
            };
            Cow::Owned(format!("Success:\n```\n{response}\n```"))
        }
        Err(rcon::Error::Auth) => Cow::Borrowed("Invalid authentication (check bot config)."),
        Err(rcon::Error::CommandTooLong) => Cow::Borrowed("Command too long."),
        Err(rcon::Error::Io(..)) => Cow::Borrowed("The server is closed."),
    }
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
    command
        .name("run")
        .description("Run an admin command from the console.")
        .create_option(|option| {
            option
                .name("cmd")
                .description("The command to run")
                .kind(CommandOptionType::String)
                .required(true)
        })
}
