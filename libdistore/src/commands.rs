use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
};

use crate::{
    config::{ConfigError, ConfigValue},
    parser::FileEntry,
};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use futures::future::join_all;
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use lazy_static::lazy_static;
use log::info;
use reqwest::Client;
use semver::Version;
use serde_json::Value;
use serenity::all::{
    ChannelId, CreateAttachment, CreateMessage, EditMessage, GetMessages, Http, Message,
};

static PART_SIZE: usize = 1000 * 1000 * 20;

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
    let (token, channel) = get_config_internal(global, dir)?;

    println!("{}", token);
    println!("{}", channel);
    Ok(())
}

pub(crate) fn get_config_internal(
    global: bool,
    dir: Option<PathBuf>,
) -> Result<(ConfigValue, ConfigValue)> {
    let mut path = dir
        .unwrap_or(dirs::config_dir().ok_or(ConfigError::NoConfigDir)?)
        .join("distore");
    fs::create_dir_all(&path).context("Failed to create config directory")?;
    path.push("distore.ini");
    let out = match global {
        true => crate::config::ConfigValue::get_global_config(&path)?,
        false => crate::config::ConfigValue::get_current_config(&path)?,
    };
    return Ok(out);
}

pub fn disassemble(path: PathBuf, output: PathBuf) -> Result<()> {
    colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .init();

    let (_, filename, i) = disassemble_internal(path, output, |_, _| {})?;

    println!(
        "{} {filename} into {i} parts",
        "Disassembled".green().bold()
    );
    Ok(())
}

pub(crate) fn disassemble_internal<F: Fn(String, f64)>(
    path: PathBuf,
    output: PathBuf,
    callback: F,
) -> Result<(Vec<PathBuf>, String, usize)> {
    let mut file =
        File::open(&path).with_context(|| format!("Cannot open file: {}", path.display()))?;
    let filename = path.file_name().unwrap().to_str().unwrap().to_owned();

    let mut out = Vec::new();

    let mut buf = vec![0; PART_SIZE];

    let mut progress = 0;
    let total = (file.metadata().unwrap().len() + PART_SIZE as u64 - 1) / PART_SIZE as u64;
    while let Ok(bytes_read) = file.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }

        let name = format!("{}.part{}", filename, progress);
        let path = output.join(&name);
        let mut chunk = File::create(&path)?;

        info!("{} {name}", "Writing".blue().bold());
        chunk.write_all(&buf[..bytes_read])?;
        progress += 1;

        out.push(path);
        let fraction = if total > 0 {
            progress as f64 / total as f64
        } else {
            1.0
        };

        let fraction = fraction.clamp(0.0, 1.0);
        callback(format!("Disassembling {}", filename), fraction);
    }

    let len = out.len();
    Ok((out, filename, len))
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

    let messages = upload_internal(&http, file, channel, |_, _| {}).await?;

    println!(
        "{} parts to channel id {}. Message id: {}",
        "Uploaded".green().bold(),
        messages[0].channel_id,
        messages[0].id
    );

    Ok(())
}

pub(crate) async fn upload_internal<F: Fn(String, f64)>(
    http: &Http,
    file: PathBuf,
    channel: u64,
    callback: F,
) -> Result<Vec<Message>> {
    let cache_dir = dirs::cache_dir().unwrap().join("distore");
    fs::create_dir_all(&cache_dir)?;
    let (part_paths, filename, _) =
        disassemble_internal(file.clone(), cache_dir.clone(), &callback)?;

    let msg = format!(
        "### This message is generated by Distore. Do not edit this message.\nname={}\nsize={}",
        filename,
        file.metadata()?.len()
    );

    info!("Uploading...");
    let chunks: Vec<Vec<PathBuf>> = part_paths.chunks(10).map(|chunk| chunk.to_vec()).collect();
    let mut messages = Vec::new();
    info!("Sending {} message(s) in total", chunks.len());

    callback(format!("Uploading {}", filename), 0.0);
    let mut progress = 0;
    let total = chunks.len();
    for chunk in chunks {
        let attachment_futures: Vec<_> = chunk
            .into_iter()
            .map(|path| CreateAttachment::path(path))
            .collect();
        let attachments = join_all(attachment_futures).await;

        let msg = ChannelId::from(channel)
            .send_files(
                &http,
                attachments.into_iter().map(|a| a.unwrap()),
                CreateMessage::new().content("tmp"),
            )
            .await?;
        messages.push(msg.clone());
        progress += 1;

        let fraction = if total > 0 {
            progress as f64 / total as f64
        } else {
            1.0
        };

        let fraction = fraction.clamp(0.0, 1.0);
        callback(format!("Uploading {}", filename), fraction);

        info!(
            "Sent {}..{}",
            msg.attachments
                .first()
                .unwrap()
                .filename
                .split(".")
                .last()
                .unwrap(),
            msg.attachments
                .last()
                .unwrap()
                .filename
                .split(".")
                .last()
                .unwrap()
        );
    }

    info!("Editing messages...");

    let mut progress = 0;
    let total = messages.len();
    for (i, message) in messages.iter().enumerate() {
        let mut content = String::new();
        let next = messages.iter().cloned().nth(i + 1);
        if i == 0 {
            content = format!("{msg}\nlen={}\n", part_paths.len());
        }
        match next {
            Some(v) => {
                content += &format!("next={}", v.id);
            }
            None => {}
        }
        message
            .clone()
            .edit(&http, EditMessage::new().content(content))
            .await?;
        progress += 1;

        let fraction = if total > 0 {
            progress as f64 / total as f64
        } else {
            1.0
        };

        callback("Editing".to_string(), fraction);
    }

    info!("Cleaning up...");

    for part in part_paths {
        info!("{} {}", "Removing".blue().bold(), part.display());
        fs::remove_file(part).context("Failed to remove file")?;
    }

    Ok(messages)
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

    let (_, _, name, len) = _get_download_variables(&http, message_id, channel).await?;

    let multi = MultiProgress::new();
    let logger = colog::default_builder()
        .filter(Some("serenity"), log::LevelFilter::Off)
        .build();
    LogWrapper::new(multi.clone(), logger)
        .try_init()
        .context("Failed to initilize logger")
        .unwrap();
    let pb = multi.add(ProgressBar::new(len as u64));

    pb.set_style(
        ProgressStyle::with_template(
            "     {msg:.blue.bold} [{bar:50.cyan/blue}] {human_pos}/{human_len}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("Assembling");

    let pb_clone = pb.clone();
    download_internal(&http, message_id, channel, output.clone(), move |_| {
        pb_clone.inc(1)
    })
    .await?;

    pb.finish();

    println!(
        "{} {}",
        "Downloaded".green().bold(),
        output.unwrap_or(name.into()).display()
    );

    Ok(())
}

pub(crate) async fn _get_download_variables(
    http: &Http,
    message_id: u64,
    channel: u64,
) -> Result<(Message, FileEntry, String, usize)> {
    let msg = http.get_message(channel.into(), message_id.into()).await?;
    let entry = FileEntry::from_str(&msg.content)?;
    let name = entry.name.clone().ok_or(anyhow!("Invalid Message"))?;
    let len = entry.len.ok_or(anyhow!("Invalid Message"))?;

    Ok((msg, entry, name, len))
}

pub(crate) async fn download_internal<F: Fn(f64)>(
    http: &Http,
    message_id: u64,
    channel: u64,
    output: Option<PathBuf>,
    callback: F,
) -> Result<PathBuf> {
    let (msg, mut entry, name, len) = _get_download_variables(http, message_id, channel).await?;

    let size = entry.size.unwrap();

    let path = output.clone().unwrap_or(name.clone().into());
    let mut out = File::create(&path)?;

    let mut i = 0;
    let mut msg = msg;
    let mut progress = 0;
    while entry.next.is_some() || i < len {
        for part in msg.attachments.iter() {
            info!("{} {}", "Downloading".blue().bold(), part.filename);
            let part = part.download().await?;

            progress += part.len();
            let fraction = if size > 0 {
                progress as f64 / size as f64
            } else {
                1.0
            };

            let fraction = fraction.clamp(0.0, 1.0);

            out.write_all(&part)?;
            callback(fraction);
        }
        i += msg.attachments.len();

        if entry.next.is_none() {
            continue;
        }

        let next_id = entry.next.unwrap();
        msg = http.get_message(channel.into(), next_id.into()).await?;
        entry = FileEntry::from_str(&msg.content)?;
    }

    Ok(path)
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

    let list = list_internal(channel.into(), &http).await?;

    for entry in list {
        println!(
            "{}: {}\n    {}: {}\n    {}: {}",
            "ID".bold(),
            entry.1,
            "Name".bold(),
            entry.0.name.unwrap(),
            "Size".bold(),
            HumanBytes(entry.0.size.unwrap())
        );
    }
    Ok(())
}

pub(crate) async fn list_internal(channel: u64, http: &Http) -> Result<Vec<(FileEntry, u64)>> {
    let messages = _get_messages(channel.into(), &http).await?;
    let mut out = Vec::new();

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
        let entry = FileEntry::from_str(&msg.content)?;
        if entry.name.is_none() {
            continue;
        }
        let name = entry.name.ok_or(anyhow!("Invalid Message"))?;
        let size = entry.size.ok_or(anyhow!("Invalid Message"))?;

        out.push((
            FileEntry {
                name: Some(name),
                size: Some(size),
                len: entry.len,
                next: entry.next,
            },
            msg.id.into(),
        ))
    }
    return Ok(out);
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

pub async fn delete(
    message_id: u64,
    token: Option<String>,
    channel: Option<u64>,
    dir: Option<PathBuf>,
) -> Result<()> {
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

    delete_internal(&http, message_id, channel, || {}).await
}

pub(crate) async fn delete_internal<F: Fn()>(
    http: &Http,
    message_id: u64,
    channel_id: u64,
    callback: F,
) -> Result<()> {
    let msg = http
        .get_message(channel_id.into(), message_id.into())
        .await?;

    let mut entry = FileEntry::from_str(&msg.content)?;

    let len = entry.len.ok_or(anyhow!("Invalid Message"))?;
    info!("Deleting {} message(s)...", (len + 9) / 10);

    msg.delete(&http).await?;

    while entry.next.is_some() {
        let msg = http
            .get_message(channel_id.into(), entry.next.unwrap().into())
            .await?;
        entry = FileEntry::from_str(&msg.content)?;
        msg.delete(&http).await?;

        callback();
    }

    Ok(())
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
