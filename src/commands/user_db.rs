use crate::{database_api, Context, Error};

pub async fn db_available(ctx: Context<'_>) -> Result<bool, Error> {
    Ok(ctx.data().db_api.is_some())
}

/// Manipulate the user database directly. This usually isn't necessary.
#[poise::command(slash_command, subcommands("fetch"))]
pub async fn user_db(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Fetch information from the database.
#[poise::command(slash_command, subcommands("with_mc", "with_discord"))]
async fn fetch(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Fetch a profile using a minecraft username.
#[poise::command(slash_command, check = "super::operator_only", check = "db_available")]
async fn with_mc(
    ctx: Context<'_>,
    #[description = "The minecraft username."] mc_name: String,
    #[description = "Whether it's an online user or an offline one."] mode: super::OfflineOnline,
) -> Result<(), Error> {
    let db_api = ctx.data().db_api.as_ref().unwrap();

    let uuid = super::get_uuid(&mc_name, mode).await?;
    let user = match db_api.get_users_with_minecraft(uuid).await {
        Ok(user) => user,
        Err(database_api::Error::Unsuccessful(response))
            if response.status() == reqwest::StatusCode::NOT_FOUND =>
        {
            ctx.say("User not found.").await?;
            return Ok(());
        }
        Err(why) => return Err(why.into()),
    };

    ctx.send(|reply| {
        reply
            .content(user.pretty_string())
            .allowed_mentions(|mentions| mentions.empty_users().empty_roles())
    })
    .await?;

    Ok(())
}

/// Fetch a profile using a discord user.
#[poise::command(slash_command, check = "super::operator_only", check = "db_available")]
async fn with_discord(
    ctx: Context<'_>,
    #[description = "The discord user."] user: poise::serenity_prelude::User,
) -> Result<(), Error> {
    let db_api = ctx.data().db_api.as_ref().unwrap();

    let user = match db_api.get_users_with_discord(user.id).await {
        Ok(user) => user,
        Err(database_api::Error::Unsuccessful(response))
            if response.status() == reqwest::StatusCode::NOT_FOUND =>
        {
            ctx.say("User not found.").await?;
            return Ok(());
        }
        Err(why) => return Err(why.into()),
    };

    ctx.send(|reply| {
        reply
            .content(user.pretty_string())
            .allowed_mentions(|mentions| mentions.empty_users().empty_roles())
    })
    .await?;

    Ok(())
}
