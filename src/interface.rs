use rcon::Result;

type Connection = rcon::Connection<tokio::net::TcpStream>;

pub struct Interface {
    address: Box<str>,
    password: Box<str>,
    connection: Option<Connection>,
}

impl Interface {
    pub fn new(addr: impl ToString, pass: impl ToString) -> Self {
        Self {
            address: addr.to_string().into_boxed_str(),
            password: pass.to_string().into_boxed_str(),
            connection: None,
        }
    }

    pub async fn exec(&mut self, command: &str) -> Result<String> {
        if let Some(conn) = &mut self.connection {
            match conn.cmd(command).await {
                x @ Ok(..) | x @ Err(rcon::Error::CommandTooLong | rcon::Error::Auth) => return x,
                Err(rcon::Error::Io(..)) => {} // purposefully exhaustive for future-proofness
            }
        }

        // if we're here, either connection is None or it got disconnected - either way, we have to renew it
        self.renew_connection().await?.cmd(command).await
    }

    /// Both updates the internal connection and also returns it.
    async fn renew_connection(&mut self) -> Result<&mut Connection> {
        match Connection::connect(&*self.address, &self.password).await {
            Ok(conn) => Ok(self.connection.insert(conn)),
            Err(why) => Err(why),
        }
    }

    pub async fn player_list(&mut self) -> std::result::Result<Vec<PlayerInfo>, crate::Error> {
        let list_output = self.exec("list uuids").await?;
        let mut list = Vec::with_capacity(list_output.bytes().filter(|&b| b == b',').count());

        let Some((_, players)) = list_output.split_once("players online: ") else {
            return Ok(list);
        };

        for p in players.split(", ") {
            let Some((name, uuid)) = p.rsplit_once(" (") else {
                return Err("Expected player information in form `name (uuid)` but could not find ` (` {p} in {list_output}".into());
            };

            let Some((uuid, _)) = uuid.rsplit_once(")") else {
                return Err("Expected player information in form `name (uuid)` but could not find ending `)` {p} in {list_output}".into());
            };

            list.push(PlayerInfo {
                name: name.into(),
                uuid: uuid_mc::PlayerUuid::new_with_uuid(uuid_mc::Uuid::parse_str(uuid)?)?,
            });
        }

        Ok(list)
    }
}

pub struct PlayerInfo {
    pub name: Box<str>,
    pub uuid: uuid_mc::PlayerUuid,
}
