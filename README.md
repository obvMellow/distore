# Distore
Utility to use Discord as cloud storage.

## Installation

First, make sure you have cargo installed.
```sh
cargo --version
```
You should be getting no errors.
If not you can install cargo from https://www.rust-lang.org/tools/install

Then, you can simply install using cargo
```sh
cargo install distore
```

Make sure you have `.cargo/bin` in your `PATH`

For Linux:
```sh
export PATH=$HOME/.cargo/bin:$PATH
```

For Windows:
```powershell
setx /M PATH "%PATH%;%HOMEPATH%/.cargo/bin"
```

Open a new shell and run the following command. If everything went right you shouldn't get any errors
```sh
distore --version
```

## Usage

First of all, you need to create a Discord bot.

Go to the [Discord Developer Portal](https://discord.com/developers/applications)

- Click 'New Application', you can give it any name you want.
- Click 'Bot' at the left 'Settings' section.
- Reset the bot's token and copy it.
- Save the token to the Distore configuration with the following command
```sh
distore config token <TOKEN> --global
```
- Go back to the Developer Portal, and click 'OAuth2'
- Check the 'Bot' box in the 'OAuth2 URL Generator'
- Check 'Send Messages' and 'Attach Files' in 'Bot Permissions'
- Copy the generated link and paste it to your browser
- Add the bot to a server you own. You can create a new one if you prefer
- Copy the ID of the channel you want your files to be stored in (You need to have 'Developer Mode' enabled. To enable it, go to your Discord setting, go to 'Advanced', and enable 'Developer Mode')
- Save the channel ID to the Distore configuration
```sh
distore config channel <CHANNEL_ID> --global
```

Now, you're ready to use Distore.

### Commands

You can upload a file with the following command:
```sh
distore upload <path/to/file>
```

List all the files you've uploaded:
```sh
distore list
```

And download a file:
```sh
distore download <MESSAGE_ID>
```

You can set a different token and channel for the directory you're in. Just don't set the `--global` flag in the config command
```sh
distore config token <TOKEN>
distore config channel <CHANNEL_ID>
```

For all the commands:
```sh
distore --help
```
