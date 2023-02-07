use menv::require_envs;

require_envs! {
    (assert_env_vars, any_set, gen_help);
    rcon_addr, "RCON_ADDR", String,
    "RCON_ADDR should be set to the address of the minecraft server.";

    rcon_pass, "RCON_PASS", String,
    "RCON_PASS should be set to the password for the RCON protocol.";

    discord_token, "DISCORD_TOKEN", String,
    "DISCORD_TOKEN should be set to the discord bot token.";

    guild_id, "GUILD_ID", u64,
    "GUILD_ID should be set to the id of the discord server where the bot will operate.";

    op_role_id, "OP_ROLD_ID", u64,
    "OP_ROLE_ID should be set to the id for the role that is assigned to server operators.";

    list_channel_id, "LIST_CHANNEL_ID", u64,
    "LIST_CHANNEL_ID should be set to the id of the channel where the bot will post the updated player list.";

    has_list_json?, "HAS_LIST_JSON", EnvUnit, // TODO: make this a proper flag if possible
    "HAS_LIST_JSON, if set, signifies to the bot that the server support's boolean_coercion's \"/list json\" protocol.";

    server_directory, "SERVER_DIR", String,
    "SERVER_DIR should be set to the root directory where the minecraft server files reside.";

    db_username?, "DB_USERNAME", String,
    "DB_USERNAME, if set, specifies the username for both endpoints of the database API.";

    db_admin_endpoint?, "DB_ADMIN_ENDPOINT", String,
    "DB_ADMIN_ENDPOINT, if set, specifies the URL of the /admin endpoint of the database API.";

    db_admin_password?, "DB_ADMIN_PASSWORD", String,
    "DB_ADMIN_PASSWORD, if set, specifies the authentication password for the /admin endpoint of the database API.";

    db_user_endpoint?, "DB_USER_ENDPOINT", String,
    "DB_USER_ENDPOINT, if set, specifies the URL of the /user endpoint of the database API.";

    db_user_password?, "DB_USER_PASSWORD", String,
    "DB_USER_PASSWORD, if set, specifies the authentication password for the /user endpoint of the database API.";
}

pub struct EnvUnit;

impl std::str::FromStr for EnvUnit {
    type Err = std::convert::Infallible;

    fn from_str(_: &str) -> Result<Self, Self::Err> {
        Ok(Self)
    }
}
