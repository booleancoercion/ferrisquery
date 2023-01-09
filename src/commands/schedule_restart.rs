use std::borrow::Cow;

use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::interaction::application_command::{
    CommandDataOption, CommandDataOptionValue,
};

pub async fn run(restart_scheduled: &mut bool, options: &[CommandDataOption]) -> Cow<'static, str> {
    if let [CommandDataOption {
        resolved: Some(CommandDataOptionValue::Boolean(cancel)),
        ..
    }] = options
    {
        if *cancel {
            return if *restart_scheduled {
                *restart_scheduled = false;
                Cow::Borrowed("The scheduled restart has been cancelled.")
            } else {
                Cow::Borrowed("There is no restart scheduled.")
            };
        }
    }

    if *restart_scheduled {
        Cow::Borrowed("There is already a restart scheduled.")
    } else {
        *restart_scheduled = true;
        Cow::Borrowed("A restart has been scheduled - it will occur as soon as everyone logs off.")
    }
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
    command
        .name("schedule_restart")
        .description("Schedule a server restart as soon as everyone logs off.")
        .create_option(|option| {
            option
                .name("cancel")
                .description("Cancel a scheduled restart")
                .kind(CommandOptionType::Boolean)
                .required(false)
        })
}
