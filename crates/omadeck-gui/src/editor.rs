//! The right-hand panel: what a key or dial does and how it looks.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use omadeck_core::{Action, AudioDevice, DialConfig, HOME, KeyConfig, Rgb, action, paths};

use crate::app::{App, Target};
use crate::{apps, media};

pub fn build_key(app: &Rc<App>, i: u8) -> gtk::Widget {
    let key = app.key(i).unwrap_or_else(|| KeyConfig::new(i));
    let deck = app.deck_info();
    let page = adw::PreferencesPage::new();
    let page_name = app.current_page();

    // --- action ---------------------------------------------------------------------------
    let group = adw::PreferencesGroup::builder()
        .title(format!("Key {}", i + 1))
        .description(format!(
            "{} · Row {} · Column {}",
            page_name.as_deref().unwrap_or(HOME),
            i / deck.cols + 1,
            i % deck.cols + 1
        ))
        .build();

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let test = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(if key.action == Action::Back { "Back only works on the deck" } else { "Try it now" })
        .valign(gtk::Align::Center)
        .sensitive(match &key.action {
            Action::None | Action::Back => false,
            Action::Page { page } => app.config.borrow().has_page(page),
            _ => true,
        })
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    test.connect_clicked(move |_| {
        let act = a.key(i).map(|k| k.action).unwrap_or_default();
        if let Action::Page { page } = act {
            a.show_page(Some(page), true);
        } else if let Err(e) = action::run(&act) {
            a.toast(&format!("{e:#}"));
        }
    });
    let clear = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Clear this key")
        .valign(gtk::Align::Center)
        .sensitive(!key.is_blank())
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    clear.connect_clicked(move |_| a.clear_key(i));
    buttons.append(&test);
    buttons.append(&clear);
    group.set_header_suffix(Some(&buttons));

    action_editor(
        app,
        &group,
        "When pressed",
        move |a| Target::Key(a.key_ref(i)),
        move |a| a.key(i).map(|k| k.action.clone()).unwrap_or_default(),
        move |a, act| {
            let mut action = act;
            if let Action::Page { page } = &mut action {
                // Nothing to choose from yet: make the page, like creating a folder.
                let others = a.config.borrow().pages.iter().any(|p| Some(&p.name) != a.current_page().as_ref());
                if !others {
                    *page = new_folder(a, i);
                }
            }
            a.edit(i, |k| k.action = action);
        },
        move |a, t, act| {
            if let Target::Key(k) = t {
                a.edit_ref(&k, |k| k.action = act);
            }
        },
        move |a, t, command, name, icon| {
            if let Target::Key(k) = t {
                a.edit_ref(k, |k| {
                    k.action = Action::App { command };
                    k.label = name;
                    if icon.is_some() {
                        k.icon = icon;
                    }
                });
            }
        },
        move |a| new_folder(a, i),
        Some(Rc::new(move |a, t, icon, name| {
            if let Target::Key(k) = t {
                a.edit_ref(&k, |k| {
                    k.icon = Some(icon);
                    if k.label.is_empty() {
                        k.label = name;
                    }
                });
            }
        })),
    );
    page.add(&group);

    // --- appearance -----------------------------------------------------------------------
    let look = adw::PreferencesGroup::builder().title("Appearance").build();

    let label = adw::EntryRow::builder().title("Label").text(&key.label).build();
    let a = app.clone();
    label.connect_changed(move |e| {
        let text = e.text().to_string();
        a.edit(i, |k| k.label = text);
    });
    look.add(&label);

    let show = adw::SwitchRow::builder().title("Show label on key").active(key.show_label).build();
    let a = app.clone();
    show.connect_active_notify(move |r| {
        let on = r.is_active();
        a.edit(i, |k| k.show_label = on);
    });
    look.add(&show);

    look.add(&icon_row(
        app,
        key.icon.as_deref(),
        move |a| Target::Key(a.key_ref(i)),
        move |a, t, v| {
            if let Target::Key(k) = t {
                a.edit_ref(&k, |k| k.icon = v);
            }
        },
    ));
    look.add(&glyph_row(app, key.glyph.as_deref(), move |a, v| a.edit(i, |k| k.glyph = v)));

    let (bg, fg) = {
        let cfg = app.config.borrow();
        let r = app.renderer.borrow();
        let face = key.face();
        let bg = r.background(Some(&face), &cfg.style);
        (bg, r.label_color(Some(&face), &cfg.style, bg))
    };
    look.add(&color_row(app, "Background", bg, key.background.is_some(), move |a, c| a.edit(i, |k| k.background = c)));
    look.add(&color_row(app, "Label colour", fg, key.label_color.is_some(), move |a, c| {
        a.edit(i, |k| k.label_color = c)
    }));
    page.add(&look);

    page.upcast()
}

pub fn build_dial(app: &Rc<App>, i: u8) -> gtk::Widget {
    let dial = app.config.borrow().dial(i).cloned().unwrap_or_else(|| DialConfig::new(i));
    let page = adw::PreferencesPage::new();

    // --- dial -----------------------------------------------------------------------------
    let group = adw::PreferencesGroup::builder()
        .title(format!("Dial {}", i + 1))
        .description("Dials are numbered left to right, from 0")
        .build();

    let preset = gtk::Button::builder()
        .label("Volume dial")
        .tooltip_text("Set this dial to the output volume: turn to change it, press to mute")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    preset.connect_clicked(move |_| {
        a.edit_dial(i, |d| {
            d.label = "Volume".into();
            d.glyph = Some("f057e".into());
            d.turn_left = Action::Volume { device: AudioDevice::Output, change: -5, mute: false };
            d.turn_right = Action::Volume { device: AudioDevice::Output, change: 5, mute: false };
            d.press = Action::Volume { device: AudioDevice::Output, change: 0, mute: true };
        });
        a.rebuild_editor();
    });
    let clear = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Clear this dial")
        .valign(gtk::Align::Center)
        .sensitive(!dial.is_blank())
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    clear.connect_clicked(move |_| a.clear_dial(i));
    let header_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header_buttons.append(&preset);
    header_buttons.append(&clear);
    group.set_header_suffix(Some(&header_buttons));

    let label = adw::EntryRow::builder().title("Label").text(&dial.label).build();
    let a = app.clone();
    label.connect_changed(move |e| {
        let text = e.text().to_string();
        a.edit_dial(i, |d| d.label = text);
    });
    group.add(&label);
    page.add(&group);

    page.add(&dial_action_group(app, i, "Turn left", |d| d.turn_left.clone(), |d, a| d.turn_left = a));
    page.add(&dial_action_group(app, i, "Turn right", |d| d.turn_right.clone(), |d, a| d.turn_right = a));
    page.add(&dial_action_group(app, i, "Press", |d| d.press.clone(), |d, a| d.press = a));

    // --- appearance (the strip panel) -----------------------------------------------------
    let look = adw::PreferencesGroup::builder()
        .title("Appearance")
        .description("How the dial's panel looks on the touch strip")
        .build();

    let show = adw::SwitchRow::builder().title("Show label on strip").active(dial.show_label).build();
    let a = app.clone();
    show.connect_active_notify(move |r| {
        let on = r.is_active();
        a.edit_dial(i, |d| d.show_label = on);
    });
    look.add(&show);

    look.add(&icon_row(
        app,
        dial.icon.as_deref(),
        move |_| Target::Dial(i),
        move |a, t, v| {
            if let Target::Dial(j) = t {
                a.edit_dial(j, |d| d.icon = v);
            }
        },
    ));
    look.add(&glyph_row(app, dial.glyph.as_deref(), move |a, v| a.edit_dial(i, |d| d.glyph = v)));

    let (bg, fg) = {
        let cfg = app.config.borrow();
        let r = app.renderer.borrow();
        let face = dial.face();
        let bg = r.background(Some(&face), &cfg.style);
        (bg, r.label_color(Some(&face), &cfg.style, bg))
    };
    look.add(&color_row(app, "Background", bg, dial.background.is_some(), move |a, c| {
        a.edit_dial(i, |d| d.background = c)
    }));
    look.add(&color_row(app, "Label colour", fg, dial.label_color.is_some(), move |a, c| {
        a.edit_dial(i, |d| d.label_color = c)
    }));
    page.add(&look);

    page.upcast()
}

/// One dial direction: a "Turn left" / "Turn right" / "Press" group with a ▶ test button.
fn dial_action_group(
    app: &Rc<App>,
    i: u8,
    title: &str,
    get: impl Fn(&DialConfig) -> Action + Clone + 'static,
    set: impl Fn(&mut DialConfig, Action) + Clone + 'static,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();

    let act = app.config.borrow().dial(i).map(&get).unwrap_or_default();
    let test = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(if act == Action::Back { "Back only works on the deck" } else { "Try it now" })
        .valign(gtk::Align::Center)
        .sensitive(match &act {
            Action::None | Action::Back => false,
            Action::Page { page } => app.config.borrow().has_page(page),
            _ => true,
        })
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    let get_test = get.clone();
    test.connect_clicked(move |_| {
        let act = a.config.borrow().dial(i).map(&get_test).unwrap_or_default();
        if let Action::Page { page } = act {
            a.show_page(Some(page), true);
        } else if let Err(e) = action::run(&act) {
            a.toast(&format!("{e:#}"));
        }
    });
    group.set_header_suffix(Some(&test));

    let set1 = set.clone();
    let set2 = set.clone();
    let set3 = set.clone();
    action_editor(
        app,
        &group,
        "Action",
        move |_| Target::Dial(i),
        move |a| a.config.borrow().dial(i).map(&get).unwrap_or_default(),
        move |a, act| a.edit_dial(i, |d| set1(d, act)),
        move |a, t, act| {
            if let Target::Dial(j) = t {
                a.edit_dial(j, |d| set2(d, act));
            }
        },
        move |a, t, command, name, _| {
            if let Target::Dial(j) = t {
                // Only the direction's action changes; the dial's label is shared by its
                // three actions, so picking an app for one must not rename the dial.
                let _ = name;
                a.edit_dial(*j, |d| set3(d, Action::App { command }));
            }
        },
        move |a| {
            let label = a.config.borrow().dial(i).map(|d| d.label.clone()).unwrap_or_default();
            a.add_page(if label.trim().is_empty() { "Page" } else { &label })
        },
        None,
    );
    group
}

/// Handles a fetched website icon: (app, target, imported icon path, site name).
type FaviconHandler = Rc<dyn Fn(&Rc<App>, Target, String, String) + 'static>;

/// Fill `group` with the action combo and the rows for the selected action. `get` and
/// `edit` read and replace the action of whatever is being edited (a key or a dial
/// direction); `target` captures it for work that finishes later (a picker, an icon
/// fetch), so the result lands where it started. `on_app_picked` applies the extras an
/// app pick carries (name and icon); `on_new_page` creates the page a "New page" button
/// points the action at; `on_favicon`, when given, handles the website icon fetch.
#[allow(clippy::too_many_arguments)]
fn action_editor(
    app: &Rc<App>,
    group: &adw::PreferencesGroup,
    combo_title: &str,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    get: impl Fn(&Rc<App>) -> Action + Clone + 'static,
    edit: impl Fn(&Rc<App>, Action) + Clone + 'static,
    edit_at: impl Fn(&Rc<App>, Target, Action) + Clone + 'static,
    on_app_picked: impl Fn(&Rc<App>, &Target, String, String, Option<String>) + Clone + 'static,
    on_new_page: impl Fn(&Rc<App>) -> String + Clone + 'static,
    on_favicon: Option<FaviconHandler>,
) {
    let action = get(app);
    let kinds = gtk::StringList::new(&Action::KINDS);
    let kind_row = adw::ComboRow::builder().title(combo_title).model(&kinds).build();
    kind_row.set_selected(action.kind_index() as u32);
    match action {
        Action::Page { .. } => kind_row.set_subtitle("Shows another page of keys on the deck"),
        Action::Back => kind_row.set_subtitle("Returns to the page shown before this one, or Home"),
        _ => {}
    }
    let a = app.clone();
    kind_row.connect_selected_notify({
        let get = get.clone();
        let edit = edit.clone();
        move |row| {
            let sel = row.selected() as usize;
            let cur = get(&a).kind_index();
            if sel != cur {
                edit(&a, Action::empty_of_kind(sel));
                a.rebuild_editor();
            }
        }
    });
    group.add(&kind_row);
    action_rows(app, group, target, get, edit, edit_at, on_app_picked, on_new_page, on_favicon);
}

#[allow(clippy::too_many_arguments)]
fn action_rows(
    app: &Rc<App>,
    group: &adw::PreferencesGroup,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    get: impl Fn(&Rc<App>) -> Action + Clone + 'static,
    edit: impl Fn(&Rc<App>, Action) + Clone + 'static,
    edit_at: impl Fn(&Rc<App>, Target, Action) + Clone + 'static,
    on_app_picked: impl Fn(&Rc<App>, &Target, String, String, Option<String>) + Clone + 'static,
    on_new_page: impl Fn(&Rc<App>) -> String + Clone + 'static,
    on_favicon: Option<FaviconHandler>,
) {
    let action = get(app);
    match &action {
        Action::None => {}

        Action::App { command } => {
            let found = apps::find(command);
            let row =
                adw::ActionRow::builder()
                    .title("Application")
                    .subtitle(found.as_ref().map(|a| a.display_name().to_string()).unwrap_or_else(|| {
                        if command.is_empty() { "None chosen".into() } else { "Custom command".into() }
                    }))
                    .use_markup(false)
                    .build();
            if let Some(gicon) = found.as_ref().and_then(|a| a.icon()) {
                let img = gtk::Image::from_gicon(&gicon);
                img.set_pixel_size(32);
                row.add_prefix(&img);
            }
            let choose = gtk::Button::builder().label("Choose…").valign(gtk::Align::Center).build();
            let a = app.clone();
            let target = target.clone();
            let on_app_picked = on_app_picked.clone();
            choose.connect_clicked(move |_| {
                let at = target(&a);
                let on_app_picked = on_app_picked.clone();
                let a2 = a.clone();
                apps::pick(&a.window, move |info| {
                    let command = info
                        .id()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| info.commandline().map(|c| c.to_string_lossy().into()).unwrap_or_default());
                    let icon = media::import_app_icon(&info);
                    let name = info.display_name().to_string();
                    on_app_picked(&a2, &at, command, name, icon);
                    a2.rebuild_editor();
                });
            });
            row.add_suffix(&choose);
            row.set_activatable_widget(Some(&choose));
            group.add(&row);

            let cmd = adw::EntryRow::builder().title("Desktop entry or command").text(command).build();
            let a = app.clone();
            let edit = edit.clone();
            cmd.connect_changed(move |e| {
                let command = e.text().to_string();
                edit(&a, Action::App { command });
            });
            group.add(&cmd);
        }

        Action::Script { path, terminal } => {
            let row = adw::EntryRow::builder().title("Script").text(path).build();
            let a = app.clone();
            let t = *terminal;
            let edit_row = edit.clone();
            row.connect_changed(move |e| {
                let path = e.text().to_string();
                edit_row(&a, Action::Script { path, terminal: t });
            });
            row.add_suffix(&browse_button(app, target.clone(), "Choose a script", None, move |a, tg, p| {
                let path = paths::portable(&p);
                edit_at(a, tg, Action::Script { path, terminal: t });
            }));
            group.add(&row);

            let term = adw::SwitchRow::builder()
                .title("Run in a terminal")
                .subtitle("Opens your terminal so you can watch the output")
                .active(*terminal)
                .build();
            let a = app.clone();
            let get = get.clone();
            let edit_switch = edit.clone();
            term.connect_active_notify(move |r| {
                let on = r.is_active();
                // Read the action out of the borrow before editing (a `Ref` held across
                // the edit would panic in `borrow_mut`).
                let mut act = get(&a);
                if let Action::Script { terminal, .. } = &mut act {
                    *terminal = on;
                }
                edit_switch(&a, act);
                a.rebuild_editor();
            });
            group.add(&term);
        }

        Action::Sound { file, volume } => {
            let row = adw::EntryRow::builder().title("Sound file").text(file).build();
            let a = app.clone();
            let v = *volume;
            let edit_row = edit.clone();
            row.connect_changed(move |e| {
                let file = e.text().to_string();
                edit_row(&a, Action::Sound { file, volume: v });
            });
            row.add_suffix(&browse_button(
                app,
                target.clone(),
                "Choose a sound",
                Some(("Audio", "audio/*")),
                move |a, tg, p| {
                    let file = paths::portable(&p);
                    edit_at(a, tg, Action::Sound { file, volume: v });
                },
            ));
            group.add(&row);

            let vol = adw::SpinRow::with_range(0.0, 150.0, 5.0);
            vol.set_title("Volume (%)");
            vol.set_value((volume.unwrap_or(1.0) * 100.0).round() as f64);
            let a = app.clone();
            let get = get.clone();
            let edit_vol = edit.clone();
            vol.connect_value_notify(move |r| {
                let pct = r.value() as f32 / 100.0;
                let mut act = get(&a);
                if let Action::Sound { volume, .. } = &mut act {
                    *volume = if (pct - 1.0).abs() < f32::EPSILON { None } else { Some(pct) };
                }
                edit_vol(&a, act);
            });
            group.add(&vol);
        }

        Action::Url { url } => {
            let row = adw::EntryRow::builder().title("Website").text(url).input_purpose(gtk::InputPurpose::Url).build();
            let a = app.clone();
            let edit = edit.clone();
            row.connect_changed(move |e| {
                let url = e.text().to_string();
                edit(&a, Action::Url { url });
            });
            group.add(&row);

            if let Some(on_favicon) = on_favicon {
                let fetch_row = adw::ActionRow::builder()
                    .title("Site icon")
                    .subtitle(
                        "Asks the website for its icon. If it has none, DuckDuckGo's icon service is used instead",
                    )
                    .build();
                let fetch = gtk::Button::builder().label("Fetch").valign(gtk::Align::Center).build();
                let a = app.clone();
                let get = get.clone();
                let target = target.clone();
                fetch.connect_clicked(move |btn| {
                    let url = match get(&a) {
                        Action::Url { url } if !url.trim().is_empty() => url,
                        _ => {
                            a.toast("Enter a website first");
                            return;
                        }
                    };
                    btn.set_sensitive(false);
                    btn.set_label("Fetching…");
                    let a = a.clone();
                    let btn = btn.clone();
                    let at = target(&a);
                    let on_favicon = on_favicon.clone();
                    glib::spawn_future_local(async move {
                        let u = url.clone();
                        let res = gio::spawn_blocking(move || media::fetch_favicon(&u)).await;
                        btn.set_sensitive(true);
                        btn.set_label("Fetch");
                        match res {
                            Ok(Ok((icon, source))) => {
                                if source == media::IconSource::DuckDuckGo {
                                    a.toast("The site had no usable icon, so this one came from DuckDuckGo");
                                }
                                let name = media::host_of(&url).map(|h| site_name(&h)).unwrap_or_default();
                                on_favicon(&a, at, icon, name);
                                a.rebuild_editor();
                            }
                            Ok(Err(e)) => a.toast(&format!("Couldn't fetch the icon: {e:#}")),
                            Err(_) => a.toast("Couldn't fetch the icon"),
                        }
                    });
                });
                fetch_row.add_suffix(&fetch);
                group.add(&fetch_row);
            }
        }

        Action::Hyprland { dispatch } => {
            group.set_description(Some(
                "Runs hyprctl dispatch. Classic syntax like workspace 3 · togglefloating · killactive · exec pavucontrol \
                 is translated for you, or write Lua directly: hl.dsp.focus({ workspace = \"3\" })",
            ));
            let row = adw::EntryRow::builder().title("Dispatcher and arguments").text(dispatch).build();
            let a = app.clone();
            let edit = edit.clone();
            row.connect_changed(move |e| {
                let dispatch = e.text().to_string();
                edit(&a, Action::Hyprland { dispatch });
            });
            group.add(&row);
        }

        Action::Page { page } => {
            // A key can't open the page it's on; a dial isn't on a page, so it can open any.
            let own = match target(app) {
                Target::Key(k) => k.page().map(str::to_string),
                _ => None,
            };
            let names: Vec<String> =
                app.config.borrow().page_names().into_iter().filter(|n| Some(n) != own.as_ref()).collect();
            // A combo row always shows some item, so an unset or missing target gets a placeholder.
            let mut choices: Vec<Option<String>> = names.iter().cloned().map(Some).collect();
            if !names.contains(page) {
                choices.insert(0, None);
            }
            let placeholder = if page.is_empty() { "Choose a page".to_string() } else { format!("{page} (missing)") };
            let labels: Vec<&str> = choices.iter().map(|c| c.as_deref().unwrap_or(&placeholder)).collect();
            let row = adw::ComboRow::builder().title("Page").model(&gtk::StringList::new(&labels)).build();
            row.set_selected(choices.iter().position(|c| c.as_deref() == Some(page.as_str())).unwrap_or(0) as u32);
            let a = app.clone();
            let edit_row = edit.clone();
            row.connect_selected_notify(move |r| {
                if let Some(Some(name)) = choices.get(r.selected() as usize).cloned() {
                    edit_row(&a, Action::Page { page: name });
                    a.rebuild_editor();
                }
            });
            group.add(&row);

            let buttons = adw::ActionRow::builder().title("Open it to add keys").build();
            let open = gtk::Button::builder()
                .icon_name("go-next-symbolic")
                .tooltip_text("Edit this page")
                .valign(gtk::Align::Center)
                .sensitive(names.contains(page))
                .css_classes(["flat"])
                .build();
            let a = app.clone();
            let target = page.clone();
            open.connect_clicked(move |_| a.show_page(Some(target.clone()), true));
            let new = gtk::Button::builder().label("New page").valign(gtk::Align::Center).build();
            let a = app.clone();
            let edit_new = edit.clone();
            let on_new_page = on_new_page.clone();
            new.connect_clicked(move |_| {
                let name = on_new_page(&a);
                edit_new(&a, Action::Page { page: name });
                a.rebuild_editor();
            });
            buttons.add_suffix(&new);
            buttons.add_suffix(&open);
            buttons.set_activatable_widget(Some(&open));
            group.add(&buttons);
        }

        Action::Volume { device, change, mute } => {
            group.set_description(Some(
                "Changes the system volume through Omarchy's own commands, so its pop-up shows. On a dial, the \
                 level is drawn on the touch strip.",
            ));

            let mute_row = adw::SwitchRow::builder()
                .title("Toggle mute")
                .subtitle("Press toggles mute instead of changing the level")
                .active(*mute)
                .build();
            mute_row.connect_active_notify({
                let a = app.clone();
                let get = get.clone();
                let edit = edit.clone();
                move |r| {
                    let on = r.is_active();
                    // Read the action out of the borrow before editing (a `Ref` held
                    // across the edit would panic in `borrow_mut`).
                    let mut act = get(&a);
                    if let Action::Volume { mute, .. } = &mut act {
                        *mute = on;
                    }
                    edit(&a, act);
                    a.rebuild_editor();
                }
            });
            group.add(&mute_row);

            if !*mute {
                let dev_kinds = gtk::StringList::new(&["Output", "Input"]);
                let dev_row = adw::ComboRow::builder().title("Device").model(&dev_kinds).build();
                dev_row.set_selected(if device.is_output() { 0 } else { 1 });
                dev_row.connect_selected_notify({
                    let a = app.clone();
                    let get = get.clone();
                    let edit = edit.clone();
                    move |row| {
                        let sel = row.selected() as usize;
                        let cur = match get(&a) {
                            Action::Volume { device, .. } => usize::from(!device.is_output()),
                            _ => 0,
                        };
                        if sel != cur {
                            let device = if sel == 1 { AudioDevice::Input } else { AudioDevice::Output };
                            let mut act = get(&a);
                            if let Action::Volume { device: d, .. } = &mut act {
                                *d = device;
                            }
                            edit(&a, act);
                            a.rebuild_editor();
                        }
                    }
                });
                group.add(&dev_row);

                let spin = adw::SpinRow::with_range(-25.0, 25.0, 1.0);
                spin.set_title("Change per turn (%)");
                spin.set_value(*change as f64);
                spin.connect_value_notify({
                    let a = app.clone();
                    let get = get.clone();
                    let edit = edit.clone();
                    move |r| {
                        let change = r.value().round() as i8;
                        let mut act = get(&a);
                        if let Action::Volume { change: c, .. } = &mut act {
                            *c = change;
                        }
                        edit(&a, act);
                    }
                });
                group.add(&spin);
            }
        }

        Action::Back => {}
    }
}

/// Create a page for key `i` to open, and make the key look like a folder if it's plain.
fn new_folder(app: &Rc<App>, i: u8) -> String {
    let label = app.key(i).map(|k| k.label).unwrap_or_default();
    let name = app.add_page(if label.trim().is_empty() { "Page" } else { &label });
    let n = name.clone();
    app.edit(i, |k| {
        if k.icon.is_none() && k.glyph.is_none() {
            k.glyph = Some("\u{f024b}".into());
        }
        if k.label.is_empty() {
            k.label = n;
        }
    });
    name
}

/// "omarchy.org" → "Omarchy", "www.youtube.com" → "Youtube".
fn site_name(host: &str) -> String {
    let host = host.trim_start_matches("www.");
    let base = host.split('.').next().unwrap_or(host);
    let mut c = base.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The icon row, shared by the key and dial editors; edits through `set_icon`, which
/// takes the target captured for the async pickers, so a pick lands where it started.
fn icon_row(
    app: &Rc<App>,
    icon: Option<&str>,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    set_icon: impl Fn(&Rc<App>, Target, Option<String>) + Clone + 'static,
) -> adw::ActionRow {
    let subtitle = icon
        .map(|p| paths::resolve(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .unwrap_or_else(|| "None".into());
    let row = adw::ActionRow::builder().title("Icon").subtitle(subtitle).use_markup(false).build();

    let preview = gtk::Image::builder().pixel_size(40).css_classes(["icon-preview"]).build();
    if let Some(img) = icon.and_then(|p| omadeck_core::render::load_icon(&paths::resolve(p), 80)) {
        preview.set_paintable(Some(&media::texture(img)));
    } else {
        preview.set_icon_name(Some("image-x-generic-symbolic"));
    }
    row.add_prefix(&preview);

    if icon.is_some() {
        let clear = gtk::Button::builder()
            .icon_name("edit-clear-symbolic")
            .tooltip_text("Remove icon")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        clear.connect_clicked({
            let a = app.clone();
            let target = target.clone();
            let set_icon = set_icon.clone();
            move |_| {
                let at = target(&a);
                set_icon(&a, at, None);
                a.rebuild_editor();
            }
        });
        row.add_suffix(&clear);
    }

    let choose = gtk::Button::builder().label("Choose…").valign(gtk::Align::Center).build();
    choose.connect_clicked({
        let a = app.clone();
        let target = target.clone();
        let set_icon = set_icon.clone();
        move |_| {
            let a2 = a.clone();
            let set_icon = set_icon.clone();
            let at = target(&a2);
            let dialog = file_dialog("Choose an icon", Some(("Images", "image/*")));
            glib::spawn_future_local(async move {
                let Ok(file) = dialog.open_future(Some(&a2.window)).await else { return };
                let Some(path) = file.path() else { return };
                match media::import_icon(&path) {
                    Ok(rel) => {
                        set_icon(&a2, at, Some(rel));
                        a2.rebuild_editor();
                    }
                    Err(e) => a2.toast(&format!("{e:#}")),
                }
            });
        }
    });
    row.add_suffix(&choose);
    row
}

/// The Nerd Font glyph row, shared by the key and dial editors; edits through `set_glyph`.
fn glyph_row(
    app: &Rc<App>,
    glyph: Option<&str>,
    set_glyph: impl Fn(&Rc<App>, Option<String>) + Clone + 'static,
) -> adw::EntryRow {
    let row = adw::EntryRow::builder()
        .title("Symbol (Nerd Font glyph, used when there's no image)")
        .text(glyph.unwrap_or(""))
        .build();
    let cheat = gtk::Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("Browse Nerd Font symbols — copy one and paste it here")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    cheat.connect_clicked(|_| {
        let _ =
            gio::AppInfo::launch_default_for_uri("https://www.nerdfonts.com/cheat-sheet", gio::AppLaunchContext::NONE);
    });
    row.add_suffix(&cheat);
    row.connect_changed({
        let a = app.clone();
        move |e| {
            let text = e.text().trim().to_string();
            set_glyph(&a, (!text.is_empty()).then_some(text));
        }
    });
    row
}

/// A colour row with a reset-to-theme button, shared by the key and dial editors;
/// edits through `set`.
fn color_row(
    app: &Rc<App>,
    title: &str,
    current: Rgb,
    overridden: bool,
    set: impl Fn(&Rc<App>, Option<String>) + Clone + 'static,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).subtitle(if overridden { "Custom" } else { "From theme" }).build();
    if overridden {
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .tooltip_text("Use the theme colour")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        reset.connect_clicked({
            let a = app.clone();
            let set = set.clone();
            move |_| {
                set(&a, None);
                a.rebuild_editor();
            }
        });
        row.add_suffix(&reset);
    }
    let button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(false).build()));
    button.set_valign(gtk::Align::Center);
    button.set_rgba(&gdk::RGBA::new(current.0 as f32 / 255.0, current.1 as f32 / 255.0, current.2 as f32 / 255.0, 1.0));
    button.connect_rgba_notify({
        let a = app.clone();
        let set = set.clone();
        move |b| {
            let c = b.rgba();
            let to = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
            let hex = Rgb(to(c.red()), to(c.green()), to(c.blue())).hex();
            set(&a, Some(hex));
            a.rebuild_editor();
        }
    });
    row.add_suffix(&button);
    row
}

fn file_dialog(title: &str, filter: Option<(&str, &str)>) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    if let Some((name, mime)) = filter {
        let f = gtk::FileFilter::new();
        f.set_name(Some(name));
        f.add_mime_type(mime);
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&f);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&f));
    }
    dialog
}

fn browse_button(
    app: &Rc<App>,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    title: &'static str,
    filter: Option<(&'static str, &'static str)>,
    on_file: impl Fn(&Rc<App>, Target, std::path::PathBuf) + Clone + 'static,
) -> gtk::Button {
    let btn = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text(title)
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    btn.connect_clicked(move |_| {
        let dialog = file_dialog(title, filter);
        let a = a.clone();
        let on_file = on_file.clone();
        let at = target(&a);
        glib::spawn_future_local(async move {
            if let Ok(file) = dialog.open_future(Some(&a.window)).await
                && let Some(p) = file.path()
            {
                on_file(&a, at, p);
                a.rebuild_editor();
            }
        });
    });
    btn
}
