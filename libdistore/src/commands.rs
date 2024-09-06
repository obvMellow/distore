use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
};

use crate::config::{ConfigError, ConfigValue};
use anyhow::{Context, Result};
use colored::Colorize;
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use lazy_static::lazy_static;
use log::info;
use reqwest::Client;
use semver::Version;
use serde_json::Value;
use serenity::all::{ChannelId, CreateAttachment, CreateMessage, GetMessages, Http, Message};

static PART_SIZE: usize = 1000 * 1000 * 10;

lazy_static! {
    static ref VERSION: Version = {
        let mut buf = String::new();
        Command::new("distore")
            .arg("-V")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
            .stdout
            .unwrap()
            .read_to_string(&mut buf)
            .unwrap();
        Version::from_str(buf.split(" ").nth(1).unwrap().trim()).unwrap()
    };
}

pub fn config(global: bool, key: String, val: String, dir: Option<PathBuf>) -> Result<()> {
    let conf = ConfigValue::parse(key, val)?;
    let current_dir = env::current_dir()?;
    let scope = match global {
        true => None,
        false => Some(
            current_dir
                .clone()
                .to_str()
                .ok_or(ConfigError::NonUnicodePath(current_dir))?
                .to_string(),
        ),
    };

    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");

    ConfigValue::write_to_path(path.as_path(), &conf, scope)
        .context("Failed to write to the config file")?;
    println!("Set \"{}\"", conf);
    Ok(())
}

pub fn get_config(global: bool, dir: Option<PathBuf>) -> Result<()> {
    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");
    let (token, channel) = match global {
        true => crate::config::ConfigValue::get_global_config(&path)?,
        false => crate::config::ConfigValue::get_current_config(&path)?,
    };

    println!("{}", token);
    println!("{}", channel);
    Ok(())
}

pub fn disassemble(path: PathBuf, output: PathBuf) -> Result<()> {
    colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .init();
    let mut file =
        File::open(&path).with_context(|| format!("Cannot open file: {}", path.display()))?;
    let filename = path.file_name().unwrap().to_str().unwrap().to_owned();

    let mut buf = vec![0; PART_SIZE];

    let mut i = 0;
    while let Ok(bytes_read) = file.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }

        let name = format!("{}.part{}", filename, i);
        let mut chunk = File::create(output.join(&name))?;

        info!("{} {name}", "Writing".blue().bold());
        chunk.write_all(&buf[..bytes_read])?;
        i += 1;
    }

    println!(
        "{} {filename} into {i} parts",
        "Disassembled".green().bold()
    );
    Ok(())
}

pub fn assemble(filename: String, path: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let read_dir = path.read_dir()?;

    let mut parts = Vec::new();

    let look_for = format!("{filename}.part");

    for entry in read_dir {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        if !entry.file_name().to_str().unwrap().starts_with(&look_for) {
            continue;
        }

        parts.push(entry.path());
    }
    parts.sort_unstable_by(|a, b| {
        let a_name = a
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .chars()
            .last()
            .unwrap();
        let b_name = b
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .chars()
            .last()
            .unwrap();

        a_name.partial_cmp(&b_name).unwrap()
    });

    let mut out = File::create(output.clone().unwrap_or(path.clone().join(&filename)))?;

    let multi = MultiProgress::new();
    let logger = colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .build();
    LogWrapper::new(multi.clone(), logger)
        .try_init()
        .context("Failed to initilize logger")
        .unwrap();
    let pb = multi.add(ProgressBar::new(parts.len().try_into().unwrap()));

    pb.set_style(
        ProgressStyle::with_template(
            "     {msg:.blue.bold} [{bar:50.cyan/blue}] {human_pos}/{human_len} [{elapsed_precise}]",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("Assembling");

    let amount = parts.len();
    let mut buf = Vec::new();
    for part in parts {
        info!("{} {}", "Writing".blue().bold(), part.display());
        buf.clear();
        let mut part = File::open(part).unwrap();
        part.read_to_end(&mut buf)?;
        out.write_all(&buf)?;
        pb.inc(1);
    }
    pb.finish();

    println!(
        "{} {} parts into {}",
        "Assembled".green().bold(),
        amount,
        output.unwrap_or(path.join(filename)).display()
    );

    Ok(())
}

pub async fn upload(
    file: PathBuf,
    token: Option<String>,
    channel: Option<u64>,
    dir: Option<PathBuf>,
) -> Result<()> {
    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");

    let token = token.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .context("Failed to get the config file")
            .unwrap()
            .0
            .inner()
            .to_string()
    });
    let channel = channel.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .unwrap()
            .1
            .inner()
            .parse()
            .unwrap()
    });

    let http = Http::new(&token);

    let filename = file.file_name().unwrap().to_str().unwrap();

    let msg = format!(
        "### This message is generated by Distore. Do not edit this message.\nname={}\nsize={}",
        filename,
        file.metadata()?.len()
    );

    let cache_dir = dirs::cache_dir().unwrap().join("distore");
    fs::create_dir_all(&cache_dir)?;
    disassemble(file.clone(), cache_dir.clone())?;
    let read_dir = cache_dir.read_dir()?;

    let mut parts = Vec::new();
    let mut part_paths = Vec::new();

    let look_for = format!("{filename}.part");

    for entry in read_dir {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        if !entry.file_name().to_str().unwrap().starts_with(&look_for) {
            continue;
        }

        parts.push(CreateAttachment::path(entry.path()).await?);
        part_paths.push(entry.path());
    }

    info!("Uploading...");
    let msg = ChannelId::from(channel)
        .send_files(http, parts, CreateMessage::new().content(msg))
        .await?;

    info!("Cleaning up...");

    for part in part_paths {
        info!("{} {}", "Removing".blue().bold(), part.display());
        fs::remove_file(part).context("Failed to remove file")?;
    }

    println!(
        "{} parts to channel id {}. Message id: {}",
        "Uploaded".green().bold(),
        msg.channel_id,
        msg.id
    );

    Ok(())
}

pub async fn download(
    message_id: u64,
    token: Option<String>,
    channel: Option<u64>,
    dir: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<()> {
    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");

    let token = token.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .context("Failed to get the config file")
            .unwrap()
            .0
            .inner()
            .to_string()
    });
    let channel = channel.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .unwrap()
            .1
            .inner()
            .parse()
            .unwrap()
    });

    let http = Http::new(&token);

    let msg = http.get_message(channel.into(), message_id.into()).await?;
    let content = msg.content.replace(
        "### This message is generated by Distore. Do not edit this message.\n",
        "",
    );
    let name = content
        .split("\n")
        .next()
        .unwrap()
        .split("=")
        .nth(1)
        .unwrap();

    let mut out = File::create(output.clone().unwrap_or(name.into()))?;

    let multi = MultiProgress::new();
    let logger = colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .build();
    LogWrapper::new(multi.clone(), logger)
        .try_init()
        .context("Failed to initilize logger")
        .unwrap();
    let pb = multi.add(ProgressBar::new(msg.attachments.len().try_into().unwrap()));

    pb.set_style(
        ProgressStyle::with_template(
            "     {msg:.blue.bold} [{bar:50.cyan/blue}] {human_pos}/{human_len}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("Assembling");

    for part in msg.attachments {
        info!("{} {}", "Downloading".blue().bold(), part.filename);
        let part = part.download().await?;
        out.write_all(&part)?;
        pb.inc(1);
    }
    pb.finish();

    println!(
        "{} {}",
        "Downloaded".green().bold(),
        output.unwrap_or(name.into()).display()
    );

    Ok(())
}

pub async fn list(token: Option<String>, channel: Option<u64>, dir: Option<PathBuf>) -> Result<()> {
    colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .init();
    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");

    let token = token.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .context("Failed to get the config file")
            .unwrap()
            .0
            .inner()
            .to_string()
    });
    let channel = channel.unwrap_or_else(|| {
        crate::config::ConfigValue::get_current_config(&path)
            .unwrap()
            .1
            .inner()
            .parse()
            .unwrap()
    });

    let http = Http::new(&token);

    let channel = http.get_channel(channel.into()).await?.id();

    info!("Retrieving messages...");
    let messages = _get_messages(channel, &http).await?;

    for msg in messages {
        if !msg.author.bot {
            continue;
        }

        if !msg
            .content
            .starts_with("### This message is generated by Distore. Do not edit this message.\n")
        {
            continue;
        }
        let content = msg.content.replace(
            "### This message is generated by Distore. Do not edit this message.\n",
            "",
        );
        let name = content
            .split("\n")
            .next()
            .unwrap()
            .split("=")
            .nth(1)
            .unwrap();
        let size = content
            .split("\n")
            .nth(1)
            .unwrap()
            .split("=")
            .nth(1)
            .unwrap();

        println!(
            "{}: {}\n    {}: {name}\n    {}: {}",
            "ID".bold(),
            msg.id,
            "Name".bold(),
            "Size".bold(),
            HumanBytes(size.parse().unwrap())
        );
    }

    Ok(())
}

pub async fn check_update() -> Result<()> {
    let url = "https://crates.io/api/v1/crates/distore";

    let res = Client::new()
        .get(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", format!("distore/{}", *VERSION))
        .send()
        .await
        .context("Failed to fetch the latest version")?;

    let error = res.error_for_status();
    match error {
        Ok(res) => {
            let json = res.json::<Value>().await?;

            let latest = Version::from_str(
                json["crate"]["newest_version"]
                    .to_string()
                    .replace("\"", "")
                    .as_str(),
            )?;

            if latest > *VERSION {
                println!("New version available: v{} -> v{latest}", *VERSION);
                println!("Run this command to update: cargo install distore");
            } else {
                println!("Already at the latest version: v{latest}");
            }
            Ok(())
        }
        Err(e) => Err(anyhow::Error::new(e).context("Failed to fetch the latest version")),
    }
}

async fn _get_messages(
    channel_id: ChannelId,
    http: &Http,
) -> Result<Vec<Message>, serenity::Error> {
    let mut out = Vec::new();
    let mut last_message_id = None;

    loop {
        let mut filter = GetMessages::new().limit(100);
        if last_message_id.is_some() {
            filter = filter.before(last_message_id.unwrap());
        }
        let messages = channel_id.messages(http, filter).await?;

        if messages.is_empty() {
            break;
        }

        out.extend(messages);
        last_message_id = Some(out.last().unwrap().id);
    }

    Ok(out)
}
