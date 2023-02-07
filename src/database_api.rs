use reqwest::{Client, Method, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serenity::model::prelude::{UserId, UserIdParseError};
use uuid_mc::PlayerUuid;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("reqwest error ({0})")]
    Reqwest(#[from] reqwest::Error),

    #[error("unsuccessful query error")]
    Unsuccessful(Response),

    #[error("json deserialization error ({0})")]
    JsonDeserializationError(#[from] serde_json::Error),

    #[error("uuid_mc error ({0})")]
    UuidMcError(#[from] uuid_mc::Error),

    #[error("uuid error ({0})")]
    UuidError(#[from] uuid_mc::uuid::Error),

    #[error("user id parse error ({0})")]
    UserIdParseError(#[from] UserIdParseError),
}

#[derive(Serialize, Deserialize)]
struct _MCUser {
    minecraft_id: String,
    minecraft_name: Option<String>,
    offline_mode: bool,
}

pub struct MCUser {
    pub uuid: PlayerUuid,
    pub name: Option<String>,
}

impl TryFrom<_MCUser> for MCUser {
    type Error = Error;

    fn try_from(value: _MCUser) -> Result<Self, Self::Error> {
        let _MCUser {
            minecraft_id,
            minecraft_name,
            ..
        } = value;

        let uuid = minecraft_id.parse()?;
        let uuid = PlayerUuid::new_with_uuid(uuid)?;

        Ok(Self {
            uuid,
            name: minecraft_name,
        })
    }
}

#[derive(Deserialize)]
struct _User {
    discord_id: String,
    mc: Vec<_MCUser>,
}

pub struct User {
    pub discord_id: UserId,
    pub mc_users: Vec<MCUser>,
}

impl TryFrom<_User> for User {
    type Error = Error;

    fn try_from(value: _User) -> Result<Self, Self::Error> {
        let _User { discord_id, mc } = value;

        let mc_users: Result<Vec<MCUser>, Error> = mc.into_iter().map(TryInto::try_into).collect();

        Ok(Self {
            discord_id: discord_id.parse()?,
            mc_users: mc_users?,
        })
    }
}

pub struct MonadApi {
    username: Box<str>,
    admin_endpoint: Box<str>,
    admin_password: Box<str>,
    user_endpoint: Box<str>,
    user_password: Box<str>,
}

impl MonadApi {
    pub fn new(
        username: &str,
        admin_endpoint: &str,
        admin_password: &str,
        user_endpoint: &str,
        user_password: &str,
    ) -> Self {
        Self {
            username: username.to_owned().into_boxed_str(),
            admin_endpoint: admin_endpoint.to_owned().into_boxed_str(),
            admin_password: admin_password.to_owned().into_boxed_str(),
            user_endpoint: user_endpoint.to_owned().into_boxed_str(),
            user_password: user_password.to_owned().into_boxed_str(),
        }
    }

    fn user_request(&self, method: Method, endpoint: &str) -> RequestBuilder {
        Client::new()
            .request(method, format!("https://{}/{endpoint}", self.user_endpoint))
            .basic_auth(&self.username, Some(&self.user_password))
    }

    fn admin_request(&self, method: Method, endpoint: &str) -> RequestBuilder {
        Client::new()
            .request(
                method,
                format!("https://{}/{endpoint}", self.admin_endpoint),
            )
            .basic_auth(&self.username, Some(&self.admin_password))
    }

    async fn post_admin_discord(&self, user_id: &str, body: &_MCUser) -> Result<(), Error> {
        let response = self
            .admin_request(Method::POST, &format!("discord/{user_id}"))
            .json(body)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(Error::Unsuccessful(response))
        }
    }

    pub async fn insert_user_with_name(
        &self,
        discord_id: UserId,
        minecraft_username: &str,
        online_mode: bool,
    ) -> Result<(), Error> {
        let uuid = if online_mode {
            let owned_username = minecraft_username.to_owned();
            tokio::task::spawn_blocking(move || {
                PlayerUuid::new_with_online_username(&owned_username)
            })
            .await
            .unwrap()?
        } else {
            PlayerUuid::new_with_offline_username(minecraft_username)
        };
        let mc_user = _MCUser {
            minecraft_id: uuid.as_uuid().to_string(),
            minecraft_name: Some(minecraft_username.to_string()),
            offline_mode: !online_mode,
        };

        self.post_admin_discord(&discord_id.to_string(), &mc_user)
            .await
    }

    pub async fn insert_user_with_uuid(
        &self,
        discord_id: UserId,
        minecraft_id: PlayerUuid,
    ) -> Result<(), Error> {
        let mc_user = _MCUser {
            minecraft_id: minecraft_id.as_uuid().to_string(),
            minecraft_name: None,
            offline_mode: minecraft_id.offline().is_some(),
        };

        self.post_admin_discord(&discord_id.to_string(), &mc_user)
            .await
    }

    async fn delete_admin_discord(&self, user_id: &str) -> Result<(), Error> {
        let response = self
            .admin_request(Method::DELETE, &format!("discord/{user_id}"))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::Unsuccessful(response))
        }
    }

    pub async fn delete_user_with_discord(&self, discord_id: UserId) -> Result<(), Error> {
        self.delete_admin_discord(&discord_id.to_string()).await
    }

    async fn delete_admin_minecraft(&self, user_id: &str) -> Result<(), Error> {
        let response = self
            .admin_request(Method::DELETE, &format!("minecraft/{user_id}"))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::Unsuccessful(response))
        }
    }

    pub async fn delete_user_with_minecraft(
        &self,
        minecraft_uuid: PlayerUuid,
    ) -> Result<(), Error> {
        self.delete_admin_minecraft(&minecraft_uuid.as_uuid().to_string())
            .await
    }

    async fn get_user_discord(&self, user_id: &str) -> Result<_User, Error> {
        let response = self
            .user_request(Method::GET, &format!("discord/{user_id}"))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(Error::Unsuccessful(response))
        }
    }

    pub async fn get_users_with_discord(&self, discord_id: UserId) -> Result<Vec<MCUser>, Error> {
        let user = self.get_user_discord(&discord_id.to_string()).await?;
        user.mc.into_iter().map(TryInto::try_into).collect()
    }

    async fn get_user_minecraft(&self, user_id: &str) -> Result<_User, Error> {
        let response = self
            .user_request(Method::GET, &format!("minecraft/{user_id}"))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(Error::Unsuccessful(response))
        }
    }

    pub async fn get_users_with_minecraft(
        &self,
        minecraft_uuid: PlayerUuid,
    ) -> Result<User, Error> {
        self.get_user_minecraft(&minecraft_uuid.as_uuid().to_string())
            .await
            .and_then(TryInto::try_into)
    }
}
