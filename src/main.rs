use std::{io::Write, path::PathBuf};

use clap::{Parser, Subcommand};
use libdistore::gui;

mod commands;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Custom config directory to use
    #[arg(short, long)]
    config_directory: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print or set config values
    Config {
        /// If present, set the value to be used globally
        #[arg(short, long)]
        global: bool,

        /// Key to be set. Possible keys: token, channel
        #[arg(requires = "value")]
        key: Option<String>,
        /// Value for the key
        #[arg(requires = "key")]
        value: Option<String>,
    },
    /// Disassemble the file into '.part' files
    Disassemble {
        /// File to be disassembled
        file: PathBuf,

        /// Directory for the part files to be written to. Defaults to the current directory
        #[arg(short, long, default_value = "./")]
        output_directory: PathBuf,
    },
    /// Assembles '.part' files into the original file
    Assemble {
        /// Name of the original file
        file_name: String,

        /// Directory where the part files are located in. Defaults to the current directory
        #[arg(short, long, require_equals = true, default_value = "./")]
        parts: PathBuf,

        /// Specifies the output file where the assembled file will be written to
        #[arg(short, long, require_equals = true)]
        output: Option<PathBuf>,
    },
    /// Uploads a file to Discord
    Upload {
        /// File to be uploaded
        file: PathBuf,

        /// Optionally use a token for this one time
        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        /// Optionally use a channel for this one time
        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
    /// Downloads a file from Discord
    Download {
        /// Message ID for the file
        message_id: u64,

        /// Specifies the output file where the assembled file will be written to
        #[arg(short, long, require_equals = true)]
        output: Option<PathBuf>,

        /// Optionally use a token for this one time
        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        /// Optionally use a channel for this one time
        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
    /// Lists all the files uploaded to the channel
    List {
        /// Optionally use a token for this one time
        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        /// Optionally use a channel for this one time
        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
    /// Checks for updates
    Update,
    /// Deletes a file from Discord
    Delete {
        /// Message ID for the file
        message_id: u64,

        /// Optionally use a token for this one time
        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        /// Optionally use a channel for this one time
        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
}

// Convenience macro to read user input
macro_rules! inputln {
    ($message:expr) => {{
        print!("{}: ", $message);
        std::io::stdout().flush().unwrap();
        inputln!()
    }};
    () => {{
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    }};
}

fn first_time_run(args: Args) {
    println!("Looks like it's your first time running.");
    println!(
        "Follow the instructions at https://github.com/obvMellow/distore?tab=readme-ov-file#usage"
    );
    println!("Then Input your token and channel ID. They will be set automatically for you.");

    let token = inputln!("Token");
    let channel = inputln!("Channel");

    commands::config(true, "token".into(), token, args.config_directory.clone()).unwrap();
    commands::config(true, "channel".into(), channel, args.config_directory).unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.command.is_none() {
        gui::run();
    }

    let config_path = dirs::config_dir()
        .expect("No config directory found.")
        .join("distore/distore.ini");

    if !config_path.exists() {
        first_time_run(args);
        return Ok(());
    }

    let command = args.command.unwrap();

    match command {
        Commands::Config { global, key, value } => match key {
            Some(key) => commands::config(global, key, value.unwrap(), args.config_directory)?,
            None => commands::get_config(global, args.config_directory)?,
        },
        Commands::Disassemble {
            file,
            output_directory,
        } => commands::disassemble(file, output_directory)?,
        Commands::Assemble {
            file_name,
            parts,
            output,
        } => commands::assemble(file_name, parts, output)?,
        Commands::Upload {
            file,
            token,
            channel,
        } => commands::upload(file, token, channel, args.config_directory).await?,
        Commands::Download {
            message_id,
            output,
            token,
            channel,
        } => commands::download(message_id, token, channel, args.config_directory, output).await?,
        Commands::List { token, channel } => {
            commands::list(token, channel, args.config_directory).await?
        }
        Commands::Update => commands::check_update().await?,
        Commands::Delete {
            message_id,
            token,
            channel,
        } => commands::delete(message_id, token, channel, args.config_directory).await?,
    }

    Ok(())
}
