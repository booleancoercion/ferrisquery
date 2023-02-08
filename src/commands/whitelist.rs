use std::borrow::Cow;
use std::fmt::Write;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid_mc::PlayerUuid;

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

/// Make changes to the whitelist that also modify related data, like the user database.
#[poise::command(slash_command, subcommands("add", "remove", "list"))]
pub async fn whitelist(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add a user to the whitelist.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
async fn add(
    ctx: Context<'_>,
    #[description = "The user to be added."] username: String,
    #[description = "Whether the user uses online or offline mode."] mode: super::OfflineOnline,
) -> Result<(), Error> {
    let mut whitelist = get_whitelist(&ctx.data().server_directory).await?;
    if whitelist.iter().any(|entry| entry.name == username) {
        ctx.say(format!("The user {username} is already in the whitelist."))
            .await?;
        return Ok(());
    }

    let _username = username.clone();
    let uuid = match mode {
        super::OfflineOnline::Online => {
            tokio::task::spawn_blocking(move || PlayerUuid::new_with_online_username(&_username))
                .await
                .unwrap()?
        }
        super::OfflineOnline::Offline => PlayerUuid::new_with_offline_username(&_username),
    };

    whitelist.push(WhitelistEntry {
        name: username.as_str().into(),
        uuid,
    });

    save_whitelist(&ctx, &whitelist).await?;
    ctx.say(format!("Player {username} added to the whitelist."))
        .await?;

    Ok(())
}

/// Remove a player from the whitelist.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
async fn remove(
    ctx: Context<'_>,
    #[description = "The user to be removed."] username: String,
) -> Result<(), Error> {
    let mut whitelist = get_whitelist(&ctx.data().server_directory).await?;

    let len = whitelist.len();
    whitelist.retain(|entry| entry.name != username);
    if whitelist.len() >= len {
        ctx.say(format!("The player {username} is not in the whitelist."))
            .await?;
        return Ok(());
    }

    save_whitelist(&ctx, &whitelist).await?;
    ctx.say(format!("Player {username} removed from the whitelist."))
        .await?;

    Ok(())
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
