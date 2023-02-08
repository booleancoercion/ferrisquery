use crate::{Context, Error};

/// Get a link to the bot's source code.
#[poise::command(slash_command)]
pub async fn source(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("<https://github.com/booleancoercion/ferrisquery>")
        .await?;
    Ok(())
}
