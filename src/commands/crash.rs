use std::path::{Path, PathBuf};
use std::time::SystemTime;

use poise::serenity_prelude::CreateAttachment;
use poise::CreateReply;
use tokio::fs;

use crate::{Context, Error};

/// Upload the latest crash report.
#[poise::command(slash_command, global_cooldown = 30)]
pub async fn crash(ctx: Context<'_>) -> Result<(), Error> {
    let mut path = PathBuf::from(&*ctx.data().server_directory);
    path.push("crash-reports");

    let (file, created) = get_latest_file(path).await?;
    let created = created
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    ctx.send(
        CreateReply::default()
            .attachment(CreateAttachment::path(&file).await?)
            .content(format!("This file was created <t:{created}:R>.",)),
    )
    .await?;

    Ok(())
}

async fn get_latest_file(
    directory: impl AsRef<Path>,
) -> Result<(PathBuf, SystemTime), tokio::io::Error> {
    let mut entries = vec![];
    let mut read_dir = fs::read_dir(directory).await?;

    while let Some(dir_entry) = read_dir.next_entry().await? {
        let metadata = dir_entry.metadata().await?;
        if !metadata.is_file() {
            continue;
        }
        entries.push((dir_entry, metadata.created()?))
    }

    entries.sort_unstable_by_key(|(_, created)| std::cmp::Reverse(*created));

    Ok((entries[0].0.path(), entries[0].1))
}
