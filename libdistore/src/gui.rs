use std::cell::RefCell;
use std::path::PathBuf;
use std::process::exit;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use gtk::gio::{Cancellable, FileQueryInfoFlags, FILE_ATTRIBUTE_STANDARD_NAME};
use gtk::{glib, Entry, FileDialog, ScrolledWindow};
use gtk::{prelude::*, Align, ApplicationWindow, Box, Label, ListBox, ListBoxRow, Orientation};
use gtk::{AlertDialog, Application, Button, ProgressBar};
use indicatif::HumanBytes;
use serenity::all::{ChannelId, Http};

use crate::commands::{self, delete_internal, download_internal, upload_internal};
use crate::config::ConfigValue;
use crate::parser::FileEntry;

const APP_ID: &str = "org.distore.Distore";

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);

    let exitcode = app.run();
    exit(exitcode.into());
}

fn build_ui(app: &Application) {
    let window = Rc::new(ApplicationWindow::new(app));
    window.set_title(Some("Distore"));
    window.set_default_size(300, 200);

    let list_box = Rc::new(ListBox::new());
    let margin = 50;
    list_box.set_margin_top(margin);
    list_box.set_margin_bottom(margin);
    list_box.set_show_separators(true);

    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_width(200)
        .min_content_height(300)
        .vexpand(true)
        .build();

    scrolled_window.set_child(Some(&*list_box));

    let parent_box = Rc::new(Box::new(Orientation::Vertical, 5));
    parent_box.set_margin_bottom(margin);

    let container = Rc::new(Box::new(Orientation::Vertical, 20));
    container.append(&scrolled_window);

    container.set_margin_start(margin);
    container.set_margin_end(margin);

    let (token, channel) = commands::get_config_internal(true, None).unwrap();
    let (token, channel) = (Rc::new(RefCell::new(token)), Rc::new(RefCell::new(channel)));
    let http = Arc::new(Http::new(token.borrow().inner()));

    let components = async_std::task::block_on(commands::list_internal(
        channel.borrow().inner().parse().unwrap(),
        &http,
    ))
    .unwrap();

    for (file, id) in components {
        let row = ListBoxRow::new();
        let box_ = Box::new(Orientation::Vertical, 5);
        box_.set_halign(Align::Start);

        let name_label = Label::new(file.name.as_deref());
        let id_label = Label::new(Some(&format!("ID: {}", id)));
        let size_label = Label::new(Some(&format!("Size: {}", HumanBytes(file.size.unwrap()))));

        name_label.set_halign(Align::Start);
        id_label.set_halign(Align::Start);
        size_label.set_halign(Align::Start);

        size_label.set_opacity(0.5);
        id_label.set_opacity(0.5);

        size_label.set_margin_start(20);
        id_label.set_margin_start(20);

        box_.append(&name_label);
        box_.append(&size_label);
        box_.append(&id_label);

        row.set_child(Some(&box_));
        list_box.append(&row);
    }

    let progress_box = Rc::new(Box::new(Orientation::Vertical, 20));
    progress_box.set_margin_start(margin);
    progress_box.set_margin_end(margin);

    let button_box = Box::new(Orientation::Horizontal, 10);

    let download_btn = Button::builder().label("Download").build();
    let upload_btn = Button::builder().label("Upload").build();
    let delete_btn = Button::builder().label("Delete").build();
    let settings_btn = Button::builder().label("Settings").build();
    button_box.append(&download_btn);
    button_box.append(&upload_btn);
    button_box.append(&delete_btn);
    button_box.append(&settings_btn);

    let top_settings_panel = Rc::new(Box::new(Orientation::Vertical, 0));

    let settings_panel = Rc::new(Box::new(Orientation::Vertical, 25));
    settings_panel.set_margin_end(margin);
    settings_panel.set_margin_top(margin);
    settings_panel.set_margin_bottom(margin);
    settings_panel.set_margin_start(margin);

    let token_box = Rc::new(Box::new(Orientation::Horizontal, 10));
    token_box.append(&Label::new(Some("Token")));
    let token_entry = Rc::new(Entry::new());
    token_entry.set_hexpand(true);
    token_box.append(&*token_entry);

    let channel_box = Rc::new(Box::new(Orientation::Horizontal, 10));
    channel_box.append(&Label::new(Some("Channel ID")));
    let channel_entry = Rc::new(Entry::new());
    channel_box.append(&*channel_entry);

    settings_panel.append(&*token_box);
    settings_panel.append(&*channel_box);

    let settings_buttons = Rc::new(Box::new(Orientation::Horizontal, 10));
    settings_buttons.set_margin_bottom(20);
    settings_buttons.set_margin_end(20);
    settings_buttons.set_halign(Align::End);

    let apply_button = Button::builder().label("Apply").build();
    let exit_button = Button::builder().label("Exit").build();

    let token_entry_ = token_entry.clone();
    let channel_entry_ = channel_entry.clone();
    let token_ = token.clone();
    let channel_ = channel.clone();
    apply_button.connect_clicked(move |_| {
        token_.replace(ConfigValue::Token(token_entry_.text().to_string()));
        channel_.replace(ConfigValue::Channel(channel_entry_.text().to_string()));
        commands::config(true, "token".into(), token_entry_.text().to_string(), None).unwrap();
        commands::config(
            true,
            "channel".into(),
            channel_entry_.text().to_string(),
            None,
        )
        .unwrap();
    });

    let window_clone = window.clone();
    let parent_clone = parent_box.clone();
    exit_button.connect_clicked(move |_| {
        window_clone.set_child(Some(&*parent_clone));
    });

    settings_buttons.append(&apply_button);
    settings_buttons.append(&exit_button);

    top_settings_panel.append(&*settings_panel);
    top_settings_panel.append(&*settings_buttons);

    let window_clone = window.clone();
    let settings_panel_ = top_settings_panel.clone();
    let channel_ = channel.clone();
    let token_ = token.clone();
    settings_btn.connect_clicked(move |_| {
        token_entry.set_text(token_.borrow().inner());
        channel_entry.set_text(channel_.borrow().inner());
        window_clone.set_child(Some(&*settings_panel_));
    });

    let list_box_clone = list_box.clone();
    let progress_box_clone = progress_box.clone();
    let window_clone = window.clone();
    let channel_ = channel.clone();
    let token_ = token.clone();
    delete_btn.connect_clicked(move |_| {
        if let Some(selected_row) = list_box_clone.selected_row() {
            if let Some(box_) = selected_row.child().and_then(|w| w.downcast::<Box>().ok()) {
                let mut labels: Vec<Label> = Vec::new();

                let first_child: Label = box_.first_child().unwrap().downcast().unwrap();
                labels.push(first_child.clone());

                let mut current: Option<Label> = Some(first_child);

                while let Some(curr) = current {
                    current = match curr.next_sibling() {
                        Some(v) => v.downcast().ok(),
                        None => None,
                    };

                    if current.is_some() {
                        labels.push(current.clone().unwrap());
                    }
                }

                for (i, label) in labels.iter().enumerate() {
                    println!("Label {}: {}", i, label.label());
                }

                let mut iter = labels.iter();
                let name = iter.next().unwrap().to_owned();
                let _size = iter.next().unwrap().label().replace("Size: ", "");
                let id = iter
                    .next()
                    .unwrap()
                    .label()
                    .replace("ID: ", "")
                    .parse::<u64>()
                    .unwrap();

                let list_box_clone = list_box_clone.clone();
                let progress_box_clone = progress_box_clone.clone();
                let window_clone_ = window_clone.clone();
                let channel_ = channel_.clone();
                let token_ = token_.clone();
                AlertDialog::builder()
                    .message("Are You Sure?")
                    .detail(format!("Do you really want to delete {}?", name.label()))
                    .buttons(["Yes", "No"])
                    .cancel_button(1)
                    .build()
                    .choose(
                        Some(&*window_clone),
                        Some(&gtk::gio::Cancellable::new()),
                        move |r| {
                            let index = r.unwrap();

                            if index == 1 {
                                return;
                            }
                            let progressbar = Rc::new(
                                ProgressBar::builder()
                                    .visible(true)
                                    .show_text(true)
                                    .valign(Align::Fill)
                                    .pulse_step(0.0)
                                    .build(),
                            );
                            progressbar
                                .set_text(Some(format!("Deleting) {}", name.label()).as_str()));
                            progressbar.set_fraction(0.0);

                            progress_box_clone.append(&*progressbar);

                            let (sender, receiver) = mpsc::channel();

                            let http = Http::new(token_.borrow().inner());
                            let channel_ = channel_.borrow().inner().to_owned();
                            tokio::spawn(async move {
                                let res =
                                    delete_internal(&http, id, channel_.parse().unwrap(), || {
                                        sender.send((Some(()), None)).unwrap();
                                    })
                                    .await;

                                sender.send((None, Some(res))).unwrap();
                            });

                            let progressbar = progressbar.clone();
                            let progress_box_clone = progress_box_clone.clone();
                            let window_clone = window_clone_.clone();
                            let list_box_clone = list_box_clone.clone();
                            progressbar.pulse();
                            glib::timeout_add_local(Duration::from_millis(100), move || {
                                match receiver.try_recv() {
                                    Ok((p, r)) => {
                                        if p.is_some() {
                                            progressbar.pulse();
                                        }

                                        if let Some(r) = r {
                                            match r {
                                                Ok(_) => {
                                                    list_box_clone.remove(&selected_row);

                                                    AlertDialog::builder()
                                                        .message("Delete Complete")
                                                        .detail(format!(
                                                            "Succesfully deleted {}",
                                                            name.label()
                                                        ))
                                                        .build()
                                                        .show(Some(&*window_clone));
                                                }
                                                Err(e) => {
                                                    AlertDialog::builder()
                                                        .message("Delete Failed")
                                                        .detail(format!("An error occured: {}", e))
                                                        .build()
                                                        .show(Some(&*window_clone));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if let TryRecvError::Disconnected = e {
                                            progress_box_clone.remove(&*progressbar);
                                            return glib::ControlFlow::Break;
                                        }
                                    }
                                }
                                glib::ControlFlow::Continue
                            });
                        },
                    );
            }
        }
    });

    let window_clone = window.clone();
    let channel_ = channel.clone();
    let progress_box_ = progress_box.clone();
    let list_box_ = list_box.clone();
    upload_btn.connect_clicked(move |_| {
        let channel_clone = channel_.clone();
        let window_clone_ = window_clone.clone();
        let http_clone = http.clone();
        let progress_box_clone = progress_box_.clone();
        let list_box_ = list_box_.clone();
        FileDialog::builder()
            .title("Upload")
            .accept_label("Upload")
            .build()
            .open(
                Some(&*window_clone),
                Some(&Cancellable::new()),
                move |res| {
                    if let Err(e) = res {
                        if e.message() == "Dismissed by user" {
                            return;
                        }

                        AlertDialog::builder()
                            .message("Error")
                            .detail(format!("{}", e).as_str())
                            .build()
                            .show(Some(&*window_clone_));
                        return;
                    }

                    let res = res.unwrap();
                    let path = res.path().unwrap();
                    let name = res
                        .query_info(
                            FILE_ATTRIBUTE_STANDARD_NAME,
                            FileQueryInfoFlags::NONE,
                            Some(&Cancellable::new()),
                        )
                        .unwrap()
                        .name();
                    println!("{}", path.display());

                    let (sender, receiver) = mpsc::channel();

                    let progressbar = Rc::new(
                        ProgressBar::builder()
                            .visible(true)
                            .show_text(true)
                            .valign(Align::Fill)
                            .build(),
                    );
                    progressbar.set_text(Some(format!("Uploading {}", name.display()).as_str()));
                    progressbar.set_fraction(0.0);

                    progress_box_clone.append(&*progressbar);

                    let file = Arc::new(Mutex::new(FileEntry::default()));
                    let id = Arc::new(AtomicU64::new(0));

                    let http_ = http_clone.clone();
                    let file_ = file.clone();
                    let id_ = id.clone();
                    let channel_ = channel_clone.borrow().inner().to_owned();
                    tokio::spawn(async move {
                        let res =
                            upload_internal(&http_, path, channel_.parse().unwrap(), |s, f| {
                                sender.send((Some((s, f)), None)).unwrap();
                            })
                            .await;

                        match res {
                            Ok(v) => {
                                let content = ChannelId::new(channel_.parse().unwrap())
                                    .message(&http_, v[0].id)
                                    .await
                                    .unwrap()
                                    .content;
                                let mut f_lock = file_.lock().unwrap();
                                *f_lock = FileEntry::from_str(&content).unwrap();
                                id_.store(v[0].id.into(), Ordering::SeqCst);
                            }
                            Err(e) => sender.send((None, Some(e))).unwrap(),
                        }
                    });

                    let progress_clone = progressbar.clone();
                    let progress_box_clone = progress_box_clone.clone();
                    let file_ = file.clone();
                    let id_ = id.clone();
                    let list_box_ = list_box_.clone();
                    // let channel_ = channel_clone.clone();
                    // let http_ = http_clone.clone();
                    glib::timeout_add_local(Duration::from_millis(100), move || {
                        match receiver.try_recv() {
                            Ok(res) => {
                                if let Some(f) = res.0 {
                                    progress_clone.set_text(Some(&f.0));
                                    progress_clone.set_fraction(f.1);
                                }

                                if let Some(e) = res.1 {
                                    progress_box_clone.remove(&*progress_clone);

                                    AlertDialog::builder()
                                        .message("Error")
                                        .detail(format!(
                                            "An error occured during installation: {}",
                                            e
                                        ))
                                        .build()
                                        .show(Some(&*window_clone_));
                                    return glib::ControlFlow::Break;
                                }
                            }
                            Err(e) => {
                                if let TryRecvError::Disconnected = e {
                                    progress_box_clone.remove(&*progress_clone);

                                    let row = ListBoxRow::new();
                                    let box_ = Box::new(Orientation::Vertical, 5);
                                    box_.set_halign(Align::Start);

                                    let file = file_.lock().unwrap();
                                    let id = id_.load(Ordering::SeqCst);
                                    let name_label = Label::new(file.name.as_deref());
                                    let id_label = Label::new(Some(&format!("ID: {}", id)));
                                    let size_label = Label::new(Some(&format!(
                                        "{}",
                                        HumanBytes(file.size.unwrap())
                                    )));

                                    name_label.set_halign(Align::Start);
                                    id_label.set_halign(Align::Start);
                                    size_label.set_halign(Align::Start);

                                    size_label.set_opacity(0.5);
                                    id_label.set_opacity(0.5);

                                    size_label.set_margin_start(20);
                                    id_label.set_margin_start(20);

                                    box_.append(&name_label);
                                    box_.append(&size_label);
                                    box_.append(&id_label);

                                    row.set_child(Some(&box_));
                                    list_box_.prepend(&row);

                                    // let msg = async_std::task::block_on(ChannelId::new(channel_.inner().parse().unwrap()).message(&http_, id)).unwrap();
                                    // let link = async_std::task::block_on(msg.link_ensured(&http_));
                                    AlertDialog::builder()
                                        .message("Upload complete")
                                        .detail(format!("Uploaded file {}", name.display()))
                                        .build()
                                        .show(Some(&*window_clone_));
                                    return glib::ControlFlow::Break;
                                }
                            }
                        }
                        glib::ControlFlow::Continue
                    });
                },
            )
    });

    let list_box_clone = list_box.clone();
    let progress_box_clone = progress_box.clone();
    let window_clone = window.clone();
    let channel_ = channel.clone();
    let token_ = token.clone();
    download_btn.connect_clicked(move |_| {
        if let Some(selected_row) = list_box_clone.selected_row() {
            if let Some(box_) = selected_row.child().and_then(|w| w.downcast::<Box>().ok()) {
                let mut labels: Vec<Label> = Vec::new();

                let first_child: Label = box_.first_child().unwrap().downcast().unwrap();
                labels.push(first_child.clone());

                let mut current: Option<Label> = Some(first_child);

                while let Some(curr) = current {
                    current = match curr.next_sibling() {
                        Some(v) => v.downcast().ok(),
                        None => None,
                    };

                    if current.is_some() {
                        labels.push(current.clone().unwrap());
                    }
                }

                for (i, label) in labels.iter().enumerate() {
                    println!("Label {}: {}", i, label.label());
                }

                let mut iter = labels.iter();
                let name = iter.next().unwrap();
                let size = iter.next().unwrap().label().replace("Size: ", "");
                let id = iter
                    .next()
                    .unwrap()
                    .label()
                    .replace("ID: ", "")
                    .parse::<u64>()
                    .unwrap();

                let progressbar = Rc::new(
                    ProgressBar::builder()
                        .visible(true)
                        .show_text(true)
                        .valign(Align::Fill)
                        .build(),
                );
                progressbar.set_text(Some(format!("Downloading {}", name.label()).as_str()));
                progressbar.set_fraction(0.0);

                progress_box_clone.append(&*progressbar);

                let path = Arc::new(Mutex::new(PathBuf::new()));

                let channel = channel_.borrow().inner().to_owned();
                let token = token_.borrow().inner().to_owned();
                let window_clone_ = window_clone.clone();
                let p = path.clone();

                let (sender, receiver) = mpsc::channel();
                tokio::task::spawn(async move {
                    let sender_ = sender.clone();
                    let channel = match channel.parse() {
                        Ok(v) => v,
                        Err(_) => {
                            sender
                                .send((None, Some(anyhow!("Invalid Channel ID"))))
                                .unwrap();
                            return;
                        }
                    };
                    let result =
                        download_internal(&Http::new(&token), id, channel, None, move |fraction| {
                            sender_.send((Some(fraction), None)).unwrap();
                        })
                        .await;

                    match result {
                        Ok(r) => {
                            let mut borrow = p.lock().unwrap();
                            *borrow = r;
                        }
                        Err(e) => {
                            sender.send((None, Some(e))).unwrap();
                        }
                    }
                });

                let progress_clone = progressbar.clone();
                let progress_box_clone = progress_box_clone.clone();
                glib::timeout_add_local(Duration::from_millis(100), move || {
                    match receiver.try_recv() {
                        Ok(res) => {
                            if let Some(f) = res.0 {
                                progress_clone.set_fraction(f);
                            }

                            if let Some(e) = res.1 {
                                progress_box_clone.remove(&*progress_clone);

                                AlertDialog::builder()
                                    .message("Error")
                                    .detail(format!("An error occured during download: {}", e))
                                    .build()
                                    .show(Some(&*window_clone_));
                                return glib::ControlFlow::Break;
                            }
                        }
                        Err(e) => {
                            if let TryRecvError::Disconnected = e {
                                progress_box_clone.remove(&*progress_clone);
                                AlertDialog::builder()
                                    .message("Download complete")
                                    .detail(format!(
                                        "Downloaded file {} ({})",
                                        path.lock().unwrap().display(),
                                        size
                                    ))
                                    .build()
                                    .show(Some(&*window_clone_));
                                return glib::ControlFlow::Break;
                            }
                        }
                    }
                    glib::ControlFlow::Continue
                });
            }
        }
    });

    container.append(&button_box);

    parent_box.append(&*container);
    parent_box.append(&*progress_box);

    window.set_child(Some(&*parent_box));

    window.present();
}
