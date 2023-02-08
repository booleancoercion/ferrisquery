mod crash;
mod run;
mod schedule_restart;
mod source;
mod whitelist;

pub use crash::crash;
pub use run::run;
pub use schedule_restart::schedule_restart;
pub use source::source;
pub use whitelist::whitelist;

async fn operator_only(ctx: crate::Context<'_>) -> Result<bool, crate::Error> {
    let Some(member) = ctx.author_member().await else {
        return Ok(false);
    };

    if member.roles.contains(&ctx.data().op_role_id) {
        Ok(true)
    } else {
        ctx.say("You're not an op!").await?;
        Ok(false)
    }
}

#[derive(poise::ChoiceParameter)]
enum OfflineOnline {
    Offline,
    Online,
}
