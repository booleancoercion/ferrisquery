use flate2::{
    read::{GzDecoder, GzEncoder},
    Compression,
};
use poise::{
    serenity_prelude::{
        futures::{self, Stream},
        CreateAttachment,
    },
    CreateReply,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::OpenOptions, io::Read, path::PathBuf, str::FromStr};
use std::{io::Write, path::Path};

use fastnbt::Value;
use uuid_mc::{PlayerUuid, Uuid};

use crate::{commands::get_uuid, Context, Error};

async fn autocomplete_dimension<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Iterator<Item = &'static str> + 'a {
    const KNOWN_DIMENSIONS: [&str; 3] = [
        "minecraft:overworld",
        "minecraft:the_nether",
        "minecraft:the_end",
    ];

    KNOWN_DIMENSIONS
        .into_iter()
        .filter(move |s| s.contains(partial))
}

async fn autocomplete_offline_player_uuid(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let dir = ctx.data().server_directory.clone();

    let Ok(Some(file_uuids)) = tokio::task::spawn_blocking(move || {
        Some(
            std::fs::read_dir(PathBuf::from_str(&dir).ok()?)
                .ok()?
                .filter_map(Result::ok)
                .filter_map(|e| {
                    e.file_name()
                        .to_str()
                        .and_then(|s| s.strip_suffix(".dat"))
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>(),
        )
    })
    .await
    else {
        return vec![];
    };

    let Ok(players) = ctx.data().interface.lock().await.player_list().await else {
        return vec![];
    };

    file_uuids
        .into_iter()
        .filter(|u| u.contains(partial))
        .filter_map(|u| uuid_mc::Uuid::from_str(&u).ok())
        .filter(|d| !players.iter().any(|p| p.uuid.as_uuid() == d))
        .map(|u| u.as_hyphenated().to_string())
        .collect()
}

/// Teleport an offline player.
#[poise::command(slash_command, guild_only, check = "super::operator_only")]
pub async fn tp_offline(
    ctx: Context<'_>,
    #[description = "Name or UUID of the player"]
    #[autocomplete = "autocomplete_offline_player_uuid"]
    player: String,
    #[description = "X coordinate"] x: f64,
    #[description = "Y coordinate"] y: f64,
    #[description = "Z coordinate"] z: f64,
    #[description = "Dimension ID"]
    #[autocomplete = "autocomplete_dimension"]
    dimension: Option<String>,
) -> Result<(), Error> {
    let uuid = PlayerUuid::new_with_uuid(Uuid::parse_str(&player)?)?;

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
        if let Some(ref dimension) = dimension {
            data.dimension = dimension.clone();
        }

        if let Some(vehicle) = &mut data.root_vehicle {
            if vehicle.entity.pos.len() != 3 {
                return Err(format!("Expected RootVehicle.Entity.Pos to be a list of 3 64-bit floating point numbers but found {:?} instead.", vehicle.entity.pos).into());
            }

            vehicle.entity.pos = vec![x, y, z];
            if let Some(dimension) = dimension {
                vehicle.entity.dimension = dimension;
            }
        }

        let bytes = fastnbt::to_bytes(&data)?;

        let temp_path = path.with_added_extension(".tmp");
        let temp_file = OpenOptions::new()
            .read(true)
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(Error::from)?;

        let mut encoder = GzEncoder::new(&temp_file, Compression::fast());

        encoder.write_all(&bytes)?;

        // Drop and unlock the original .dat file
        drop(file);

        std::fs::rename(temp_path, &path)?;

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

    #[serde(default)]
    pub root_vehicle: Option<RootVehicle>,

    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct RootVehicle {
    pub entity: Entity,

    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct Entity {
    pub pos: Vec<f64>,
    pub dimension: String,

    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}
