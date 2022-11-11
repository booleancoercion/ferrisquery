use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::interaction::application_command::{
    CommandDataOption, CommandDataOptionValue,
};

pub fn run(options: &[CommandDataOption]) -> String {
    let [CommandDataOption { resolved: Some(CommandDataOptionValue::String(string)), .. }] = options else {
        return "expected an argument, and it must be a string".into()
    };

    todo!()
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
