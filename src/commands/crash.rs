use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use once_cell::sync::OnceCell;
use serenity::builder::CreateApplicationCommand;
use tokio::fs;
use tokio::sync::Mutex;

static LAST_USED: OnceCell<Mutex<Instant>> = OnceCell::new();
const RATELIMIT: Duration = Duration::from_secs(30);

pub async fn run(server_dir: &str) -> Result<(PathBuf, SystemTime), Cow<'static, str>> {
    let last_used = &mut *LAST_USED
        .get_or_init(|| Mutex::new(Instant::now().checked_sub(RATELIMIT * 2).unwrap()))
        .lock()
        .await;

    let elapsed = last_used.elapsed();
    if elapsed < RATELIMIT {
        Err(format!(
            "Please wait at least {:.5} more seconds before using this command again.",
            (RATELIMIT - elapsed).as_secs_f64()
        )
        .into())
    } else {
        let mut path = PathBuf::from(server_dir);
        path.push("crash-reports");

        match get_latest_file(path).await {
            Ok(file) => {
                *last_used = Instant::now();
                Ok(file)
            }
            Err(err) => {
                eprintln!("/crash failed with error: {err:?}");
                Err("Could not read crash log file.".into())
            }
        }
    }
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

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
    command
        .name("crash")
        .description("Upload the latest crash report.")
}
