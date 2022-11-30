# ferrisquery
A simple discord bot that will query your minecraft server and allow administrators to run commands without having to log in.  
This implementation uses the RCON protocol, so it can even run with a vanilla server.

It is highly recommended to run this bot on the same machine that hosts the server, since Minecraft's RCON protocol is unencrypted and insecure,
and therefore should not be exposed to the internet.

## Installation
Either clone this repo and run `cargo build --release` (requires cargo and rustc >= 1.65), or download one of the releases.

## Configuration
The following environment variables must be set before running the bot:
- `GUILD_ID`: The ID of the guild it will be interacting with.
- `OP_ROLE_ID`: The ID of the discord roles which is given to server operators (users without said role will not be allowed to run commands).
- `RCON_ADDR`: The address and port of your server. Use the IP `127.0.0.1` if running locally (recommended).  
The default port that comes with every server is 25575 (this is specified in `server.properties`), therefore when using the IP above the
full address would be `127.0.0.1:25575`.
- `RCON_PASS`: The RCON password as configured in `server.properties`.
- `DISCORD_TOKEN`: The token of your discord bot.
- `LIST_CHANNEL_ID`: The ID of the channel where the self-updating `list` message is going to be. This should be a dedicated channel
for the bot.

Here's an example shell script similar to the one that I use:
```sh
#!/bin/sh
GUILD_ID="<insert guild id here>" \
OP_ROLE_ID="<insert op role id here>" \
RCON_ADDR="127.0.0.1:25575" \
RCON_PASS="<insert rcon password here>" \
DISCORD_TOKEN="<insert discord bot token here>" \
LIST_CHANNEL_ID="<insert list channel id here>" \
\
./ferrisquery
```

Your discord bot must be authorized with the `application.command` and `bot` scopes, and must additionally have the "Send Messages" permission.

You must also verify the following in `server.properties`:
- `enable-rcon` is set to `true`.
- `rcon.port` and `rcon.password` are correctly configured (note: Minecraft treats the `\` character (along with some others) as special, so be aware of that
when creating your password. I recommend setting a random, long, strictly alphanumeric password to avoid problems.  
Also note that the password is just a precaution, since as mentioned earlier your RCON port should not be open to the internet in the first place.
- Optionally, set `broadcast-rcon-to-op` to `false` to prevent spamming the chat with the periodic `list` executions.
