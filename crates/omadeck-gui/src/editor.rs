//! The right-hand panel: what a key or dial does and how it looks.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use omadeck_core::{
    Action, AudioDevice, DialConfig, Direction, FaceTab, HOME, KeyConfig, LookField, Rgb, action, paths,
};

use crate::app::{ActionTarget, App, Target};
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

    let slot = ActionSlot {
        target: ActionTarget::Key(app.key_ref(i)),
        dir: None,
        get: Rc::new(move |a| a.key(i).map(|k| k.action).unwrap_or_default()),
        set: Rc::new(move |a, act| {
            let mut action = act;
            if let Action::Page { page } = &mut action {
                // Nothing to choose from yet: make the page, like creating a folder.
                let others = a.config.borrow().pages.iter().any(|p| Some(&p.name) != a.current_page().as_ref());
                if !others {
                    *page = new_folder(a, i);
                }
            }
            a.edit(i, |k| k.action = action);
        }),
        own_page: app.current_page(),
        on_app_picked: Rc::new(|a, at, command, name, icon| {
            if let ActionTarget::Key(k) = at {
                a.edit_ref(k, |k| {
                    k.action = Action::App { command };
                    k.label = name;
                    if icon.is_some() {
                        k.icon = icon;
                    }
                });
            }
        }),
        on_new_page: Rc::new(move |a| new_folder(a, i)),
        on_favicon: Some(Rc::new(move |a, at, icon, name| {
            if let ActionTarget::Key(k) = at {
                a.edit_ref(&k, |k| {
                    k.icon = Some(icon);
                    if k.label.is_empty() {
                        k.label = name;
                    }
                });
            }
        })),
    };

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.append(&test_button(app, &slot));
    let clear = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Clear this key")
        .valign(gtk::Align::Center)
        .sensitive(!key.is_blank())
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    clear.connect_clicked(move |_| a.clear_key(i));
    buttons.append(&clear);
    group.set_header_suffix(Some(&buttons));

    action_editor(app, &group, "When pressed", &slot);
    page.add(&group);

    // --- appearance -----------------------------------------------------------------------
    let look = adw::PreferencesGroup::builder().title("Appearance").build();
    appearance_tabs(app, &look, &key);
    appearance_rows(app, &look, i, app.face_tab.get(), &key);
    page.add(&look);

    page.upcast()
}

/// The linked Off | On | Busy buttons in the Appearance header. On shows for toggles
/// (a key with an on face); Busy for slow toggles and busy-while-running keys.
fn appearance_tabs(app: &Rc<App>, group: &adw::PreferencesGroup, key: &KeyConfig) {
    let tabs = vec![("Off", FaceTab::Off), ("On", FaceTab::On), ("Busy", FaceTab::Busy)]
        .into_iter()
        .filter(|(_, tab)| match tab {
            FaceTab::Off => true,
            FaceTab::On => key.on.is_some(),
            FaceTab::Busy => key.is_slow_toggle() || key.busy_while_running,
        })
        .collect::<Vec<_>>();
    if tabs.len() < 2 {
        return;
    }
    let current = app.face_tab.get();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    buttons.add_css_class("linked");
    for (label, tab) in tabs {
        let b = gtk::ToggleButton::builder().label(label).active(tab == current).build();
        let a = app.clone();
        let self_btn = b.clone();
        b.connect_toggled(move |_| {
            if self_btn.is_active() {
                a.set_face_tab(tab);
            }
        });
        buttons.append(&b);
    }
    group.set_header_suffix(Some(&buttons));
}

/// The look rows for the face tab being edited, through `KeyConfig::look`/`set_look`.
fn appearance_rows(app: &Rc<App>, group: &adw::PreferencesGroup, i: u8, tab: FaceTab, key: &KeyConfig) {
    let while_in = |base: &str| match tab {
        FaceTab::Off => base.to_string(),
        FaceTab::On => format!("{base} while on"),
        FaceTab::Busy => format!("{base} while busy"),
    };
    let empty_says = match tab {
        FaceTab::On => " (empty: the key's label)",
        FaceTab::Busy => " (empty: the command's output)",
        FaceTab::Off => "",
    };

    let label = adw::EntryRow::builder()
        .title(format!("Label{empty_says}"))
        .text(key.look(tab, LookField::Label).unwrap_or_default())
        .build();
    let a = app.clone();
    label.connect_changed(move |e| {
        let text = e.text().to_string();
        a.edit(i, |k| k.set_look(tab, LookField::Label, Some(text)));
    });
    group.add(&label);

    if tab == FaceTab::Off {
        let show = adw::SwitchRow::builder().title("Show label on key").active(key.show_label).build();
        let a = app.clone();
        show.connect_active_notify(move |r| {
            let on = r.is_active();
            a.edit(i, |k| k.show_label = on);
        });
        group.add(&show);
    }

    group.add(&icon_row(
        app,
        key.look(tab, LookField::Icon).as_deref(),
        move |a| Target::Key(a.key_ref(i)),
        move |a, t, v| {
            if let Target::Key(k) = t {
                a.edit_ref(&k, |k| k.set_look(tab, LookField::Icon, v));
            }
        },
    ));
    group.add(&glyph_row(app, key.look(tab, LookField::Glyph).as_deref(), move |a, v| {
        a.edit(i, |k| k.set_look(tab, LookField::Glyph, v))
    }));

    let (bg, fg) = {
        let st = tab.preview();
        let cfg = app.config.borrow();
        let r = app.renderer.borrow();
        let face = key.face_for(&st);
        let bg = r.background(Some(&face), &cfg.style);
        (bg, r.label_color(Some(&face), &cfg.style, bg))
    };
    let overridden = |f: LookField| key.look(tab, f).is_some();
    group.add(&color_row(app, &while_in("Background"), bg, overridden(LookField::Background), None, move |a, c| {
        a.edit(i, |k| k.set_look(tab, LookField::Background, c))
    }));
    group.add(&color_row(app, &while_in("Label colour"), fg, overridden(LookField::LabelColor), None, move |a, c| {
        a.edit(i, |k| k.set_look(tab, LookField::LabelColor, c))
    }));
}

pub fn build_dial(app: &Rc<App>, i: u8) -> gtk::Widget {
    let dial = app.config.borrow().dial(i).cloned().unwrap_or_else(|| DialConfig::new(i));
    let page = adw::PreferencesPage::new();

    // --- dial -----------------------------------------------------------------------------
    let group = adw::PreferencesGroup::builder()
        .title(format!("Dial {}", i + 1))
        .description("Dials are numbered left to right, from 0")
        .build();

    let presets = gtk::Box::new(gtk::Orientation::Horizontal, 6);
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
    presets.append(&preset);
    let preset = gtk::Button::builder()
        .label("Brightness dial")
        .tooltip_text("Set this dial to the panel brightness: turn to change it")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    preset.connect_clicked(move |_| {
        a.edit_dial(i, |d| {
            d.label = "Brightness".into();
            d.glyph = Some("f00de".into());
            d.turn_left = Action::Brightness { change: -5, set: None };
            d.turn_right = Action::Brightness { change: 5, set: None };
            // A preset sets every slot: a Volume press left over from the Volume preset
            // would make the panel show the volume.
            d.press = Action::None;
        });
        a.rebuild_editor();
    });
    presets.append(&preset);
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
    header_buttons.append(&presets);
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

    // A kind pick starts from what the slot implies: turning left lowers or goes back, a
    // Volume press mutes (`Action::default_for`).
    for dir in [Direction::Left, Direction::Right, Direction::Press] {
        page.add(&dial_action_group(app, i, dir));
    }

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
    look.add(&color_row(
        app,
        "Background",
        bg,
        dial.background.is_some(),
        dial.background_image.is_some().then_some("Under the image"),
        move |a, c| a.edit_dial(i, |d| d.background = c),
    ));
    look.add(&image_row(
        app,
        "Background image",
        "200 × 100; fills the panel and covers the strip image",
        dial.background_image.as_deref(),
        dial.background_image.as_deref().and_then(|p| omadeck_core::render::load_cover(&paths::resolve(p), 80, 40)),
        move |_| Target::Dial(i),
        move |a, t, v| {
            if let Target::Dial(j) = t {
                a.edit_dial(j, |d| d.background_image = v);
            }
        },
    ));
    look.add(&color_row(app, "Label colour", fg, dial.label_color.is_some(), None, move |a, c| {
        a.edit_dial(i, |d| d.label_color = c)
    }));
    page.add(&look);

    page.upcast()
}

/// One dial slot: a "Turn left" / "Turn right" / "Press" group with a ▶ test button.
fn dial_action_group(app: &Rc<App>, i: u8, dir: Direction) -> adw::PreferencesGroup {
    let title = match dir {
        Direction::Left => "Turn left",
        Direction::Right => "Turn right",
        Direction::Press => "Press",
    };
    let group = adw::PreferencesGroup::builder().title(title).build();
    let slot = ActionSlot::dial(i, dir);
    group.set_header_suffix(Some(&test_button(app, &slot)));
    action_editor(app, &group, "Action", &slot);
    group
}

/// Handles a fetched website icon: (app, target, imported icon path, site name).
type FaviconHandler = Rc<dyn Fn(&Rc<App>, ActionTarget, String, String) + 'static>;

/// One action an editor edits: a key's action, one of a dial's three, and later a toggle's
/// on action, a multi-action step or a long press.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub(crate) struct ActionSlot {
    /// Where the action lives. Late results write through `App::set_action_at`.
    pub target: ActionTarget,
    /// The dial slot it is, for the defaults a kind pick starts from; `None` for a key.
    pub dir: Option<Direction>,
    /// Read the action now.
    pub get: Rc<dyn Fn(&Rc<App>) -> Action>,
    /// Replace the action (the rows' own edits).
    pub set: Rc<dyn Fn(&Rc<App>, Action)>,
    /// The page an "Open page" list leaves out: a key's own page; `None` for dials.
    pub own_page: Option<String>,
    /// Apply an app pick: (app, target, command, name, icon).
    pub on_app_picked: Rc<dyn Fn(&Rc<App>, &ActionTarget, String, String, Option<String>)>,
    /// Make the page a "New page" button points the action at; returns its name.
    pub on_new_page: Rc<dyn Fn(&Rc<App>) -> String>,
    /// Handle a fetched website icon; `None` hides the Fetch row.
    pub on_favicon: Option<FaviconHandler>,
}

impl ActionSlot {
    /// A dial slot: reads and writes through its target and only changes the action.
    pub fn dial(i: u8, dir: Direction) -> Self {
        let target = ActionTarget::dial(i, dir);
        let (t1, t2) = (target.clone(), target.clone());
        ActionSlot {
            target,
            dir: Some(dir),
            get: Rc::new(move |a| a.action_at(&t1).unwrap_or_default()),
            set: Rc::new(move |a, act| {
                a.set_action_at(&t2, act);
            }),
            own_page: None,
            // Only the slot's action changes: the dial's label is shared by its slots.
            on_app_picked: Rc::new(|a, at, command, _, _| {
                a.set_action_at(at, Action::App { command });
            }),
            on_new_page: Rc::new(move |a| {
                let label = a.config.borrow().dial(i).map(|d| d.label.clone()).unwrap_or_default();
                a.add_page(if label.trim().is_empty() { "Page" } else { label.as_str() }, true)
            }),
            on_favicon: None,
        }
    }

    /// The action a kind pick starts from: the slot's direction for a dial (a left
    /// turn lowers, a press mutes), the empty action for a key.
    pub fn default_action(&self, kind: usize) -> Action {
        match self.dir {
            Some(dir) => Action::default_for(kind, dir),
            None => Action::empty_of_kind(kind),
        }
    }
}

/// The ▶ "Try it now" button for a slot. Off for an empty action and for actions only
/// the deck runs (`Action::is_deck_owned`), except Open page, which shows the page here.
fn test_button(app: &Rc<App>, slot: &ActionSlot) -> gtk::Button {
    let act = (slot.get)(app);
    let (tip, sensitive) = match &act {
        Action::Page { page } => ("Try it now".to_string(), app.config.borrow().has_page(page)),
        a if a.is_deck_owned() => (format!("{} only works on the deck", Action::KINDS[a.kind_index()]), false),
        a => ("Try it now".to_string(), !a.is_none()),
    };
    let test = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .sensitive(sensitive)
        .css_classes(["flat"])
        .build();
    let a = app.clone();
    let get = slot.get.clone();
    test.connect_clicked(move |_| {
        let act = get(&a);
        if let Action::Page { page } = act {
            a.show_page(Some(page), true);
        } else if let Err(e) = action::run(&act) {
            a.toast(&format!("{e:#}"));
        }
    });
    test
}

/// Fill `group` with the action combo and the rows for the selected action, editing
/// through `slot`.
fn action_editor(app: &Rc<App>, group: &adw::PreferencesGroup, combo_title: &str, slot: &ActionSlot) {
    let action = (slot.get)(app);
    let kinds = gtk::StringList::new(&Action::KINDS);
    let kind_row = adw::ComboRow::builder().title(combo_title).model(&kinds).build();
    kind_row.set_selected(action.kind_index() as u32);
    match action {
        Action::Page { .. } => kind_row.set_subtitle("Shows another page of keys on the deck"),
        Action::Back => kind_row.set_subtitle("Returns to the page shown before this one, or Home"),
        Action::PageStep { .. } => kind_row.set_subtitle("The next or previous page, for flicks and dial turns"),
        _ => {}
    }
    let a = app.clone();
    kind_row.connect_selected_notify({
        let slot = slot.clone();
        move |row| {
            let sel = row.selected() as usize;
            if sel != (slot.get)(&a).kind_index() {
                (slot.set)(&a, slot.default_action(sel));
                a.rebuild_editor();
            }
        }
    });
    group.add(&kind_row);
    action_rows(app, group, slot);
}

/// A short explanation under an action's rows. Its own row, not the group's description:
/// the group belongs to whoever built it.
fn hint_row(text: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .subtitle(text)
        .subtitle_lines(0)
        .use_markup(false)
        .activatable(false)
        .css_classes(["dim-label"])
        .build()
}

fn action_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot) {
    let action = (slot.get)(app);
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
            let on_app_picked = slot.on_app_picked.clone();
            let at = slot.target.clone();
            choose.connect_clicked(move |_| {
                let on_app_picked = on_app_picked.clone();
                let at = at.clone();
                let a2 = a.clone();
                apps::pick(&a.window, move |info| {
                    let command = info
                        .id()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| info.commandline().map(|c| c.to_string_lossy().into()).unwrap_or_default());
                    let icon = media::import_app_icon(&info);
                    let name = info.display_name().to_string();
                    (on_app_picked)(&a2, &at, command, name, icon);
                    a2.rebuild_editor();
                });
            });
            row.add_suffix(&choose);
            row.set_activatable_widget(Some(&choose));
            group.add(&row);

            let cmd = adw::EntryRow::builder().title("Desktop entry or command").text(command).build();
            let a = app.clone();
            let set_cmd = slot.set.clone();
            cmd.connect_changed(move |e| {
                let command = e.text().to_string();
                (set_cmd)(&a, Action::App { command });
            });
            group.add(&cmd);
        }

        Action::Script { path, terminal } => {
            let row = adw::EntryRow::builder().title("Script").text(path).build();
            let a = app.clone();
            let t = *terminal;
            let set_row = slot.set.clone();
            row.connect_changed(move |e| {
                let path = e.text().to_string();
                (set_row)(&a, Action::Script { path, terminal: t });
            });
            row.add_suffix(&browse_button(app, "Choose a script", None, {
                let at = slot.target.clone();
                move |a, p| {
                    a.set_action_at(&at, Action::Script { path: paths::portable(&p), terminal: t });
                }
            }));
            group.add(&row);

            let term = adw::SwitchRow::builder()
                .title("Run in a terminal")
                .subtitle("Opens your terminal so you can watch the output")
                .active(*terminal)
                .build();
            let a = app.clone();
            let get = slot.get.clone();
            let set_switch = slot.set.clone();
            term.connect_active_notify(move |r| {
                let on = r.is_active();
                // Read the action out of the borrow before editing (a `Ref` held across
                // the edit would panic in `borrow_mut`).
                let mut act = (get)(&a);
                if let Action::Script { terminal, .. } = &mut act {
                    *terminal = on;
                }
                (set_switch)(&a, act);
                a.rebuild_editor();
            });
            group.add(&term);
        }

        Action::Sound { file, volume } => {
            let row = adw::EntryRow::builder().title("Sound file").text(file).build();
            let a = app.clone();
            let v = *volume;
            let set_row = slot.set.clone();
            row.connect_changed(move |e| {
                let file = e.text().to_string();
                (set_row)(&a, Action::Sound { file, volume: v });
            });
            row.add_suffix(&browse_button(app, "Choose a sound", Some(("Audio", "audio/*")), {
                let at = slot.target.clone();
                move |a, p| {
                    a.set_action_at(&at, Action::Sound { file: paths::portable(&p), volume: v });
                }
            }));
            group.add(&row);

            let vol = adw::SpinRow::with_range(0.0, 150.0, 5.0);
            vol.set_title("Volume (%)");
            vol.set_value((volume.unwrap_or(1.0) * 100.0).round() as f64);
            let a = app.clone();
            let get = slot.get.clone();
            let set_vol = slot.set.clone();
            vol.connect_value_notify(move |r| {
                let pct = r.value() as f32 / 100.0;
                let mut act = (get)(&a);
                if let Action::Sound { volume, .. } = &mut act {
                    *volume = if (pct - 1.0).abs() < f32::EPSILON { None } else { Some(pct) };
                }
                (set_vol)(&a, act);
            });
            group.add(&vol);
        }

        Action::Url { url } => {
            let row = adw::EntryRow::builder().title("Website").text(url).input_purpose(gtk::InputPurpose::Url).build();
            let a = app.clone();
            let set_row = slot.set.clone();
            row.connect_changed(move |e| {
                let url = e.text().to_string();
                (set_row)(&a, Action::Url { url });
            });
            group.add(&row);

            if let Some(on_favicon) = slot.on_favicon.clone() {
                let fetch_row = adw::ActionRow::builder()
                    .title("Site icon")
                    .subtitle(
                        "Asks the website for its icon. If it has none, DuckDuckGo's icon service is used instead",
                    )
                    .build();
                let fetch = gtk::Button::builder().label("Fetch").valign(gtk::Align::Center).build();
                let a = app.clone();
                let get = slot.get.clone();
                let at = slot.target.clone();
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
                    let at = at.clone();
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
            group.add(&hint_row(
                "Runs hyprctl dispatch. Classic syntax like workspace 3 · togglefloating · killactive · exec pavucontrol \
                 is translated for you, or write Lua directly: hl.dsp.focus({ workspace = \"3\" })",
            ));
            let row = adw::EntryRow::builder().title("Dispatcher and arguments").text(dispatch).build();
            let a = app.clone();
            let set_row = slot.set.clone();
            row.connect_changed(move |e| {
                let dispatch = e.text().to_string();
                (set_row)(&a, Action::Hyprland { dispatch });
            });
            group.add(&row);
        }

        Action::Page { page } => {
            // A key can't open the page it's on; a dial isn't on a page, so it can open any.
            let own = slot.own_page.clone();
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
            let set_row = slot.set.clone();
            row.connect_selected_notify(move |r| {
                if let Some(Some(name)) = choices.get(r.selected() as usize).cloned() {
                    (set_row)(&a, Action::Page { page: name });
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
            let set_new = slot.set.clone();
            let on_new_page = slot.on_new_page.clone();
            new.connect_clicked(move |_| {
                let name = (on_new_page)(&a);
                (set_new)(&a, Action::Page { page: name });
                a.rebuild_editor();
            });
            buttons.add_suffix(&new);
            buttons.add_suffix(&open);
            buttons.set_activatable_widget(Some(&open));
            group.add(&buttons);
        }

        Action::PageStep { change } => {
            let spin = adw::SpinRow::with_range(-5.0, 5.0, 1.0);
            spin.set_title("Pages to move");
            spin.set_value(*change as f64);
            let a = app.clone();
            let get = slot.get.clone();
            let set_spin = slot.set.clone();
            spin.connect_value_notify(move |r| {
                let change = r.value().round() as i8;
                let mut act = (get)(&a);
                if let Action::PageStep { change: c } = &mut act {
                    *c = change;
                }
                (set_spin)(&a, act);
            });
            group.add(&spin);
        }

        Action::Volume { device, change, mute } => {
            group.add(&hint_row(
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
                let get = slot.get.clone();
                let set_mute = slot.set.clone();
                move |r| {
                    let on = r.is_active();
                    // Read the action out of the borrow before editing (a `Ref` held
                    // across the edit would panic in `borrow_mut`).
                    let mut act = (get)(&a);
                    if let Action::Volume { mute, .. } = &mut act {
                        *mute = on;
                    }
                    (set_mute)(&a, act);
                    a.rebuild_editor();
                }
            });
            group.add(&mute_row);

            // A mute action has a device too (a mic-mute key), so the row shows either
            // way; only the level change is irrelevant while muting.
            let dev_kinds = gtk::StringList::new(&["Output", "Input"]);
            let dev_row = adw::ComboRow::builder().title("Device").model(&dev_kinds).build();
            dev_row.set_selected(if device.is_output() { 0 } else { 1 });
            dev_row.connect_selected_notify({
                let a = app.clone();
                let get = slot.get.clone();
                let set_dev = slot.set.clone();
                move |row| {
                    let sel = row.selected() as usize;
                    let cur = match (get)(&a) {
                        Action::Volume { device, .. } => usize::from(!device.is_output()),
                        _ => 0,
                    };
                    if sel != cur {
                        let device = if sel == 1 { AudioDevice::Input } else { AudioDevice::Output };
                        let mut act = (get)(&a);
                        if let Action::Volume { device: d, .. } = &mut act {
                            *d = device;
                        }
                        (set_dev)(&a, act);
                        a.rebuild_editor();
                    }
                }
            });
            group.add(&dev_row);

            if !*mute {
                let spin = adw::SpinRow::with_range(-25.0, 25.0, 1.0);
                spin.set_title("Change per turn (%)");
                spin.set_value(*change as f64);
                spin.connect_value_notify({
                    let a = app.clone();
                    let get = slot.get.clone();
                    let set_spin = slot.set.clone();
                    move |r| {
                        let change = r.value().round() as i8;
                        let mut act = (get)(&a);
                        if let Action::Volume { change: c, .. } = &mut act {
                            *c = change;
                        }
                        (set_spin)(&a, act);
                    }
                });
                group.add(&spin);
            }
        }

        Action::Brightness { change, set } => {
            group.add(&hint_row(
                "Changes the panel brightness (0–100). The value is remembered in the \
                 state file, so it survives a restart. On a dial it is drawn on the touch \
                 strip.",
            ));

            let mode_of = |a: &Action| match a {
                Action::Brightness { set: Some(..), .. } => 1,
                _ => 0,
            };
            let modes = gtk::StringList::new(&["Change per turn", "Set to a value"]);
            let mode_row = adw::ComboRow::builder().title("Mode").model(&modes).build();
            mode_row.set_selected(mode_of(&action) as u32);
            mode_row.connect_selected_notify({
                let a = app.clone();
                let get = slot.get.clone();
                let set_mode = slot.set.clone();
                let slot = slot.clone();
                move |row| {
                    let sel = row.selected() as usize;
                    if sel != mode_of(&(get)(&a)) {
                        // Back to "change": the slot's default, so Turn left lowers again.
                        let act = match sel {
                            1 => Action::Brightness { change: 0, set: Some(50) },
                            _ => slot.default_action(Action::Brightness { change: 0, set: None }.kind_index()),
                        };
                        (set_mode)(&a, act);
                        a.rebuild_editor();
                    }
                }
            });
            group.add(&mode_row);

            if let Some(v) = set {
                let spin = adw::SpinRow::with_range(0.0, 100.0, 5.0);
                spin.set_title("Brightness (%)");
                spin.set_value(*v as f64);
                spin.connect_value_notify({
                    let a = app.clone();
                    let set_spin = slot.set.clone();
                    move |r| {
                        (set_spin)(&a, Action::Brightness { change: 0, set: Some(r.value().round() as u8) });
                    }
                });
                group.add(&spin);
            } else {
                let spin = adw::SpinRow::with_range(-50.0, 50.0, 1.0);
                spin.set_title("Change per turn (%)");
                spin.set_value(*change as f64);
                spin.connect_value_notify({
                    let a = app.clone();
                    let set_spin = slot.set.clone();
                    move |r| {
                        (set_spin)(&a, Action::Brightness { change: r.value().round() as i8, set: None });
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
    let name = app.add_page(if label.trim().is_empty() { "Page" } else { &label }, true);
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
/// An image picker row (key icons, dial and strip backgrounds). `set` gets the target
/// captured for the async picker, so a pick lands where it started.
pub(crate) fn image_row(
    app: &Rc<App>,
    title: &str,
    hint: &str,
    current: Option<&str>,
    preview_img: Option<omadeck_core::render::RgbaImage>,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    set: impl Fn(&Rc<App>, Target, Option<String>) + Clone + 'static,
) -> adw::ActionRow {
    let subtitle = current
        .map(|p| paths::resolve(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .unwrap_or_else(|| "None".into());
    let row = adw::ActionRow::builder().title(title).subtitle(subtitle).use_markup(false).build();

    let preview = gtk::Image::builder().pixel_size(40).css_classes(["icon-preview"]).build();
    match preview_img {
        Some(img) => preview.set_paintable(Some(&media::texture(img))),
        None => preview.set_icon_name(Some("image-x-generic-symbolic")),
    }
    row.add_prefix(&preview);

    if current.is_some() {
        let clear = gtk::Button::builder()
            .icon_name("edit-clear-symbolic")
            .tooltip_text("Remove image")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        clear.connect_clicked({
            let a = app.clone();
            let target = target.clone();
            let set = set.clone();
            move |_| {
                let at = target(&a);
                set(&a, at, None);
                a.rebuild_editor();
            }
        });
        row.add_suffix(&clear);
    }

    let choose = gtk::Button::builder().label("Choose…").tooltip_text(hint).valign(gtk::Align::Center).build();
    choose.connect_clicked({
        let a = app.clone();
        let target = target.clone();
        let set = set.clone();
        move |_| {
            let a2 = a.clone();
            let set = set.clone();
            let at = target(&a2);
            let dialog = file_dialog("Choose an image", Some(("Images", "image/*")));
            glib::spawn_future_local(async move {
                let Ok(file) = dialog.open_future(Some(&a2.window)).await else { return };
                let Some(path) = file.path() else { return };
                match media::import_icon(&path) {
                    Ok(rel) => {
                        set(&a2, at, Some(rel));
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

/// The key editor's icon row: an `image_row` with the icon's square preview.
fn icon_row(
    app: &Rc<App>,
    icon: Option<&str>,
    target: impl Fn(&Rc<App>) -> Target + Clone + 'static,
    set_icon: impl Fn(&Rc<App>, Target, Option<String>) + Clone + 'static,
) -> adw::ActionRow {
    let preview = icon.and_then(|p| omadeck_core::render::load_icon(&paths::resolve(p), 80));
    image_row(app, "Icon", "", icon, preview, target, set_icon)
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
    extra_subtitle: Option<&str>,
    set: impl Fn(&Rc<App>, Option<String>) + Clone + 'static,
) -> adw::ActionRow {
    let subtitle = extra_subtitle.unwrap_or(if overridden { "Custom" } else { "From theme" });
    let row = adw::ActionRow::builder().title(title).subtitle(subtitle).build();
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

pub(crate) fn file_dialog(title: &str, filter: Option<(&str, &str)>) -> gtk::FileDialog {
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
    title: &'static str,
    filter: Option<(&'static str, &'static str)>,
    on_file: impl Fn(&Rc<App>, std::path::PathBuf) + Clone + 'static,
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
        glib::spawn_future_local(async move {
            if let Ok(file) = dialog.open_future(Some(&a.window)).await
                && let Some(p) = file.path()
            {
                on_file(&a, p);
                a.rebuild_editor();
            }
        });
    });
    btn
}
