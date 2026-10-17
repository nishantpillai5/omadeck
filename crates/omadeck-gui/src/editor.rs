//! The right-hand panel: what a key or dial does and how it looks.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use omadeck_core::{
    Action, AudioDevice, BusyFace, DialConfig, Direction, FaceTab, HOME, KeyConfig, Live, LookField, MediaCommand,
    OnFace, Rgb, StateChoice, action, audio, ipc, paths,
};

use crate::app::{ActionTarget, App, Target};
use crate::{apps, media};

pub fn build_key(app: &Rc<App>, i: u8) -> gtk::Widget {
    let key = app.key(i).unwrap_or_else(|| KeyConfig::new(i));
    // A tab this key doesn't offer (its on face was just removed) falls back to Off.
    let tab = key.clamp_face_tab(app.face_tab.get());
    app.face_tab.set(tab);
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
        kinds: Action::KEY_KINDS,
    };

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let run = Rc::new(move |a: &Rc<App>| a.key(i).map(|k| k.action).unwrap_or_default());
    buttons.append(&test_button(app, &slot, run));
    buttons.append(&presets_button(app, i));
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
    if key.action.is_trackable() {
        let busy = adw::SwitchRow::builder()
            .title("Busy while running")
            .subtitle("Show the busy face until the command finishes")
            .active(key.busy_while_running)
            .build();
        let a = app.clone();
        busy.connect_active_notify(move |r| {
            let on = r.is_active();
            a.edit(i, |k| k.busy_while_running = on);
            // The Busy tab and the Progress bar row come and go with it.
            a.rebuild_editor();
        });
        group.add(&busy);
    }
    page.add(&group);

    // --- toggle ---------------------------------------------------------------------------
    let toggle = adw::PreferencesGroup::builder().title("Toggle").build();
    let two = adw::SwitchRow::builder()
        .title("Two faces (toggle)")
        .subtitle("A second face while on")
        .active(key.on.is_some())
        .build();
    let a = app.clone();
    two.connect_active_notify(move |r| {
        let on = r.is_active();
        a.edit(i, |k| {
            if on {
                k.on = Some(OnFace::default());
            } else {
                k.on = None;
                k.busy = None;
            }
        });
        a.rebuild_editor();
    });
    toggle.add(&two);
    if let Some(on) = &key.on {
        let choices = key.state_choices();
        let names: Vec<&str> = choices
            .iter()
            .map(|c| match c {
                StateChoice::Press => "Flips on each press",
                StateChoice::Command => "Shell command",
                StateChoice::Mute => "Follows mute",
                StateChoice::Player => "Follows the player",
            })
            .collect();
        let list = gtk::StringList::new(&names);
        let state_row = adw::ComboRow::builder().title("State from").model(&list).build();
        // The row is the choice's place in this key's list, not the enum's number.
        state_row.set_selected(choices.iter().position(|c| *c == key.state_choice()).unwrap_or(0) as u32);
        let choices = choices.to_vec();
        let a = app.clone();
        state_row.connect_selected_notify(move |row| {
            let Some(&c) = choices.get(row.selected() as usize) else { return };
            a.edit(i, |k| k.set_state_choice(c));
            a.rebuild_editor();
        });
        toggle.add(&state_row);
        if key.state_choice() == StateChoice::Command {
            let cmd = adw::EntryRow::builder()
                .title("State command")
                .text(on.state.as_deref().unwrap_or(""))
                .show_apply_button(true)
                .build();
            cmd.add_suffix(
                &gtk::Label::builder().label("exit 0 = on, 75 = busy, else off").css_classes(["dim-label"]).build(),
            );
            // Saved on Enter, ✓ or focus-out, not per keystroke: the daemon runs whatever is
            // saved, and a half-typed `systemctl --user stop` would run too. Focus-out keeps an
            // edit the user clicked away from.
            let at = app.key_ref(i); // the key it was built for, even if the page changes first
            let saved = Rc::new(RefCell::new(cmd.text().to_string()));
            let save: Rc<dyn Fn(&adw::EntryRow)> = {
                let a = app.clone();
                Rc::new(move |e: &adw::EntryRow| {
                    let text = e.text().to_string();
                    // Unchanged (Enter, then focus moves on): no second edit, no second save.
                    if *saved.borrow() == text {
                        return;
                    }
                    saved.replace(text.clone());
                    a.edit_ref(&at, |k| {
                        if let Some(on) = &mut k.on {
                            on.state = Some(text);
                        }
                    });
                })
            };
            cmd.connect_apply({
                let save = save.clone();
                move |e| save(e)
            });
            let focus = gtk::EventControllerFocus::new();
            let weak = cmd.downgrade();
            focus.connect_leave(move |_| {
                if let Some(e) = weak.upgrade() {
                    save(&e);
                }
            });
            cmd.add_controller(focus);
            toggle.add(&cmd);
            let adj = gtk::Adjustment::new(on.interval as f64, 1.0, 60.0, 1.0, 1.0, 0.0);
            let every = adw::SpinRow::builder().title("Every (seconds)").adjustment(&adj).build();
            let a = app.clone();
            adj.connect_value_changed(move |adj| {
                let v = adj.value().round() as u16;
                a.edit(i, |k| {
                    if let Some(on) = &mut k.on {
                        on.interval = v;
                    }
                });
            });
            toggle.add(&every);
            let slow = adw::SwitchRow::builder().title("Slow (shows progress)").active(on.slow).build();
            let a = app.clone();
            slow.connect_active_notify(move |r| {
                let active = r.is_active();
                a.edit(i, |k| {
                    if let Some(on) = &mut k.on {
                        on.slow = active;
                    }
                });
                // The Busy tab and the Progress bar row come and go with it.
                a.rebuild_editor();
            });
            toggle.add(&slow);
        }
        let on_slot = ActionSlot {
            target: ActionTarget::KeyOn(app.key_ref(i)),
            dir: None,
            get: Rc::new(move |a| a.key(i).and_then(|k| k.on).map(|f| f.action).unwrap_or_default()),
            set: Rc::new(move |a, act| {
                a.edit(i, |k| {
                    if let Some(on) = &mut k.on {
                        on.action = act;
                    }
                });
            }),
            own_page: app.current_page(),
            on_app_picked: Rc::new(|a, at, command, _, _| {
                if let ActionTarget::KeyOn(k) = at {
                    a.edit_ref(k, |k| {
                        if let Some(on) = &mut k.on {
                            on.action = Action::App { command };
                        }
                    });
                }
            }),
            // A folder for the on action only: the key keeps its own look (`new_folder`
            // would make the key itself look like a folder).
            on_new_page: Rc::new(move |a| {
                let label = a.key(i).map(|k| k.label).unwrap_or_default();
                a.add_page(if label.trim().is_empty() { "Page" } else { label.as_str() }, true)
            }),
            on_favicon: None,
            kinds: Action::KEY_KINDS,
        };
        // ▶ here runs the on action only, whatever state the key is in: no flip.
        toggle.set_header_suffix(Some(&test_button(app, &on_slot, on_slot.get.clone())));
        action_editor(app, &toggle, "When pressed while on", &on_slot);
    }
    // A busy-while-running key has a busy face without being a toggle: the row follows
    // the Busy tab, not the on face.
    if key.face_tabs().contains(&FaceTab::Busy) {
        let bar = adw::SwitchRow::builder()
            .title("Progress bar")
            .subtitle("Draw the first percentage of the busy text")
            .active(key.busy.as_ref().is_some_and(|b| b.bar))
            .build();
        let a = app.clone();
        bar.connect_active_notify(move |r| {
            let on = r.is_active();
            a.edit(i, |k| {
                if on {
                    k.busy.get_or_insert_with(BusyFace::default).bar = true;
                } else if let Some(b) = &mut k.busy {
                    b.bar = false;
                }
            });
        });
        toggle.add(&bar);
    }
    page.add(&toggle);

    // --- live value -----------------------------------------------------------------------
    let live_group = adw::PreferencesGroup::builder().title("Live value").build();
    let at = app.key_ref(i); // the key it was built for, even if the page changes first
    let edit: LiveEdit = Rc::new(move |a: &Rc<App>, f: LiveChange| {
        a.edit_ref(&at, |k: &mut KeyConfig| f(&mut k.live));
    });
    live_rows(app, &live_group, key.live.as_ref(), edit);
    if key.live.is_some() {
        live_group
            .add(&hint_row("The label shows as a small caption; with an icon, the value takes the label's place."));
    }
    page.add(&live_group);

    // --- appearance -----------------------------------------------------------------------
    let look = adw::PreferencesGroup::builder().title("Appearance").build();
    appearance_tabs(app, &look, &key);
    appearance_rows(app, &look, i, tab, &key);
    page.add(&look);

    page.upcast()
}

/// The linked Off | On | Busy buttons in the Appearance header. On shows for toggles
/// (a key with an on face); Busy for slow toggles and busy-while-running keys.
fn appearance_tabs(app: &Rc<App>, group: &adw::PreferencesGroup, key: &KeyConfig) {
    let tabs = key.face_tabs();
    if tabs.len() < 2 {
        return;
    }
    let current = app.face_tab.get();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    buttons.add_css_class("linked");
    let mut first: Option<gtk::ToggleButton> = None;
    for tab in tabs {
        let label = match tab {
            FaceTab::Off => "Off",
            FaceTab::On => "On",
            FaceTab::Busy => "Busy",
        };
        let b = gtk::ToggleButton::builder().label(label).active(tab == current).build();
        // One group: exactly one tab is active, and clicking the active one keeps it.
        match &first {
            Some(f) => b.set_group(Some(f)),
            None => first = Some(b.clone()),
        }
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

    let presets = dial_presets_button(app, i);
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
    // Only a deck with a touch strip has panels to hold.
    if app.deck_info().strip.is_some() {
        page.add(&long_press_group(app, i));
    }

    // --- panel value ----------------------------------------------------------------------
    let panel_group = adw::PreferencesGroup::builder().title("Panel value").build();
    let edit: LiveEdit = Rc::new(move |a: &Rc<App>, f: LiveChange| {
        a.edit_dial(i, |d: &mut DialConfig| f(&mut d.live));
    });
    live_rows(app, &panel_group, dial.live.as_ref(), edit);
    if dial.live.is_some() {
        panel_group.add(&hint_row("Shown instead of the volume or brightness the dial's actions change."));
    }
    page.add(&panel_group);

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
    group.set_header_suffix(Some(&test_button(app, &slot, slot.get.clone())));
    action_editor(app, &group, "Action", &slot);
    group
}

/// The "Long press on the strip" group: what holding the dial's panel runs (Elgato's
/// "long touch"), with a ▶ that runs that action only.
fn long_press_group(app: &Rc<App>, i: u8) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Long press on the strip")
        .description("Press and hold this dial's panel. Nothing set: same as Press.")
        .build();
    let slot = ActionSlot::dial_long_press(i);
    group.set_header_suffix(Some(&test_button(app, &slot, slot.get.clone())));
    action_editor(app, &group, "Action", &slot);
    group
}

/// Handles a fetched website icon: (app, target, imported icon path, site name).
type FaviconHandler = Rc<dyn Fn(&Rc<App>, ActionTarget, String, String) + 'static>;
/// What the Try-it-now button executes.
type ActionRun = Rc<dyn Fn(&Rc<App>) -> Action>;

/// The key group's Presets menu: one click makes the key a working toggle. A preset
/// whose binary is missing is insensitive ("needs <bin>"); Service… asks for the unit.
fn presets_button(app: &Rc<App>, i: u8) -> gtk::MenuButton {
    let a = app.clone();
    let group = gio::SimpleActionGroup::new();
    for (n, p) in omadeck_core::PRESETS.iter().enumerate() {
        let name = format!("preset-{n}");
        let action = gio::SimpleAction::new(name.as_str(), None);
        let a2 = a.clone();
        action.connect_activate(move |_action, _param| {
            let p = &omadeck_core::PRESETS[n];
            a2.edit(i, |k| (p.apply)(k));
            a2.rebuild_editor();
        });
        if let Some(bin) = p.needs
            && !omadeck_core::on_path(bin)
        {
            action.set_enabled(false);
        }
        group.add_action(&action);
    }
    let service = gio::SimpleAction::new("service", None);
    let a3 = a.clone();
    service.connect_activate(move |_action, _param| service_dialog(&a3, i));
    group.add_action(&service);
    let menu = gio::Menu::new();
    for (n, p) in omadeck_core::PRESETS.iter().enumerate() {
        let label = match p.needs.filter(|bin| !omadeck_core::on_path(bin)) {
            Some(bin) => format!("{} (needs {bin})", p.name),
            None => p.name.to_string(),
        };
        let action = format!("presets.preset-{n}");
        let item = gio::MenuItem::new(Some(label.as_str()), Some(action.as_str()));
        menu.append_item(&item);
    }
    let service_item = gio::MenuItem::new(Some("Service…"), Some("presets.service"));
    menu.append_item(&service_item);
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.insert_action_group("presets", Some(&group));
    gtk::MenuButton::builder()
        .icon_name("emblem-ok-symbolic")
        .tooltip_text("Make this key a working toggle")
        .popover(&popover)
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build()
}

/// The dial group's Presets menu: a volume, brightness, chosen-app or focused-app dial
/// in one click. Each preset sets all three slots.
fn dial_presets_button(app: &Rc<App>, i: u8) -> gtk::MenuButton {
    let group = gio::SimpleActionGroup::new();

    let volume = gio::SimpleAction::new("volume", None);
    let a = app.clone();
    volume.connect_activate(move |_action, _param| {
        a.edit_dial(i, |d| {
            d.label = "Volume".into();
            d.glyph = Some("f057e".into());
            d.turn_left = Action::Volume { device: AudioDevice::Output, change: -5, mute: false };
            d.turn_right = Action::Volume { device: AudioDevice::Output, change: 5, mute: false };
            d.press = Action::Volume { device: AudioDevice::Output, change: 0, mute: true };
        });
        a.rebuild_editor();
    });
    group.add_action(&volume);

    let brightness = gio::SimpleAction::new("brightness", None);
    let a = app.clone();
    brightness.connect_activate(move |_action, _param| {
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
    group.add_action(&brightness);

    // Pick the app first; nothing changes if the picker is closed.
    let app_dial = gio::SimpleAction::new("app", None);
    let a = app.clone();
    app_dial.connect_activate(move |_action, _param| {
        let a2 = a.clone();
        apps::pick_audio(&a.window, move |key: String, name: String| {
            a2.edit_dial(i, |d| {
                let act = |change: i8, mute: bool| Action::AppVolume { app: key.clone(), focused: false, change, mute };
                d.label = name.clone();
                d.glyph = None;
                // A live value would hide the app's panel.
                d.live = None;
                d.turn_left = act(-5, false);
                d.turn_right = act(5, false);
                d.press = act(0, true);
            });
            a2.rebuild_editor();
        });
    });
    group.add_action(&app_dial);

    let focused = gio::SimpleAction::new("focused", None);
    let a = app.clone();
    focused.connect_activate(move |_action, _param| {
        a.edit_dial(i, |d| {
            let act = |change: i8, mute: bool| Action::AppVolume { app: String::new(), focused: true, change, mute };
            // The panel names the focused app itself.
            d.label = String::new();
            d.glyph = None;
            d.live = None;
            d.turn_left = act(-5, false);
            d.turn_right = act(5, false);
            d.press = act(0, true);
        });
        a.rebuild_editor();
    });
    group.add_action(&focused);

    // Turn to scrub, press to play or pause; the panel shows the track.
    let media_dial = gio::SimpleAction::new("media", None);
    let a = app.clone();
    media_dial.connect_activate(move |_action, _param| {
        a.edit_dial(i, |d| {
            let act = |seek: i16| Action::Media { command: MediaCommand::PlayPause, seek, player: None };
            d.label = "Media".into();
            // The state glyph and the cover show.
            d.glyph = None;
            // A live value would hide the media panel.
            d.live = None;
            d.turn_left = act(-5);
            d.turn_right = act(5);
            d.press = act(0);
        });
        a.rebuild_editor();
    });
    group.add_action(&media_dial);

    let menu = gio::Menu::new();
    menu.append(Some("Volume dial"), Some("dial-presets.volume"));
    menu.append(Some("Brightness dial"), Some("dial-presets.brightness"));
    menu.append(Some("App volume dial…"), Some("dial-presets.app"));
    menu.append(Some("Focused app dial"), Some("dial-presets.focused"));
    menu.append(Some("Media dial"), Some("dial-presets.media"));
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.insert_action_group("dial-presets", Some(&group));
    gtk::MenuButton::builder()
        .label("Presets")
        .tooltip_text("Make this dial a volume, brightness, app volume or media dial")
        .popover(&popover)
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build()
}

/// The Service… dialog: a systemd user unit and an optional health URL.
fn service_dialog(app: &Rc<App>, i: u8) {
    let unit = gtk::Entry::builder().placeholder_text("llm.service").build();
    let url_entry = gtk::Entry::builder().placeholder_text("http://127.0.0.1:8000/health (optional)").build();
    let entries = gtk::Box::new(gtk::Orientation::Vertical, 6);
    entries.append(&unit);
    entries.append(&url_entry);
    // The key it was opened for: a page change on the deck meanwhile must not move it.
    let at = app.key_ref(i);
    let dialog = adw::AlertDialog::builder()
        .heading("Service toggle")
        .body("A slow toggle for a systemd user unit. Optionally check a health URL while it comes up.")
        .build();
    dialog.set_extra_child(Some(&entries));
    dialog.add_responses(&[("cancel", "Cancel"), ("ok", "Create")]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("ok"));
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    let a = app.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "ok" {
            return;
        }
        let u = unit.text().to_string();
        let url = url_entry.text().trim().to_string();
        let url = (!url.is_empty()).then_some(url.as_str());
        // Checked on a scratch key first: a typo must not cost an edit (and a save)
        // of the real key.
        if let Err(e) = omadeck_core::apply_service(&mut KeyConfig::new(0), &u, url) {
            a.toast(&format!("{e:#}"));
            return;
        }
        // In place: the key keeps its index, background and colours.
        a.edit_ref(&at, |k| {
            let _ = omadeck_core::apply_service(k, &u, url);
        });
        a.rebuild_editor();
    });
    // AdwAlertDialog closes itself after a response: no `unparent`.
    dialog.present(Some(&app.window));
}

/// One action an editor edits: a key's action, one of a dial's three or its long press,
/// a toggle's on action, or a multi-action step. Every action editor takes one, so
/// editors nest: a multi's step editor builds a slot for the step (`ActionSlot::step`)
/// and calls `action_editor` again.
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
    /// The kinds its combo offers (indexes into `Action::KINDS`): `KEY_KINDS` for a key or
    /// dial, `STEP_KINDS` for a multi's step.
    pub kinds: &'static [usize],
}

impl ActionSlot {
    /// A dial slot: reads and writes through its target and only changes the action.
    pub fn dial(i: u8, dir: Direction) -> Self {
        Self::for_dial(i, ActionTarget::dial(i, dir), dir)
    }

    /// Dial `i`'s long press on the strip. Kind picks start from the press defaults.
    pub fn dial_long_press(i: u8) -> Self {
        Self::for_dial(i, ActionTarget::DialLongPress(i), Direction::Press)
    }

    /// A slot of dial `i` at `target`; `dir` sets the defaults a kind pick starts from.
    fn for_dial(i: u8, target: ActionTarget, dir: Direction) -> Self {
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
            kinds: Action::KEY_KINDS,
        }
    }

    /// The slot for step `n` of the multi that `self` edits. Reads and writes go through
    /// its target, like a dial slot's.
    pub fn step(&self, n: usize) -> Self {
        let target = self.step_target(n);
        let (t1, t2) = (target.clone(), target.clone());
        ActionSlot {
            target,
            dir: None,
            get: Rc::new(move |a| a.action_at(&t1).unwrap_or_default()),
            set: Rc::new(move |a, act| {
                a.set_action_at(&t2, act);
            }),
            own_page: self.own_page.clone(),
            // A step's app pick sets only the step: the key's label and icon aren't its.
            on_app_picked: Rc::new(|a, at, command, _, _| {
                a.set_action_at(at, Action::App { command });
            }),
            // A new page for a step doesn't restyle the key as a folder (new_folder would).
            on_new_page: Rc::new(|a| a.add_page("Page", true)),
            on_favicon: None,
            kinds: Action::STEP_KINDS,
        }
    }

    /// The target of step `n` of the multi that `self` edits.
    pub fn step_target(&self, n: usize) -> ActionTarget {
        ActionTarget::Step(Box::new(self.target.clone()), n)
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

/// What ▶ does with an action, given whether the daemon is up. Pure, for the test.
#[derive(Debug, PartialEq)]
enum Route {
    /// Nothing to run.
    Nothing,
    /// Open page: show it here (and on the deck), as before.
    ShowPage(String),
    /// `Command::Press` for the key itself, through the daemon.
    Press { index: u8, page: String },
    /// `Command::Run` through the daemon.
    Daemon,
    /// A deck-owned action with no daemon: say so.
    NeedsDaemon,
    /// Run it here, off the GTK thread.
    Local,
}

/// `key` is the key's index and page (`None` is Home) when ▶ is the key's own, `None`
/// for a nested editor or a dial. First match wins.
fn route(act: &Action, key: Option<(u8, Option<&str>)>, daemon_up: bool) -> Route {
    match (act, key) {
        (Action::None, _) => Route::Nothing,
        (_, Some((index, page))) if daemon_up => Route::Press { index, page: page.unwrap_or(HOME).to_string() },
        (Action::Page { page }, _) => Route::ShowPage(page.clone()),
        (a, _) if a.is_deck_owned() => {
            if daemon_up {
                Route::Daemon
            } else {
                Route::NeedsDaemon
            }
        }
        _ => {
            if daemon_up {
                Route::Daemon
            } else {
                Route::Local
            }
        }
    }
}

/// Toast what went wrong with a request to the daemon, if anything did.
fn toast_reply<E>(a: &App, res: Result<anyhow::Result<Option<ipc::Reply>>, E>) {
    match res {
        Ok(Ok(Some(ipc::Reply::Error { message }))) => a.toast(&message),
        Ok(Ok(Some(_))) => {}
        Ok(Ok(None)) => a.toast("omadeckd isn't running"),
        Ok(Err(e)) => a.toast(&format!("{e:#}")),
        Err(_) => a.toast("Couldn't reach omadeckd"),
    }
}

/// The ▶ "Try it now" button for a slot. `run` is what a click executes. With the daemon
/// up, a key's own ▶ presses the key there (flash, toggle flip) and every other ▶ runs
/// its action there, so deck-owned actions work too; with no daemon it runs here, off
/// the GTK thread. Open page shows the page here (and on the deck). The choice is made
/// on click, so it never goes stale.
fn test_button(app: &Rc<App>, slot: &ActionSlot, run: ActionRun) -> gtk::Button {
    let act = (run)(app);
    let (tip, sensitive) = match &act {
        Action::Page { page } => ("Try it now".to_string(), app.config.borrow().has_page(page)),
        Action::AppVolume { focused: true, .. } => {
            ("Follows the focused window — here that would be the configurator".to_string(), false)
        }
        Action::Delay { .. } if matches!(slot.target, ActionTarget::Step(..)) => {
            ("Nothing to try: a delay only waits".to_string(), false)
        }
        Action::Delay { .. } => ("Delay only works inside a multi action".to_string(), false),
        Action::Multi { steps } if steps.iter().all(|s| matches!(s, Action::Delay { .. })) => {
            ("Add a step first".to_string(), false)
        }
        a if a.is_deck_owned() => ("Try it on the deck".to_string(), true),
        a => ("Try it now".to_string(), !a.is_none()),
    };
    let test = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .sensitive(sensitive)
        .css_classes(["flat"])
        .build();
    // Only a key's own ▶ presses the key; a nested editor's or a dial's runs its action.
    let key: Option<(u8, Option<String>)> = match &slot.target {
        ActionTarget::Key(k) => Some((k.index(), k.page().map(str::to_string))),
        _ => None,
    };
    let app = app.clone();
    test.connect_clicked(move |_| {
        let act = (run)(&app);
        let r = route(&act, key.as_ref().map(|(i, p)| (*i, p.as_deref())), app.daemon_up());
        match r {
            Route::Nothing => {}
            Route::ShowPage(p) => app.show_page(Some(p), true),
            Route::NeedsDaemon => app.toast("This works on the deck: start omadeck's service first"),
            Route::Daemon => {
                let a = app.clone();
                glib::spawn_future_local(async move {
                    let res = gio::spawn_blocking(move || ipc::request(&ipc::Command::Run { action: act })).await;
                    toast_reply(&a, res);
                });
            }
            Route::Press { index, page } => {
                // The daemon presses what it has loaded: write a pending edit first.
                let saved = app.flush_save();
                let a = app.clone();
                glib::spawn_future_local(async move {
                    let res = gio::spawn_blocking(move || -> anyhow::Result<Option<ipc::Reply>> {
                        if saved {
                            // The daemon's own reload waits 150 ms; this press must see the edit.
                            ipc::request(&ipc::Command::Reload)?;
                        }
                        match ipc::request(&ipc::Command::Press { index, page: Some(page) })? {
                            // No deck: run the action on the daemon instead.
                            Some(ipc::Reply::NoDeck) => ipc::request(&ipc::Command::Run { action: act }),
                            r => Ok(r),
                        }
                    })
                    .await;
                    toast_reply(&a, res);
                });
            }
            Route::Local => {
                let a = app.clone();
                glib::spawn_future_local(async move {
                    // A multi runs its steps in order here, delays included, off the GTK
                    // thread; the first failure is toasted with its step number.
                    let res = gio::spawn_blocking(move || -> anyhow::Result<()> {
                        match &act {
                            Action::Multi { steps } => match action::run_multi_blocking(steps).into_iter().next() {
                                Some((n, e)) => Err(anyhow::anyhow!("Step {}: {e:#}", n + 1)),
                                None => Ok(()),
                            },
                            act => action::run(act),
                        }
                    })
                    .await;
                    match res {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => a.toast(&format!("{e:#}")),
                        Err(_) => a.toast("Couldn't run it"),
                    }
                });
            }
        }
    });
    test
}

/// Fill `group` with the action combo and the rows for the selected action, editing
/// through `slot`.
fn action_editor(app: &Rc<App>, group: &adw::PreferencesGroup, combo_title: &str, slot: &ActionSlot) {
    let action = (slot.get)(app);
    // Only the slot's kinds: a key has no Delay, a step no Multi.
    let names: Vec<&str> = slot.kinds.iter().map(|&k| Action::KINDS[k]).collect();
    let kinds = gtk::StringList::new(&names);
    let kind_row = adw::ComboRow::builder().title(combo_title).model(&kinds).build();
    // A kind not offered here (a hand-edited nested multi, a top-level delay) shows no
    // item rather than the wrong one.
    kind_row.set_selected(Action::kind_position(slot.kinds, action.kind_index()).unwrap_or(gtk::INVALID_LIST_POSITION));
    match action {
        Action::Page { .. } => kind_row.set_subtitle("Shows another page of keys on the deck"),
        Action::Back => kind_row.set_subtitle("Returns to the page shown before this one, or Home"),
        Action::PageStep { .. } => kind_row.set_subtitle("The next or previous page, for flicks and dial turns"),
        Action::Multi { .. } => kind_row.set_subtitle("Runs its steps in order, top to bottom"),
        Action::Delay { .. } => kind_row.set_subtitle("Waits before the next step"),
        _ => {}
    }
    let a = app.clone();
    kind_row.connect_selected_notify({
        let slot = slot.clone();
        move |row: &adw::ComboRow| {
            let Some(&kind) = slot.kinds.get(row.selected() as usize) else { return };
            if kind != (slot.get)(&a).kind_index() {
                (slot.set)(&a, slot.default_action(kind));
                a.rebuild_editor();
            }
        }
    });
    group.add(&kind_row);
    action_rows(app, group, slot);
}

/// Changes a key's or a dial's `live` (the one it was built for).
type LiveEdit = Rc<dyn Fn(&Rc<App>, LiveChange)>;

/// One change to a `live`, applied by a [`LiveEdit`].
type LiveChange = Box<dyn FnOnce(&mut Option<Live>)>;

/// A clock's example, on one line.
fn clock_example(format: &str) -> String {
    omadeck_core::live::clock_text(format).replace('\n', " · ")
}

/// The rows of a key's Live value group or a dial's Panel value group: what it shows,
/// and that source's settings. The GUI never reads the value itself: the preview shows
/// what the daemon sends (`–` without it).
fn live_rows(app: &Rc<App>, group: &adw::PreferencesGroup, live: Option<&Live>, edit: LiveEdit) {
    let list = gtk::StringList::new(&Live::KINDS);
    let shows = adw::ComboRow::builder().title("Shows").model(&list).build();
    // Before connecting, so building the row edits nothing.
    shows.set_selected(Live::kind_index(live) as u32);
    let a = app.clone();
    let e = edit.clone();
    shows.connect_selected_notify(move |row: &adw::ComboRow| {
        let sel = row.selected() as usize;
        e(&a, Box::new(move |l: &mut Option<Live>| *l = Live::of_kind(sel)));
        // The source's own rows come and go with it.
        a.rebuild_editor();
    });
    group.add(&shows);

    match live {
        Some(Live::Clock { format }) => {
            let entry = adw::EntryRow::builder().title("Format").text(format.as_str()).build();
            entry.add_suffix(
                &gtk::Label::builder().label("strftime, e.g. %H:%M or %a %d %b").css_classes(["dim-label"]).build(),
            );
            let example = adw::ActionRow::builder()
                .title(clock_example(format))
                .subtitle("Now")
                .use_markup(false)
                .activatable(false)
                .build();
            // Formatting runs nothing, so it is saved per keystroke, like a label. The
            // example follows the entry's text, not the config.
            let a = app.clone();
            let e = edit.clone();
            let ex = example.clone();
            entry.connect_changed(move |en: &adw::EntryRow| {
                let text = en.text().to_string();
                ex.set_title(&clock_example(&text));
                e(
                    &a,
                    Box::new(move |l: &mut Option<Live>| {
                        if let Some(Live::Clock { format }) = l {
                            *format = text;
                        }
                    }),
                );
            });
            group.add(&entry);
            group.add(&example);
        }
        Some(Live::Command { command, interval }) => {
            let cmd = adw::EntryRow::builder().title("Command").text(command.as_str()).show_apply_button(true).build();
            cmd.add_suffix(&gtk::Label::builder().label("its first output line").css_classes(["dim-label"]).build());
            // Saved on Enter, ✓ or focus-out, not per keystroke: the daemon runs whatever
            // is saved (as for a state command).
            let saved = Rc::new(RefCell::new(command.clone()));
            let save: Rc<dyn Fn(&adw::EntryRow)> = {
                let a = app.clone();
                let e = edit.clone();
                Rc::new(move |en: &adw::EntryRow| {
                    let text = en.text().to_string();
                    if *saved.borrow() == text {
                        return;
                    }
                    saved.replace(text.clone());
                    e(
                        &a,
                        Box::new(move |l: &mut Option<Live>| {
                            if let Some(Live::Command { command, .. }) = l {
                                *command = text;
                            }
                        }),
                    );
                })
            };
            cmd.connect_apply({
                let save = save.clone();
                move |en: &adw::EntryRow| save(en)
            });
            let focus = gtk::EventControllerFocus::new();
            let weak = cmd.downgrade();
            focus.connect_leave(move |_| {
                if let Some(en) = weak.upgrade() {
                    save(&en);
                }
            });
            cmd.add_controller(focus);
            group.add(&cmd);
            let adj = gtk::Adjustment::new(*interval as f64, 1.0, 3600.0, 1.0, 10.0, 0.0);
            let every = adw::SpinRow::builder().title("Every (seconds)").adjustment(&adj).build();
            let a = app.clone();
            let e = edit.clone();
            adj.connect_value_changed(move |adj: &gtk::Adjustment| {
                let v = adj.value().round() as u16;
                e(
                    &a,
                    Box::new(move |l: &mut Option<Live>| {
                        if let Some(Live::Command { interval, .. }) = l {
                            *interval = v;
                        }
                    }),
                );
            });
            group.add(&every);
        }
        _ => {}
    }
}

/// A short explanation under an action's rows. Its own row, not the group's
/// description: the group belongs to whoever built it (a key's "Home · Row 1 ·
/// Column 2", the Toggle group, PR 12's step editors).
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

        Action::Media { command, seek, player } => media_rows(app, group, slot, *command, *seek, player.as_deref()),

        Action::Delay { ms } => {
            let spin = adw::SpinRow::with_range(0.0, 600_000.0, 100.0);
            spin.set_title("Wait (ms)");
            spin.set_value(f64::from(*ms));
            spin.set_subtitle(&action.summary());
            spin.connect_value_notify({
                let a = app.clone();
                let set_spin = slot.set.clone();
                // Edits in place: a rebuild would take the focus from the spin.
                move |r: &adw::SpinRow| {
                    let act = Action::Delay { ms: r.value().round().max(0.0) as u32 };
                    r.set_subtitle(&act.summary());
                    (set_spin)(&a, act);
                }
            });
            group.add(&spin);
        }

        Action::Multi { steps } => multi_rows(app, group, slot, steps),

        Action::AppVolume { app: app_name, focused, change, mute } => {
            group
                .add(&hint_row("Changes one app's volume. On a dial, the strip shows the app's icon, name and level."));

            let follow = adw::SwitchRow::builder()
                .title("Follow the focused window")
                .subtitle("The app with the focused window, or the last focused one that played audio")
                .active(*focused)
                .build();
            follow.connect_active_notify({
                let a = app.clone();
                let get = slot.get.clone();
                let set_follow = slot.set.clone();
                move |r: &adw::SwitchRow| {
                    let on = r.is_active();
                    // Read the action out of the borrow before editing (BUGS.md 4).
                    let mut act = (get)(&a);
                    if let Action::AppVolume { focused, .. } = &mut act {
                        *focused = on;
                    }
                    (set_follow)(&a, act);
                    a.rebuild_editor();
                }
            });
            group.add(&follow);

            if !*focused {
                let shown = |name: &str| {
                    if name.trim().is_empty() { "None chosen".to_string() } else { audio::display_app(name) }
                };
                let row = adw::ActionRow::builder()
                    .title("Application")
                    .subtitle(shown(app_name.as_str()))
                    .use_markup(false)
                    .build();
                let choose = gtk::Button::builder().label("Choose…").valign(gtk::Align::Center).build();
                let a = app.clone();
                // The slot's target, captured now: the selection may change while the
                // picker is open.
                let at = slot.target.clone();
                choose.connect_clicked(move |_| {
                    let a2 = a.clone();
                    let at = at.clone();
                    apps::pick_audio(&a.window, move |key: String, name: String| {
                        let mut act = a2.action_at(&at).unwrap_or_default();
                        match &mut act {
                            Action::AppVolume { app, .. } => *app = key.clone(),
                            // Changed to another kind meanwhile: nothing to set.
                            _ => return,
                        }
                        if !a2.set_action_at(&at, act) {
                            return;
                        }
                        // A dial with no label is named after the app; key labels are
                        // left alone.
                        if let Some((i, _)) = at.dial_slot() {
                            let unlabelled = a2.config.borrow().dial(i).is_none_or(|d| d.label.trim().is_empty());
                            if unlabelled {
                                a2.edit_dial(i, |d| d.label = name.clone());
                            }
                        }
                        a2.rebuild_editor();
                    });
                });
                row.add_suffix(&choose);
                row.set_activatable_widget(Some(&choose));
                group.add(&row);

                let entry =
                    adw::EntryRow::builder().title("App (process, name or Flatpak id)").text(app_name.as_str()).build();
                entry.connect_changed({
                    let a = app.clone();
                    let get = slot.get.clone();
                    let set_app = slot.set.clone();
                    let row = row.clone();
                    move |e: &adw::EntryRow| {
                        let text = e.text().to_string();
                        row.set_subtitle(&shown(text.as_str()));
                        let mut act = (get)(&a);
                        if let Action::AppVolume { app, .. } = &mut act {
                            *app = text;
                        }
                        // No `rebuild_editor` here: it would take the focus away.
                        (set_app)(&a, act);
                    }
                });
                group.add(&entry);
            }

            let mute_row = adw::SwitchRow::builder()
                .title("Toggle mute")
                .subtitle("Press toggles the app's mute instead of changing its level")
                .active(*mute)
                .build();
            mute_row.connect_active_notify({
                let a = app.clone();
                let get = slot.get.clone();
                let set_mute = slot.set.clone();
                move |r: &adw::SwitchRow| {
                    let on = r.is_active();
                    let mut act = (get)(&a);
                    if let Action::AppVolume { mute, .. } = &mut act {
                        *mute = on;
                    }
                    (set_mute)(&a, act);
                    a.rebuild_editor();
                }
            });
            group.add(&mute_row);

            if !*mute {
                let spin = adw::SpinRow::with_range(-25.0, 25.0, 1.0);
                spin.set_title("Change per turn (%)");
                spin.set_value(*change as f64);
                spin.connect_value_notify({
                    let a = app.clone();
                    let get = slot.get.clone();
                    let set_spin = slot.set.clone();
                    move |r: &adw::SpinRow| {
                        let change = r.value().round() as i8;
                        let mut act = (get)(&a);
                        if let Action::AppVolume { change: c, .. } = &mut act {
                            *c = change;
                        }
                        (set_spin)(&a, act);
                    }
                });
                group.add(&spin);
            }
        }
    }
}

/// The rows of a media action: what it does (a seek shows its seconds) and which player.
/// Every row reads the action fresh through `slot.get` and writes through `slot.set`.
fn media_rows(
    app: &Rc<App>,
    group: &adw::PreferencesGroup,
    slot: &ActionSlot,
    command: MediaCommand,
    seek: i16,
    player: Option<&str>,
) {
    group.add(&hint_row(
        "Controls the media player. Play/pause, next and previous go through Omarchy's own media commands, so its \
         pop-up shows. Seeking, stop, a chosen player and the strip need playerctl.",
    ));

    // The seek a Seek pick starts from: back on a left turn, forward otherwise.
    let default_seek: i16 = if slot.dir == Some(Direction::Left) { -5 } else { 5 };
    let position_of = |command: MediaCommand, seek: i16| -> u32 {
        if seek != 0 {
            return 4;
        }
        match command {
            MediaCommand::PlayPause => 0,
            MediaCommand::Next => 1,
            MediaCommand::Previous => 2,
            MediaCommand::Stop => 3,
        }
    };
    let commands = gtk::StringList::new(&["Play/Pause", "Next track", "Previous track", "Stop", "Seek"]);
    let cmd_row = adw::ComboRow::builder().title("Command").model(&commands).build();
    cmd_row.set_selected(position_of(command, seek));
    cmd_row.connect_selected_notify({
        let a = app.clone();
        let get = slot.get.clone();
        let set_cmd = slot.set.clone();
        move |row: &adw::ComboRow| {
            let sel = row.selected();
            let Action::Media { command, seek, player } = (get)(&a) else { return };
            if sel == position_of(command, seek) {
                return;
            }
            let (command, seek) = match sel {
                1 => (MediaCommand::Next, 0),
                2 => (MediaCommand::Previous, 0),
                3 => (MediaCommand::Stop, 0),
                4 => (MediaCommand::PlayPause, default_seek),
                _ => (MediaCommand::PlayPause, 0),
            };
            (set_cmd)(&a, Action::Media { command, seek, player });
            a.rebuild_editor();
        }
    });
    group.add(&cmd_row);

    if seek != 0 {
        let spin = adw::SpinRow::with_range(-60.0, 60.0, 1.0);
        spin.set_title("Seconds");
        spin.set_subtitle("Per detent on a dial; negative goes back");
        spin.set_value(f64::from(seek));
        spin.connect_value_notify({
            let a = app.clone();
            let get = slot.get.clone();
            let set_spin = slot.set.clone();
            // Edits in place: a rebuild would take the focus from the spin.
            move |r: &adw::SpinRow| {
                let v = r.value().round() as i16;
                if v == 0 {
                    // 0 is no seek at all: back to the slot's default (this notifies again).
                    r.set_value(f64::from(default_seek));
                    return;
                }
                let mut act = (get)(&a);
                if let Action::Media { seek, .. } = &mut act {
                    *seek = v;
                }
                (set_spin)(&a, act);
            }
        });
        group.add(&spin);
    }

    let player_row = adw::EntryRow::builder().title("Player (e.g. spotify; empty = the one playing)").build();
    player_row.set_text(player.unwrap_or(""));
    player_row.connect_changed({
        let a = app.clone();
        let get = slot.get.clone();
        let set_player = slot.set.clone();
        move |e: &adw::EntryRow| {
            let text = e.text();
            let name = text.trim();
            let mut act = (get)(&a);
            if let Action::Media { player, .. } = &mut act {
                *player = (!name.is_empty()).then(|| name.to_string());
            }
            (set_player)(&a, act);
        }
    });
    group.add(&player_row);
}

/// A multi action's rows: one per step (↑, ↓, remove; activate to edit it), "Add step",
/// and the selected step's own editor below them.
fn multi_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot, steps: &[Action]) {
    // A hand-edited multi inside a multi: never offered, and it doesn't run.
    if matches!(slot.target, ActionTarget::Step(..)) {
        group.add(
            &adw::ActionRow::builder()
                .title("Multi-actions can't be nested")
                .activatable(false)
                .css_classes(["error"])
                .build(),
        );
        return;
    }
    let len = steps.len();
    let selected = selected_step_in(app, slot, len);

    for (n, step) in steps.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(format!("{} · {}", n + 1, step.summary()))
            .use_markup(false)
            .activatable(true)
            .build();
        if matches!(step, Action::Multi { .. }) {
            row.set_subtitle("Multi-actions can't be nested");
            row.add_css_class("error");
        }
        if selected == Some(n) {
            row.add_prefix(&gtk::Image::from_icon_name("document-edit-symbolic"));
        }
        let buttons = [
            ("go-up-symbolic", "Move up", StepEdit::Up(n), n > 0),
            ("go-down-symbolic", "Move down", StepEdit::Down(n), n + 1 < len),
            ("user-trash-symbolic", "Remove this step", StepEdit::Remove(n), true),
        ];
        for (icon, tip, edit, sensitive) in buttons {
            let b = gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .valign(gtk::Align::Center)
                .sensitive(sensitive)
                .css_classes(["flat"])
                .build();
            let a = app.clone();
            let slot = slot.clone();
            b.connect_clicked(move |_| change_steps(&a, &slot, edit, None));
            row.add_suffix(&b);
        }
        let a = app.clone();
        let target = slot.step_target(n);
        row.connect_activated(move |_: &adw::ActionRow| {
            // Read in its own statement: the guard must be gone before `replace`.
            let again = a.selected_step.borrow().as_ref() == Some(&target);
            a.selected_step.replace(if again { None } else { Some(target.clone()) });
            a.rebuild_editor();
        });
        group.add(&row);
    }

    // "Add step": a popover of every step kind but Nothing.
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let pop = gtk::Popover::builder().child(&list).build();
    for &kind in Action::STEP_KINDS.iter().filter(|&&k| k != 0) {
        let b = gtk::Button::builder().label(Action::KINDS[kind]).css_classes(["flat"]).build();
        let a = app.clone();
        let slot = slot.clone();
        // Weak: a strong popover inside its own child's handler would keep both alive.
        let weak = pop.downgrade();
        b.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.popdown();
            }
            change_steps(&a, &slot, StepEdit::Add, Some(Action::empty_of_kind(kind)));
        });
        list.append(&b);
    }
    let add = gtk::MenuButton::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add a step")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .popover(&pop)
        .build();
    let add_row = adw::ActionRow::builder().title("Add step").build();
    add_row.add_suffix(&add);
    add_row.set_activatable_widget(Some(&add));
    group.add(&add_row);
    if steps.iter().any(|s| matches!(s, Action::Delay { .. })) || len > 1 {
        group.add(&hint_row(
            "Steps start in order without waiting for each other: add a delay where the next step needs the \
             previous one done (a window moved, an app open).",
        ));
    }

    // The selected step's editor: a group of its own, below the list.
    if let Some(n) = selected {
        let step = slot.step(n);
        let step_group = adw::PreferencesGroup::builder().title(format!("Step {}", n + 1)).margin_top(12).build();
        // Its ▶ runs this step only.
        step_group.set_header_suffix(Some(&test_button(app, &step, step.get.clone())));
        action_editor(app, &step_group, "Action", &step);
        group.add(&step_group);
    }
}

/// Which of the multi's `len` steps (at `slot`) is selected, if one is.
fn selected_step_in(app: &App, slot: &ActionSlot, len: usize) -> Option<usize> {
    let sel = app.selected_step.borrow();
    (0..len).find(|&n| sel.as_ref() == Some(&slot.step_target(n)))
}

/// Change the multi's step list: an owned copy is edited and written back, so no borrow
/// of the config is held across `set` (BUGS.md 4).
fn edit_steps(app: &Rc<App>, slot: &ActionSlot, f: impl FnOnce(&mut Vec<Action>)) {
    let mut act = (slot.get)(app);
    if let Action::Multi { steps } = &mut act {
        f(steps);
        (slot.set)(app, act);
    }
}

/// Apply one structural edit (`new` is the step `Add` pushes), keep the selection on the
/// same step, and rebuild.
fn change_steps(app: &Rc<App>, slot: &ActionSlot, edit: StepEdit, new: Option<Action>) {
    let len = match (slot.get)(app) {
        Action::Multi { steps } => steps.len(),
        _ => return,
    };
    let sel = selected_step_in(app, slot, len);
    edit_steps(app, slot, |steps| match edit {
        StepEdit::Up(i) if i > 0 && i < steps.len() => steps.swap(i - 1, i),
        StepEdit::Down(i) if i + 1 < steps.len() => steps.swap(i, i + 1),
        StepEdit::Remove(i) if i < steps.len() => {
            steps.remove(i);
        }
        StepEdit::Add => steps.push(new.unwrap_or_default()),
        _ => {}
    });
    // Another multi's selection (a dial's other slot) is left alone.
    if sel.is_some() || matches!(edit, StepEdit::Add) {
        let after = selection_after(sel, edit, len).map(|n| slot.step_target(n));
        app.selected_step.replace(after);
    }
    app.rebuild_editor();
}

/// One structural edit of a step list.
#[derive(Debug, Clone, Copy)]
enum StepEdit {
    Up(usize),
    Down(usize),
    Remove(usize),
    Add,
}

/// Where the selected step is after `edit` on a list of `len` steps (before the edit).
/// A moved step stays selected; removing the selected one clears it; removing one above
/// moves it up; Add selects the new last step. An edit that can't apply (Up on the first
/// step) changes nothing. Pure, for the test.
fn selection_after(sel: Option<usize>, edit: StepEdit, len: usize) -> Option<usize> {
    match edit {
        StepEdit::Add => Some(len),
        StepEdit::Up(i) if i > 0 && i < len => sel.map(|s| match s {
            s if s == i => i - 1,
            s if s == i - 1 => i,
            s => s,
        }),
        StepEdit::Down(i) if i + 1 < len => sel.map(|s| match s {
            s if s == i => i + 1,
            s if s == i + 1 => i,
            s => s,
        }),
        StepEdit::Remove(i) if i < len => match sel {
            Some(s) if s == i => None,
            Some(s) if s > i => Some(s - 1),
            s => s,
        },
        _ => sel,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_it_routes() {
        let url = Action::Url { url: "omarchy.org".into() };
        let media = Action::Page { page: "Media".into() };
        // Nested editors and dials (no key): Run for that action only.
        assert_eq!(route(&Action::None, None, true), Route::Nothing);
        assert_eq!(route(&media, None, false), Route::ShowPage("Media".into()));
        assert_eq!(route(&Action::Back, None, true), Route::Daemon);
        assert_eq!(route(&Action::Back, None, false), Route::NeedsDaemon);
        assert_eq!(route(&url, None, true), Route::Daemon);
        assert_eq!(route(&url, None, false), Route::Local);
        // A key's own ▶: a press of that key while the daemon is up, by page name.
        assert_eq!(route(&url, Some((3, None)), true), Route::Press { index: 3, page: "Home".into() });
        assert_eq!(route(&media, Some((0, Some("Apps"))), true), Route::Press { index: 0, page: "Apps".into() });
        assert_eq!(route(&Action::None, Some((3, None)), true), Route::Nothing);
        // No daemon: as a nested ▶.
        assert_eq!(route(&url, Some((3, None)), false), Route::Local);
        assert_eq!(route(&Action::Back, Some((3, None)), false), Route::NeedsDaemon);
        assert_eq!(route(&media, Some((3, None)), false), Route::ShowPage("Media".into()));
    }

    #[test]
    fn selection_follows_the_step() {
        // Moving the selected step up keeps it selected; moving its neighbour shifts it.
        assert_eq!(selection_after(Some(2), StepEdit::Up(2), 4), Some(1));
        assert_eq!(selection_after(Some(1), StepEdit::Up(2), 4), Some(2));
        assert_eq!(selection_after(Some(1), StepEdit::Down(1), 4), Some(2));
        // Removing it clears the selection; removing one above moves it up.
        assert_eq!(selection_after(Some(2), StepEdit::Remove(2), 4), None);
        assert_eq!(selection_after(Some(2), StepEdit::Remove(0), 4), Some(1));
        assert_eq!(selection_after(Some(0), StepEdit::Remove(3), 4), Some(0));
        // A new step is selected.
        assert_eq!(selection_after(None, StepEdit::Add, 4), Some(4));
        // An edit that can't apply moves nothing.
        assert_eq!(selection_after(Some(0), StepEdit::Up(0), 4), Some(0));
        assert_eq!(selection_after(Some(3), StepEdit::Down(3), 4), Some(3));
    }

    #[test]
    fn multi_routes() {
        let multi = Action::Multi { steps: vec![Action::App { command: "a".into() }, Action::Delay { ms: 5 }] };
        let owned = Action::Multi { steps: vec![Action::Back] };
        assert_eq!(route(&multi, Some((5, None)), true), Route::Press { index: 5, page: "Home".into() });
        assert_eq!(route(&multi, None, true), Route::Daemon);
        assert_eq!(route(&multi, None, false), Route::Local);
        assert_eq!(route(&owned, None, false), Route::NeedsDaemon);
    }
}
