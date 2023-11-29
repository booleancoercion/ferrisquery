mod crash;
mod run;
mod schedule_restart;
mod source;
mod user_db;
mod whitelist;

pub use crash::crash;
pub use run::run;
pub use schedule_restart::schedule_restart;
pub use source::source;
pub use user_db::user_db;
use uuid_mc::PlayerUuid;
pub use whitelist::whitelist;

use crate::Error;

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

async fn get_uuid(mc_username: &str, mode: OfflineOnline) -> Result<PlayerUuid, Error> {
    match mode {
        OfflineOnline::Offline => Ok(PlayerUuid::new_with_offline_username(mc_username)),
        OfflineOnline::Online => {
            let mc_username = mc_username.to_owned();
            tokio::task::spawn_blocking(move || {
                PlayerUuid::new_with_online_username(&mc_username).map_err(Into::into)
            })
            .await
            .unwrap()
        }
    }
}

#[derive(poise::ChoiceParameter, Copy, Clone, Debug, PartialEq, Eq)]
enum OfflineOnline {
    Offline,
    Online,
}

impl OfflineOnline {
    /// Returns true iff the variant is Online.
    pub fn is_online(self) -> bool {
        match self {
            OfflineOnline::Offline => false,
            OfflineOnline::Online => true,
        }
    }
}
