use crate::{Context, Error};

/// Schedule a server restart as soon as everyone logs off.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
pub async fn schedule_restart(
    ctx: Context<'_>,
    #[description = "Whether or not to cancel an already scheduled restart."] cancel: Option<bool>,
) -> Result<(), Error> {
    let cancel = cancel.unwrap_or(false);
    let restart_scheduled = &mut *ctx.data().restart_scheduled.lock().await;

    if cancel && *restart_scheduled {
        *restart_scheduled = false;
        ctx.say("The scheduled restart has been cancelled.").await?;
    } else if cancel {
        ctx.say("There is no restart scheduled.").await?;
    } else if *restart_scheduled {
        ctx.say("There is already a restart scheduled.").await?;
    } else {
        *restart_scheduled = true;
        ctx.say("A restart has been scheduled - it will occur as soon as everyone logs off.")
            .await?;
    }
    Ok(())
}
