use flate2::{
    read::{GzDecoder, GzEncoder},
    Compression,
};
use poise::{serenity_prelude::CreateAttachment, CreateReply};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{collections::HashMap, fs::OpenOptions, io::Read, path::PathBuf};

use fastnbt::Value;
use uuid_mc::{PlayerUuid, Uuid};

use crate::{commands::get_uuid, Context, Error};

/// Teleport an offline player.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
pub async fn tp_offline(
    ctx: Context<'_>,
    #[description = "Name or UUID of the player"] player: String,
    #[description = "Whether it's an online user or an offline one."] mode: super::OfflineOnline,
    #[description = "X coordinate"] x: f64,
    #[description = "Y coordinate"] y: f64,
    #[description = "Z coordinate"] z: f64,
    #[description = "Dimension ID"] dimension: Option<String>,
) -> Result<(), Error> {
    let uuid = match Uuid::parse_str(&player) {
        Ok(uuid) => PlayerUuid::new_with_uuid(uuid)?,
        Err(_) => get_uuid(&player, mode).await?,
    };

    let path = {
        let mut filename = uuid.as_uuid().hyphenated().to_string();
        filename.push_str(".dat");
        PathBuf::from(&*ctx.data().server_directory)
            .join("world")
            .join("playerdata")
            .join(filename)
    };

    if !path.try_exists().unwrap_or(false) {
        return Err(
            format!("{path:?} does not exist. Has this player joined the game before?").into(),
        );
    }

    let (original_bytes, data) = tokio::task::spawn_blocking(move || {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .append(false)
            .create(false)
            .open(&path)
            .map_err(Error::from)?;

        if file.try_lock().is_err() {
            return Err(Error::from(
                "{path:?} is already open. Is the player currently online?",
            ));
        }

        let mut original_bytes = Vec::with_capacity(2048);
        file.read_to_end(&mut original_bytes)?;

        let data = {
            let mut uncompressed = Vec::with_capacity(2048);
            let mut reader = GzDecoder::new(&*original_bytes);
            reader.read_to_end(&mut uncompressed)?;
            uncompressed
        };

        let mut data = fastnbt::from_bytes::<PlayerData>(&data)?;

        if data.pos.len() != 3 {
            return Err(format!("Expected Pos to be a list of 3 64-bit floating point numbers but found {:?} instead.", data.pos).into());
        }

        data.pos = vec![x, y, z];
        if let Some(dimension) = dimension {
            data.dimension = dimension;
        }

        let bytes = fastnbt::to_bytes(&data)?;
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(&bytes)?;

        Ok((original_bytes, data))
    })
    .await??;

    ctx.send(
        CreateReply::default()
            .content(format!(
                "Teleported {} to {} {} {} in {}",
                uuid.as_uuid().as_hyphenated(),
                x,
                y,
                z,
                data.dimension
            ))
            .attachment(CreateAttachment::bytes(original_bytes, "backup.dat")),
    )
    .await?;

    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct PlayerData {
    pub pos: Vec<f64>,
    pub dimension: String,

    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}
