use crate::{Context, Error};

const MAXLEN: usize = 2000 - "Success:\n```\n\n```".len();
const CUTOFF_SUFFIX: &str = "...";

/// Run an admin command from the console.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
pub async fn run(
    ctx: Context<'_>,
    #[description = "The command to run"] cmd: String,
) -> Result<(), Error> {
    let interface = &mut *ctx.data().interface.lock().await;

    match interface.exec(&cmd).await {
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
            ctx.say(format!("Success:\n```\n{response}\n```")).await?;
        }
        Err(rcon::Error::Auth) => {
            ctx.say("Invalid authentication (check bot config).")
                .await?;
        }
        Err(rcon::Error::CommandTooLong) => {
            ctx.say("Command too long.").await?;
        }
        Err(rcon::Error::Io(..)) => {
            ctx.say("The server is closed.").await?;
        }
    }

    Ok(())
}
