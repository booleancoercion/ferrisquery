use std::borrow::Cow;
use std::fmt::Write;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use uuid_mc::{PlayerUuid, Uuid};

use crate::{Context, Error};

#[derive(Serialize, Deserialize)]
struct WhitelistEntry<'a> {
    name: Cow<'a, str>,
    uuid: PlayerUuid,
}

async fn get_whitelist(server_directory: &str) -> Result<Vec<WhitelistEntry>, Error> {
    let filename = format!("{server_directory}/whitelist.json");
    let raw_json = tokio::fs::read_to_string(&filename).await?;
    let mut whitelist: Vec<WhitelistEntry> = serde_json::from_str(&raw_json)?;

    whitelist.sort_unstable_by(|e1, e2| e1.name.cmp(&e2.name));

    Ok(whitelist)
}

async fn save_whitelist(ctx: &Context<'_>, whitelist: &[WhitelistEntry<'_>]) -> Result<(), Error> {
    let filename = format!("{}/whitelist.json", ctx.data().server_directory);

    let raw_json = serde_json::to_string_pretty(whitelist).unwrap(); // this serialization cannot fail
    let mut file = tokio::fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(&filename)
        .await?;
    file.write_all(raw_json.as_bytes()).await?;

    // we don't care if the command succeeds, because then that means the server
    // is offline and so the whitelist will be reloaded anyway when it comes online.
    let mut interface = ctx.data().interface.lock().await;
    let _ = interface.exec("whitelist reload").await;

    Ok(())
}

async fn get_easyauth_config(server_directory: &str) -> Result<Value, Error> {
    let filename = format!("{server_directory}/mods/EasyAuth/config.json");
    let raw_json = tokio::fs::read_to_string(&filename).await?;

    Ok(serde_json::from_str(&raw_json)?)
}

async fn save_easyauth_config(ctx: &Context<'_>, config: &Value) -> Result<(), Error> {
    let filename = format!("{}/mods/EasyAuth/config.json", ctx.data().server_directory);

    let raw_json = serde_json::to_string_pretty(config).unwrap(); // this serialization cannot fail
    let mut file = tokio::fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(&filename)
        .await?;
    file.write_all(raw_json.as_bytes()).await?;

    // we don't care if the command succeeds, because then that means the server
    // is offline and so the whitelist will be reloaded anyway when it comes online.
    let mut interface = ctx.data().interface.lock().await;
    let _ = interface.exec("auth reload").await;

    Ok(())
}

#[poise::command(slash_command, subcommands("add", "remove", "list"))]
pub async fn whitelist(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add a user to the whitelist.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
async fn add(
    ctx: Context<'_>,
    #[description = "The minecraft user to be added."] username: String,
    #[description = "The associated discord user."] discord: poise::serenity_prelude::User,
    #[description = "Whether the user uses online or offline mode."] mode: super::OfflineOnline,
) -> Result<(), Error> {
    let db_api = ctx.data().db_api.as_ref();

    // Adding to the whitelist file
    let mut whitelist = get_whitelist(&ctx.data().server_directory).await?;
    if whitelist.iter().any(|entry| entry.name == username) {
        ctx.say(format!("The user {username} is already in the whitelist."))
            .await?;
        return Ok(());
    }

    let uuid = super::get_uuid(&username, mode).await?;

    whitelist.push(WhitelistEntry {
        name: username.as_str().into(),
        uuid,
    });
    save_whitelist(&ctx, &whitelist).await?;

    // Modifying the EasyAuth config, if necessary
    if ctx.data().has_easyauth && mode == super::OfflineOnline::Offline {
        let mut config = get_easyauth_config(&ctx.data().server_directory).await?;
        let Some(forced_offline_players) = config
            .get_mut("main")
            .and_then(|x| x.get_mut("forcedOfflinePlayers"))
            .and_then(Value::as_array_mut)
        else {
            return Err("couldn't get the forcedOfflinePlayers entry".into());
        };

        let lowercase_username = username.to_lowercase();

        if forced_offline_players
            .iter()
            .all(|v| v.as_str().unwrap() != lowercase_username)
        {
            forced_offline_players.push(Value::String(lowercase_username))
        }

        save_easyauth_config(&ctx, &config).await?;
    }

    // Saving to monad's database
    let db_result = if let Some(db_api) = db_api {
        db_api
            .insert_user_with_name(discord.id, &username, mode.is_online())
            .await
    } else {
        Ok(())
    };

    let mut output = format!("Player {username} added to the whitelist.");
    if db_result.is_err() {
        output += "\nDB Error - see log for details.";
    }

    ctx.say(output).await?;

    Ok(db_result?)
}

#[poise::command(
    slash_command,
    subcommands("remove_with_mc_username", "remove_with_mc_uuid")
)]
async fn remove(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

async fn remove_mc_inner<'a>(
    ctx: Context<'_>,
    condition: impl Fn(&WhitelistEntry) -> bool,
) -> Result<(), Error> {
    let db_api = ctx.data().db_api.as_ref();

    // Removing from the whitelist file
    let mut whitelist = get_whitelist(&ctx.data().server_directory).await?;
    let entry = whitelist.iter().find(|entry| condition(entry));
    let Some(entry) = entry else {
        ctx.say("That user is not in the whitelist.").await?;
        return Ok(());
    };

    let username = entry.name.clone();
    let uuid = entry.uuid;

    whitelist.retain(|entry| entry.name != username);

    save_whitelist(&ctx, &whitelist).await?;

    // Removign from the EasyAuth config, if necessary
    if ctx.data().has_easyauth && uuid.offline().is_some() {
        let mut config = get_easyauth_config(&ctx.data().server_directory).await?;
        let Some(forced_offline_players) = config
            .get_mut("main")
            .and_then(|x| x.get_mut("forcedOfflinePlayers"))
            .and_then(Value::as_array_mut)
        else {
            return Err("couldn't get the forcedOfflinePlayers entry".into());
        };

        let lowercase_username = username.to_lowercase();
        forced_offline_players.retain(|v| v.as_str().unwrap() != lowercase_username);

        save_easyauth_config(&ctx, &config).await?;
    }

    // Removing from monad's database
    let db_result = if let Some(db_api) = db_api {
        db_api.delete_user_with_minecraft(uuid).await
    } else {
        Ok(())
    };
    let mut output = format!("Player {username} removed from the whitelist.");
    if db_result.is_err() {
        output += "\nDB Error - see log for details.";
    }
    ctx.say(output).await?;

    Ok(db_result?)
}

/// Remove a user from the whitelist using their minecraft username.
#[poise::command(
    slash_command,
    guild_only,
    check = "super::operator_only",
    rename = "with_mc_username"
)]
async fn remove_with_mc_username(ctx: Context<'_>, username: String) -> Result<(), Error> {
    remove_mc_inner(ctx, |entry| entry.name == username).await
}

#[poise::command(
    slash_command,
    guild_only,
    check = "super::operator_only",
    rename = "with_mc_uuid"
)]
async fn remove_with_mc_uuid(ctx: Context<'_>, uuid: String) -> Result<(), Error> {
    let Ok(Ok(uuid)) = Uuid::try_parse(&uuid).map(PlayerUuid::new_with_uuid) else {
        ctx.say("The provided UUID is invalid.").await?;
        return Ok(());
    };
    remove_mc_inner(ctx, |entry| entry.uuid == uuid).await
}

/// Return the list of whitelisted players.
#[poise::command(slash_command, guild_only)]
async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let whitelist = get_whitelist(&ctx.data().server_directory).await?;

    let mut result = format!("There are {} whitelisted players", whitelist.len());
    if whitelist.is_empty() {
        write!(&mut result, ".").unwrap();
    } else {
        #[allow(unstable_name_collisions)]
        write!(
            &mut result,
            ":\n```\n{}\n```",
            whitelist
                .iter()
                .map(|entry| &*entry.name)
                .intersperse(", ")
                .collect::<String>()
        )
        .unwrap();
    }

    ctx.say(result).await?;

    Ok(())
}
