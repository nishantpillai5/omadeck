//! A page's settings: the apps it follows (Elgato's Smart Profiles, on pages).

use std::cell::RefCell;
use std::process::Command;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use omadeck_core::follow::{self, CONFIGURATOR_CLASS};
use omadeck_core::proc;

use crate::app::App;
use crate::apps;

/// The page's apps as rows in `group`, rebuilt after each add or remove.
#[derive(Clone)]
struct AppRows {
    app: Weak<App>,
    page: Rc<str>,
    group: adw::PreferencesGroup,
    rows: Rc<RefCell<Vec<adw::ActionRow>>>,
    /// Where a failed edit is told: the dialog sits over the main window's toasts. Weak, as
    /// the dialog's own rows hold this.
    dialog: glib::WeakRef<adw::PreferencesDialog>,
}

impl AppRows {
    /// Rebuild the rows from the config. Reads the page's `apps` first and drops the
    /// borrow, so a row's handler can edit the config.
    fn fill(&self) {
        let Some(app) = self.app.upgrade() else { return };
        let apps: Vec<String> = app
            .config
            .borrow()
            .pages
            .iter()
            .find(|p| *p.name == *self.page)
            .map(|p| p.apps.clone())
            .unwrap_or_default();
        let old: Vec<adw::ActionRow> = self.rows.borrow_mut().drain(..).collect();
        for row in &old {
            self.group.remove(row);
        }
        let mut new = Vec::new();
        if apps.is_empty() {
            let row = adw::ActionRow::builder()
                .title("No apps yet")
                .subtitle("This page shows only when you pick it")
                .activatable(false)
                .build();
            self.group.add(&row);
            new.push(row);
        }
        for pattern in apps {
            let row =
                adw::ActionRow::builder().title(pattern.as_str()).subtitle("Window class").use_markup(false).build();
            let remove = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Stop following this app")
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let me = self.clone();
            remove.connect_clicked(move |_| me.remove(&pattern));
            row.add_suffix(&remove);
            self.group.add(&row);
            new.push(row);
        }
        *self.rows.borrow_mut() = new;
    }

    /// Follow `class` too (trimmed); one already in the list (any case) is ignored.
    fn add(&self, class: &str) {
        let class = class.trim().to_string();
        let Some(app) = self.app.upgrade() else { return };
        if class.is_empty() {
            return;
        }
        let edited = app.edit_page_apps(&self.page, |apps: &mut Vec<String>| {
            if !apps.iter().any(|a| a.eq_ignore_ascii_case(&class)) {
                apps.push(class.clone());
            }
        });
        self.after_edit(edited);
    }

    fn remove(&self, pattern: &str) {
        let Some(app) = self.app.upgrade() else { return };
        let edited = app.edit_page_apps(&self.page, |apps: &mut Vec<String>| apps.retain(|a| a != pattern));
        self.after_edit(edited);
    }

    /// Show the new list, or say on the dialog why nothing changed.
    fn after_edit(&self, edited: Result<(), &str>) {
        match edited {
            Ok(()) => self.fill(),
            Err(why) => {
                if let Some(dialog) = self.dialog.upgrade() {
                    dialog.add_toast(adw::Toast::new(why));
                }
            }
        }
    }
}

/// The page settings dialog for page `page`: the apps it follows.
pub fn show(app: &Rc<App>, page: &str) {
    let folder = app.config.borrow().is_folder(page);
    let dialog = adw::PreferencesDialog::builder().title(page).search_enabled(false).content_width(460).build();
    let prefs = adw::PreferencesPage::new();
    let description = if folder {
        "Folders can't follow apps: they open from a key."
    } else {
        "Show this page while one of these apps is focused, and go back to where you were when you leave it."
    };
    let group = adw::PreferencesGroup::builder().title("Follow apps").description(description).build();
    let adds = adw::PreferencesGroup::builder()
        .description("A window class, as hyprctl activewindow shows it; * matches anything.")
        .build();
    prefs.add(&group);
    prefs.add(&adds);
    dialog.add(&prefs);
    if folder {
        group.set_sensitive(false);
        adds.set_sensitive(false);
    }

    let rows = AppRows {
        app: Rc::downgrade(app),
        page: Rc::from(page),
        group: group.clone(),
        rows: Rc::new(RefCell::new(Vec::new())),
        dialog: dialog.downgrade(),
    };
    rows.fill();

    let add_app = adw::ButtonRow::builder().title("Add app…").start_icon_name("list-add-symbolic").build();
    {
        let rows = rows.clone();
        let d = dialog.clone();
        add_app.connect_activated(move |_| {
            let rows = rows.clone();
            apps::pick(&d, move |info: gio::AppInfo| {
                let id = info.id().map(|s| s.to_string()).unwrap_or_default();
                let entry = desktop_entry(&id);
                rows.add(&follow::desktop_class(&id, entry.as_deref()));
            });
        });
    }
    adds.add(&add_app);

    let add_window = adw::ButtonRow::builder().title("Add open window…").start_icon_name("window-new-symbolic").build();
    {
        let rows = rows.clone();
        let d = dialog.clone();
        add_window.connect_activated(move |_| {
            let rows = rows.clone();
            let d = d.clone();
            // One read of the open windows, off the GTK thread.
            glib::spawn_future_local(async move {
                let res = gio::spawn_blocking(|| {
                    proc::output_timeout(Command::new("hyprctl").args(["-j", "clients"]), Duration::from_secs(2))
                })
                .await;
                let json = match res {
                    Ok(Some(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
                    _ => String::new(),
                };
                let windows: Vec<(String, String)> = follow::parse_clients(&json)
                    .into_iter()
                    .filter(|(class, _)| !class.eq_ignore_ascii_case(CONFIGURATOR_CLASS))
                    .collect();
                if windows.is_empty() {
                    d.add_toast(adw::Toast::new("No open windows found"));
                    return;
                }
                pick_window(&d, windows, move |class: String| rows.add(&class));
            });
        });
    }
    adds.add(&add_window);

    let entry = adw::EntryRow::builder().title("Window class").show_apply_button(true).build();
    {
        let rows = rows.clone();
        entry.connect_apply(move |e: &adw::EntryRow| {
            rows.add(&e.text());
            e.set_text("");
        });
    }
    adds.add(&entry);

    dialog.present(Some(&app.window));
}

/// Choose one of the open windows; `on_pick` gets its class.
fn pick_window(parent: &adw::PreferencesDialog, windows: Vec<(String, String)>, on_pick: impl Fn(String) + 'static) {
    let on_pick: Rc<dyn Fn(String)> = Rc::new(on_pick);
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(360)
        .propagate_natural_height(true)
        .child(&list)
        .build();
    let alert =
        adw::AlertDialog::new(Some("Add open window"), Some("The page follows the app this window belongs to."));
    alert.set_extra_child(Some(&scroller));
    alert.add_response("cancel", "Cancel");
    alert.set_close_response("cancel");
    for (class, title) in windows {
        let row = adw::ActionRow::builder()
            .title(class.as_str())
            .subtitle(title.as_str())
            .subtitle_lines(1)
            .use_markup(false)
            .activatable(true)
            .build();
        let a = alert.clone();
        let cb = on_pick.clone();
        row.connect_activated(move |_| {
            a.close();
            cb(class.clone());
        });
        list.append(&row);
    }
    alert.present(Some(parent));
}

/// The desktop entry `id` (e.g. `spotify.desktop`), from the user's data dir first, then
/// the system's, as the desktop finds it.
fn desktop_entry(id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .find_map(|dir| std::fs::read_to_string(dir.join("applications").join(id)).ok())
}
