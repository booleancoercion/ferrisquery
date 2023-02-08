mod commands;
mod database_api;
mod env;
mod interface;
mod server_status;

use std::fmt::Write;
use std::sync::Arc;

use database_api::MonadApi;
use once_cell::sync::OnceCell;
use poise::serenity_prelude as serenity;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serenity::{ChannelId, MessageId, RoleId};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::server_status::{OnlineServerStatus, ServerStatus};

const CACHE_FILE_NAME: &str = "ferrisquery_cache.toml";

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Clone)]
pub struct Data {
    interface: Arc<Mutex<interface::Interface>>,
    op_role_id: RoleId,
    list_channel_id: ChannelId,
    cache: Arc<Mutex<Option<Cache>>>,
    restart_scheduled: Arc<Mutex<bool>>,
    has_list_json: bool,
    server_directory: Box<str>,
    db_api: Option<Arc<MonadApi>>,
}

async fn list_updater(data: Data, http: Arc<poise::serenity_prelude::Http>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    loop {
        interval.tick().await;

        let status =
            server_status::get_server_status(&mut *data.interface.lock().await, data.has_list_json)
                .await;

        match status {
            Ok(status) => {
                let ServerStatus::Online(OnlineServerStatus {current_players, max_players, list, tps}) = status else {
                        set_list_text(&data, &http, "The server is offline.").await;

                        // also clear any scheduled restarts
                        *data.restart_scheduled.lock().await = false;
                        continue;
                    };

                let regex = TAG_REGEX.get().unwrap();
                let naughty_regex = NAUGHTY_REGEX.get().unwrap();
                let mut naughty = Vec::new();
                let players = list
                    .iter()
                    .map(|data| {
                        if let Some(nick) = &data.nickname {
                            let nick = regex.replace_all(nick, "");
                            if naughty_regex.is_match(&nick) {
                                naughty.push(&data.name);
                                format!("{} ({})", data.name, NAUGHTY_NICKNAME)
                            } else {
                                format!("{} ({})", data.name, nick)
                            }
                        } else {
                            data.name.to_string()
                        }
                    })
                    .collect::<Vec<_>>();

                let mut text = format!(
                        "The server is online. There are {current_players}/{max_players} connected players"
                    );

                if current_players > 0 {
                    write!(&mut text, ": ```\n{}```", players.join("\n")).unwrap();
                } else {
                    writeln!(&mut text, ".").unwrap();
                }

                if let Some(tps) = tps {
                    write!(&mut text, "\nTPS info: ```\n5s    10s   1m    5m    15m  \n{:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2}```", tps[0], tps[1], tps[2], tps[3], tps[4]).unwrap();
                }
                set_list_text(&data, &http, &text).await;

                {
                    let mut interface = data.interface.lock().await;
                    for name in naughty {
                        let _ = interface
                            .exec(&format!("styled-nicknames set {name} {NAUGHTY_NICKNAME}"))
                            .await;
                        let _ = interface.exec(&format!("kick {name} nice try")).await;
                    }
                }

                // if a restart has been scheduled and there are no players online, do it
                if current_players == 0 && *data.restart_scheduled.lock().await {
                    let _ = data.interface.lock().await.exec("stop").await;
                }
            }
            Err(why) => {
                set_list_text(&data, &http, &why).await;
            }
        }
    }
}

async fn set_list_text(data: &Data, http: &poise::serenity_prelude::Http, text: &str) {
    let cache = &mut *data.cache.lock().await;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let text = format!("{text}\n\nLast update: <t:{timestamp}:T>");

    if let Some(Cache {
        list_channel_id,
        list_message_id,
    }) = *cache
    {
        if let Err(why) = list_channel_id
            .edit_message(http, list_message_id, |m| m.content(text))
            .await
        {
            eprintln!("Couldn't edit list message: {why}")
        }
    } else {
        let message = match data
            .list_channel_id
            .send_message(http, |m| m.content(text))
            .await
        {
            Ok(message) => message,
            Err(why) => {
                eprintln!("Couldn't send list message: {why}");
                return;
            }
        };

        *cache = Some(Cache {
            list_channel_id: data.list_channel_id,
            list_message_id: message.id,
        });

        let mut file = match tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(CACHE_FILE_NAME)
            .await
        {
            Ok(file) => file,
            Err(why) => {
                eprintln!("Couldn't save cache file: {why}");
                return;
            }
        };

        file.write_all(toml::to_string_pretty(cache).unwrap().as_bytes())
            .await
            .unwrap();
    }
}

static LIST_REGEX: OnceCell<Regex> = OnceCell::new();
static TAG_REGEX: OnceCell<Regex> = OnceCell::new();
static NAUGHTY_REGEX: OnceCell<Regex> = OnceCell::new();
static NAUGHTY_NICKNAME: &str = "I MADE BOOL SAD";

#[derive(Serialize, Deserialize, Copy, Clone)]
struct Cache {
    list_channel_id: ChannelId,
    list_message_id: MessageId,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    if env::any_set() {
        env::assert_env_vars();
    } else {
        eprintln!("# Environment Variables Help\n{}", env::gen_help());
        std::process::exit(1);
    }

    let addr = env::rcon_addr();
    let password = env::rcon_pass();
    let token = env::discord_token();
    let op_role_id = RoleId(env::op_role_id());
    let list_channel_id = ChannelId(env::list_channel_id());
    let has_list_json = env::has_list_json().is_some();
    let server_directory = env::server_directory();

    let db_api = || -> Option<MonadApi> {
        Some(MonadApi::new(
            &env::db_username()?,
            &env::db_admin_endpoint()?,
            &env::db_admin_password()?,
            &env::db_user_endpoint()?,
            &env::db_user_password()?,
        ))
    }();

    LIST_REGEX
        .set(
            Regex::new(r"^There are (\d+) of a max of (\d+) players online:(?: ((?:\w+, )*\w+))?$")
                .unwrap(),
        )
        .unwrap();
    TAG_REGEX
        .set(
            Regex::new(r"</?(?:color|c|yellow|dark_blue|dark_purple|gold|red|aqua|gray|light_purple|white|dark_gray|green|dark_green|blue|dark_aqua|dark_green|black|gradient|gr|rainbow|rb|reset)(?::[^>]*)?>")
                .unwrap(),
        )
        .unwrap();
    NAUGHTY_REGEX
        .set(
            Regex::new(&format!(
                r"(?u)`{0}`{0}`|h{0}t{0}t{0}p{0}s?{0}:{0}/{0}/|d{0}i{0}s{0}c{0}o{0}r{0}d{0}\.{0}g{0}g",
                r"[^\w!-_a-~]*"
            ))
            .unwrap(),
        )
        .unwrap();

    let mut cache: Option<Cache> = std::fs::read_to_string(CACHE_FILE_NAME)
        .ok()
        .and_then(|string| toml::from_str(&string).ok());

    if let Some(Cache {
        list_channel_id: cached_channel_id,
        ..
    }) = cache
    {
        if cached_channel_id != list_channel_id {
            cache = None;
        }
    }

    poise::Framework::builder()
        .token(token)
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::source(),
                commands::schedule_restart(),
                commands::run(),
                commands::crash(),
                commands::whitelist(),
            ],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let data = Data {
                    interface: Arc::new(Mutex::new(interface::Interface::new(addr, password))),
                    op_role_id,
                    list_channel_id,
                    cache: Arc::new(Mutex::new(cache)),
                    restart_scheduled: Arc::new(Mutex::new(false)),
                    has_list_json,
                    server_directory: server_directory.into_boxed_str(),
                    db_api: db_api.map(Arc::new),
                };

                let _data = data.clone();
                let _http = Arc::clone(&ctx.http);

                tokio::spawn(async move { list_updater(_data, _http).await });

                Ok(data)
            })
        })
        .intents(poise::serenity_prelude::GatewayIntents::non_privileged())
        .run()
        .await
        .unwrap();
}
