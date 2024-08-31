use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;
mod config;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,

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
    Upload {
        file: PathBuf,

        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
    Download {
        message_id: u64,

        /// Specifies the output file where the assembled file will be written to
        #[arg(short, long, require_equals = true)]
        output: Option<PathBuf>,

        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
    List {
        #[arg(short, long, require_equals = true)]
        token: Option<String>,

        #[arg(short, long, require_equals = true)]
        channel: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
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
    }

    Ok(())
}
