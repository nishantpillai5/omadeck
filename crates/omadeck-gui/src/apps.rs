//! "Choose an application" dialog listing installed desktop apps, and the app volume
//! picker listing what's playing now.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use omadeck_core::audio;

pub fn find(id: &str) -> Option<gio::AppInfo> {
    gio::AppInfo::all().into_iter().find(|a| a.id().is_some_and(|i| i == id))
}

fn installed() -> Vec<gio::AppInfo> {
    let mut apps: Vec<_> = gio::AppInfo::all().into_iter().filter(|a| a.should_show()).collect();
    apps.sort_by_key(|a| a.display_name().to_lowercase());
    apps
}

pub fn pick(parent: &impl IsA<gtk::Widget>, on_pick: impl Fn(gio::AppInfo) + 'static) {
    let apps = Rc::new(installed());
    let haystacks: Rc<Vec<String>> = Rc::new(
        apps.iter()
            .map(|a| {
                format!("{} {} {}", a.display_name(), a.description().unwrap_or_default(), a.id().unwrap_or_default())
                    .to_lowercase()
            })
            .collect(),
    );

    let search = gtk::SearchEntry::builder().placeholder_text("Search applications").hexpand(true).build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(12)
        .valign(gtk::Align::Start)
        .build();

    for a in apps.iter() {
        let row = adw::ActionRow::builder()
            .title(a.display_name().as_str())
            .subtitle(a.description().unwrap_or_default().as_str())
            .subtitle_lines(1)
            .use_markup(false)
            .activatable(true)
            .build();
        let icon = match a.icon() {
            Some(gicon) => gtk::Image::from_gicon(&gicon),
            None => gtk::Image::from_icon_name("application-x-executable"),
        };
        icon.set_pixel_size(32);
        row.add_prefix(&icon);
        list.append(&row);
    }

    let s = search.clone();
    let hay = haystacks.clone();
    list.set_filter_func(move |row| {
        let q = s.text().to_lowercase();
        q.is_empty() || hay.get(row.index() as usize).is_some_and(|h| q.split_whitespace().all(|w| h.contains(w)))
    });
    let l = list.clone();
    search.connect_search_changed(move |_| l.invalidate_filter());

    let header = adw::HeaderBar::new();
    let search_bar = gtk::Box::builder().margin_start(12).margin_end(12).margin_bottom(6).build();
    search_bar.append(&search);
    let scroller =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&list).build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.add_top_bar(&search_bar);
    view.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title("Choose an application")
        .content_width(480)
        .content_height(620)
        .child(&view)
        .build();

    let on_pick = Rc::new(on_pick);
    let d = dialog.clone();
    let a = apps.clone();
    let cb = on_pick.clone();
    list.connect_row_activated(move |_, row| {
        if let Some(app) = a.get(row.index() as usize) {
            d.close();
            cb(app.clone());
        }
    });
    // Enter in the search box picks the first match.
    let d = dialog.clone();
    let l = list.clone();
    search.connect_activate(move |_| {
        let mut i = 0;
        while let Some(row) = l.row_at_index(i) {
            if row.is_child_visible() {
                if let Some(app) = apps.get(i as usize) {
                    d.close();
                    on_pick(app.clone());
                }
                return;
            }
            i += 1;
        }
    });

    dialog.present(Some(parent));
    search.grab_focus();
}

/// The app an installed app is found by in the audio streams: its executable's name,
/// or, for a launcher (Flatpak, a shell), its desktop id without `.desktop`.
fn audio_key(info: &gio::AppInfo) -> String {
    let exe = info.executable();
    let base = exe.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    if base.is_empty() || matches!(base.as_str(), "flatpak" | "env" | "sh" | "bash" | "gtk-launch" | "uwsm-app") {
        let id = info.id().map(|s| s.to_string()).unwrap_or_default();
        return id.strip_suffix(".desktop").unwrap_or(&id).to_string();
    }
    base
}

/// Choose an app for an app volume action. `on_pick` gets the app's key (what the
/// action stores) and its display name. The apps playing now are read once, off the
/// GTK thread, as the dialog opens; "Installed apps…" offers every app instead.
pub fn pick_audio(parent: &impl IsA<gtk::Widget>, on_pick: impl Fn(String, String) + 'static) {
    let on_pick: Rc<dyn Fn(String, String)> = Rc::new(on_pick);
    let parent: gtk::Widget = parent.upcast_ref::<gtk::Widget>().clone();

    let heading = gtk::Label::builder().label("Playing now").xalign(0.0).css_classes(["heading"]).build();
    let playing = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .valign(gtk::Align::Start)
        .build();
    let status = adw::ActionRow::builder().title("Looking for apps playing audio…").activatable(false).build();
    playing.append(&status);

    let others = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .valign(gtk::Align::Start)
        .build();
    let installed = adw::ActionRow::builder()
        .title("Installed apps…")
        .subtitle("Choose any application, playing or not")
        .activatable(true)
        .build();
    installed.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    others.append(&installed);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(12)
        .build();
    content.append(&heading);
    content.append(&playing);
    content.append(&others);
    let scroller =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&content).build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&scroller));
    let dialog =
        adw::Dialog::builder().title("Choose an app").content_width(420).content_height(480).child(&view).build();

    {
        let d = dialog.clone();
        let cb = on_pick.clone();
        let parent = parent.clone();
        installed.connect_activated(move |_| {
            d.close();
            let cb = cb.clone();
            pick(&parent, move |info: gio::AppInfo| {
                cb(audio_key(&info), info.display_name().to_string());
            });
        });
    }

    // One read of the streams, never on the GTK thread and never repeated.
    let d = dialog.clone();
    let list = playing.clone();
    glib::spawn_future_local(async move {
        let res = gio::spawn_blocking(audio::streams).await;
        let streams = match res {
            Ok(Some(s)) => s,
            Ok(None) | Err(_) => {
                status.set_title("Couldn't read what's playing (is pactl installed?)");
                return;
            }
        };
        let mut seen: Vec<String> = Vec::new();
        let mut apps: Vec<(String, String)> = Vec::new();
        for s in &streams {
            let Some(lv) = audio::app_level(&[s]) else { continue };
            if lv.key.is_empty() || seen.contains(&lv.key) {
                continue;
            }
            seen.push(lv.key.clone());
            apps.push((lv.key, lv.name));
        }
        if apps.is_empty() {
            status.set_title("Nothing is playing now");
            return;
        }
        list.remove(&status);
        for (key, name) in apps {
            let row = adw::ActionRow::builder()
                .title(name.as_str())
                .subtitle(key.as_str())
                .use_markup(false)
                .activatable(true)
                .build();
            let icon = match find(&format!("{key}.desktop")).and_then(|a| a.icon()) {
                Some(gicon) => gtk::Image::from_gicon(&gicon),
                None => gtk::Image::from_icon_name("audio-x-generic"),
            };
            icon.set_pixel_size(32);
            row.add_prefix(&icon);
            let d = d.clone();
            let cb = on_pick.clone();
            row.connect_activated(move |_| {
                d.close();
                cb(key.clone(), name.clone());
            });
            list.append(&row);
        }
    });

    dialog.present(Some(&parent));
}
