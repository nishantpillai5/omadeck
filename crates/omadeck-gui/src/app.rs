//! Main window: a life-size deck on the left, the key editor on the right.

use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use omadeck_core::ipc::{self, Command as DaemonCommand, Event};
use omadeck_core::paths::FileStamp;
use omadeck_core::probe::{self, Values};
use omadeck_core::{
    Action, Config, DeckInfo, DialConfig, DialStatus, Direction, HOME, KeyConfig, Keys, PanelStatus, Renderer, device,
    paths, tap_to_dial,
};

use crate::{editor, media};

/// After this long the brightness slider is considered the user's, and a late
/// `Event::Brightness` won't move it back.
const SLIDER_HOLD: Duration = Duration::from_millis(500);

/// Previews are rendered larger than displayed so they stay crisp on HiDPI screens.
const PREVIEW_PX: u32 = 192;

/// The key a new page starts with, so a page opened from a key works like a folder.
fn back_key() -> KeyConfig {
    KeyConfig { label: "Back".into(), glyph: Some("\u{f004d}".into()), action: Action::Back, ..KeyConfig::new(0) }
}

/// A key on a particular page. Work that finishes later (fetching an icon, a picker)
/// holds one of these, so the result lands on the key it was started for even if the
/// window has moved to another page since.
#[derive(Clone)]
pub struct KeyRef {
    page: Option<String>,
    index: u8,
}

impl KeyRef {
    /// The page the key is on (`None` is Home).
    pub fn page(&self) -> Option<&str> {
        self.page.as_deref()
    }
}

/// Which action an editor edits, for work that finishes later (a picker, a file
/// dialog, a fetch): the result lands on this action even if the window has moved on.
/// PR 8 adds `KeyOn(KeyRef)`, PR 12 `Step(Box<ActionTarget>, usize)`, PR 16
/// `DialLongPress(u8)`.
#[derive(Clone)]
pub enum ActionTarget {
    /// A key's action.
    Key(KeyRef),
    DialTurnLeft(u8),
    DialTurnRight(u8),
    DialPress(u8),
}

impl ActionTarget {
    /// The target for dial `i`'s slot `dir`.
    pub fn dial(i: u8, dir: Direction) -> Self {
        match dir {
            Direction::Left => ActionTarget::DialTurnLeft(i),
            Direction::Right => ActionTarget::DialTurnRight(i),
            Direction::Press => ActionTarget::DialPress(i),
        }
    }

    /// The dial and slot, for a dial's action.
    pub fn dial_slot(&self) -> Option<(u8, Direction)> {
        match self {
            ActionTarget::Key(_) => None,
            ActionTarget::DialTurnLeft(i) => Some((*i, Direction::Left)),
            ActionTarget::DialTurnRight(i) => Some((*i, Direction::Right)),
            ActionTarget::DialPress(i) => Some((*i, Direction::Press)),
        }
    }
}

/// What an editor row is editing: a key on a page, a dial, or the touch strip. Work
/// that finishes later (a picker, an icon fetch) holds one of these, so the result lands
/// where it started even if the window has moved on since. Dials and the strip have no
/// page, so they're always safe.
#[derive(Clone)]
pub enum Target {
    Key(KeyRef),
    Dial(u8),
    Strip,
}

enum Msg {
    DaemonUp(ipc::Sender),
    DaemonDown,
    Event(Event),
}

/// What the editor is showing.
#[derive(Clone, Copy, PartialEq)]
pub enum Selection {
    Key(u8),
    Dial(u8),
}

pub struct App {
    pub window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    title: adw::WindowTitle,
    banner: adw::Banner,
    config_banner: adw::Banner,
    body: gtk::Box,
    settings_btn: gtk::MenuButton,
    grid: gtk::Grid,
    strip: gtk::Picture,
    dials_grid: gtk::Grid,
    model_label: gtk::Label,
    page_list: gtk::StringList,
    page_dropdown: gtk::DropDown,
    page_actions: gtk::Box,
    /// The page actions' "Folder" toggle: on, flicks and Next/previous page skip the page.
    folder_toggle: gtk::ToggleButton,
    /// Set while the page dropdown is updated from code, so it doesn't count as a choice.
    syncing_pages: Cell<bool>,
    /// The page being edited: `None` is home.
    page: RefCell<Option<String>>,
    daemon: RefCell<Option<ipc::Sender>>,
    editor_slot: adw::Bin,
    keys: RefCell<Vec<(gtk::Button, gtk::Image)>>,
    dials: RefCell<Vec<(gtk::Button, gtk::Label)>>,
    pub config: RefCell<Config>,
    pub renderer: RefCell<Renderer>,
    deck: RefCell<DeckInfo>,
    connected: Cell<bool>,
    daemon_up: Cell<bool>,
    selected: Cell<Option<Selection>>,
    save_source: RefCell<Option<glib::SourceId>>,
    theme_css: gtk::CssProvider,
    theme_monitor: RefCell<Option<gio::FileMonitor>>,
    /// config.toml as this window last read or wrote it (`None`: no file). Anything else
    /// on disk was written by someone else, and is never overwritten.
    disk: RefCell<Option<String>>,
    stamp: Cell<Option<FileStamp>>,
    /// Why config.toml couldn't be parsed. Saving is off until it's fixed.
    locked: RefCell<Option<String>>,
    /// What the daemon last said each strip panel shows (`Event::DialStatus`). Empty with
    /// no daemon or no deck; the preview then builds panels itself.
    panel_status: RefCell<std::collections::HashMap<u8, PanelStatus>>,
    /// The daemon's live brightness (`Event::Brightness`); `None` with no daemon.
    live_brightness: Cell<Option<u8>>,
    /// The settings slider, so `Event::Brightness` can move it.
    brightness_scale: RefCell<Option<gtk::Scale>>,
    /// Set while the slider is moved from code, so it doesn't send the value back.
    syncing_brightness: Cell<bool>,
    /// When the user last moved the slider: a late echo mustn't pull it back mid-drag.
    slider_moved: Cell<Option<std::time::Instant>>,
}

impl App {
    pub fn build(application: &adw::Application) -> Rc<Self> {
        let file = paths::config_file();
        let stamp = FileStamp::of(&file);
        let (config, mut disk, locked) = match std::fs::read_to_string(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (crate::starter::layout(), None, None),
            // An unreadable or broken file is left exactly as it is; the window opens read-only.
            Err(e) => (Config::default(), None, Some(format!("can't read it: {e}"))),
            Ok(src) => match Config::parse(&src) {
                Ok(c) => (c, Some(src), None),
                Err(e) => (Config::default(), Some(src), Some(format!("{e:#}"))),
            },
        };
        let first_run = stamp.is_none() && locked.is_none();
        if first_run {
            match config.save() {
                Ok(body) => disk = Some(body),
                Err(e) => log::warn!("saving starter layout: {e:#}"),
            }
        }
        let renderer = Renderer::new(&config.style);
        let deck = device::detect().into_iter().next().unwrap_or_else(DeckInfo::fallback);

        let display = gdk::Display::default().expect("a display");
        let css = gtk::CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        let theme_css = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(&display, &theme_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        // --- header ----------------------------------------------------------------------
        let title = adw::WindowTitle::new("omadeck", "");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let settings_btn =
            gtk::MenuButton::builder().icon_name("open-menu-symbolic").tooltip_text("Deck settings").build();
        header.pack_end(&settings_btn);

        let banner = adw::Banner::builder()
            .title("The omadeck service isn't running, so your deck won't respond")
            .button_label("Start service")
            .revealed(false)
            .build();
        let config_banner = adw::Banner::builder().button_label("Open folder").revealed(false).build();

        // --- deck ------------------------------------------------------------------------
        let grid = gtk::Grid::builder().row_spacing(14).column_spacing(14).build();
        // The touch strip (Stream Deck +), between the keys and the dials, as on the hardware.
        let strip = gtk::Picture::builder().css_classes(["deck-strip"]).visible(false).build();
        let dials_grid = gtk::Grid::builder().row_spacing(14).column_spacing(14).build();
        let model_label = gtk::Label::builder().halign(gtk::Align::End).css_classes(["deck-model"]).build();
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 12);
        frame.add_css_class("deck-frame");
        frame.append(&grid);
        frame.append(&strip);
        frame.append(&dials_grid);
        frame.append(&model_label);
        let page_list = gtk::StringList::new(&[HOME]);
        let page_dropdown = gtk::DropDown::builder().model(&page_list).tooltip_text("The page you're editing").build();
        let new_page = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("New page").build();
        let rename_page =
            gtk::Button::builder().icon_name("document-edit-symbolic").tooltip_text("Rename this page").build();
        let delete_page =
            gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete this page").build();
        let folder_toggle = gtk::ToggleButton::builder()
            .icon_name("folder-symbolic")
            .tooltip_text("Folder: flicks and Next/previous page skip it")
            .build();
        let page_actions = gtk::Box::builder().css_classes(["linked"]).visible(false).build();
        page_actions.append(&folder_toggle);
        page_actions.append(&rename_page);
        page_actions.append(&delete_page);
        let page_bar = gtk::Box::builder().spacing(6).halign(gtk::Align::Center).build();
        page_bar.append(&page_dropdown);
        page_bar.append(&new_page);
        page_bar.append(&page_actions);

        let hint = gtk::Label::builder()
            .label("Click a key to edit it · drag keys to rearrange · press a key on your deck to jump to it")
            .css_classes(["deck-hint", "caption"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        let deck_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .hexpand(true)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        deck_area.append(&page_bar);
        deck_area.append(&frame);
        deck_area.append(&hint);

        let editor_slot = adw::Bin::new();
        let editor_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(430)
            .child(&editor_slot)
            .build();

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let separator = gtk::Separator::new(gtk::Orientation::Vertical);
        body.append(&deck_area);
        body.append(&separator);
        body.append(&editor_scroller);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&body));
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.add_top_bar(&banner);
        view.add_top_bar(&config_banner);
        view.set_content(Some(&toasts));

        let window = adw::ApplicationWindow::builder()
            .application(application)
            .title("omadeck")
            .default_width(1180)
            .default_height(700)
            .content(&view)
            .width_request(380)
            .height_request(560)
            .build();

        // Omarchy tiles windows, so a half-screen window is common: stack the deck above
        // the editor when there isn't room for both side by side.
        let narrow =
            adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 1050sp").expect("valid condition"));
        narrow.add_setter(&body, "orientation", Some(&gtk::Orientation::Vertical.to_value()));
        narrow.add_setter(&separator, "orientation", Some(&gtk::Orientation::Horizontal.to_value()));
        narrow.add_setter(&editor_scroller, "width-request", Some(&(-1i32).to_value()));
        narrow.add_setter(&editor_scroller, "vexpand", Some(&true.to_value()));
        narrow.add_setter(&deck_area, "margin-top", Some(&12i32.to_value()));
        narrow.add_setter(&deck_area, "margin-bottom", Some(&12i32.to_value()));
        window.add_breakpoint(narrow);

        let app = Rc::new(Self {
            window,
            toasts,
            title,
            banner,
            config_banner,
            body,
            settings_btn,
            grid,
            strip,
            dials_grid,
            model_label,
            page_list,
            page_dropdown,
            page_actions,
            folder_toggle,
            syncing_pages: Cell::new(false),
            page: RefCell::default(),
            daemon: RefCell::default(),
            editor_slot,
            keys: RefCell::default(),
            dials: RefCell::default(),
            config: RefCell::new(config),
            renderer: RefCell::new(renderer),
            deck: RefCell::new(deck),
            panel_status: RefCell::default(),
            live_brightness: Cell::new(None),
            brightness_scale: RefCell::default(),
            syncing_brightness: Cell::new(false),
            slider_moved: Cell::new(None),
            connected: Cell::new(false),
            daemon_up: Cell::new(false),
            selected: Cell::new(None),
            save_source: RefCell::default(),
            theme_css,
            theme_monitor: RefCell::default(),
            disk: RefCell::new(disk),
            stamp: Cell::new(FileStamp::of(&file)),
            locked: RefCell::new(locked),
        });

        let weak = Rc::downgrade(&app);
        app.page_dropdown.connect_selected_notify(move |dd| {
            let Some(app) = weak.upgrade() else { return };
            if app.syncing_pages.get() {
                return;
            }
            let page = match dd.selected() {
                0 | gtk::INVALID_LIST_POSITION => None,
                n => app.config.borrow().pages.get(n as usize - 1).map(|p| p.name.clone()),
            };
            app.show_page(page, true);
        });
        let weak = Rc::downgrade(&app);
        app.folder_toggle.connect_toggled(move |b| {
            let Some(app) = weak.upgrade() else { return };
            if app.syncing_pages.get() {
                return;
            }
            let Some(page) = app.current_page() else { return };
            app.config.borrow_mut().set_folder(&page, b.is_active());
            app.schedule_save();
        });
        let weak = Rc::downgrade(&app);
        new_page.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.ask_new_page();
            }
        });
        let weak = Rc::downgrade(&app);
        rename_page.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.ask_rename_page();
            }
        });
        let weak = Rc::downgrade(&app);
        delete_page.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.ask_delete_page();
            }
        });
        // Clicking a strip panel selects its dial, like clicking a dial knob.
        let strip_click = gtk::GestureClick::new();
        let weak = Rc::downgrade(&app);
        strip_click.connect_pressed(move |_, _, x, _| {
            let Some(app) = weak.upgrade() else { return };
            let d = app.deck.borrow();
            let width = app.strip_width() as u32;
            if let Some(i) = tap_to_dial(x as u32, width, d.dials as usize) {
                app.select(Some(Selection::Dial(i as u8)));
            }
        });
        app.strip.add_controller(strip_click);
        // Right-click opens the strip's background menu, as the Elgato app does.
        let strip_menu = gtk::GestureClick::new();
        strip_menu.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(&app);
        strip_menu.connect_pressed(move |_, _, _, _| {
            if let Some(app) = weak.upgrade() {
                app.strip_menu();
            }
        });
        app.strip.add_controller(strip_menu);
        let weak = Rc::downgrade(&app);
        app.banner.connect_button_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.start_daemon();
            }
        });
        app.config_banner.connect_button_clicked(|_| {
            let uri = gio::File::for_path(paths::config_dir()).uri();
            let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
        });
        let weak = Rc::downgrade(&app);
        app.window.connect_close_request(move |_| {
            if let Some(app) = weak.upgrade() {
                app.flush_save();
            }
            glib::Propagation::Proceed
        });

        // The window owns the app state; callbacks only hold weak references.
        let keep = RefCell::new(Some(app.clone()));
        app.window.connect_destroy(move |_| drop(keep.take()));

        app.apply_theme_css();
        app.watch_theme();
        app.sync_page_list();
        app.rebuild_grid();
        // OMADECK_SELECT=<n> opens with a key selected (handy for screenshots and scripting).
        app.select(std::env::var("OMADECK_SELECT").ok().and_then(|v| v.parse().ok()).map(Selection::Key));
        app.update_title();
        app.listen();
        app.watch_config();
        app.show_lock();
        app.window.present();
        app
    }

    // --- deck grid ------------------------------------------------------------------------

    fn key_display_px(&self) -> i32 {
        let d = self.deck.borrow();
        match d.cols {
            0..=5 => 96,
            6..=8 => 80,
            _ => 64,
        }
    }

    /// The width the key grid (and, under it, the strip) is drawn at.
    fn strip_width(&self) -> i32 {
        let d = self.deck.borrow();
        let px = self.key_display_px();
        d.cols as i32 * px + d.cols.saturating_sub(1) as i32 * 14
    }

    fn rebuild_grid(self: &Rc<Self>) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        while let Some(child) = self.dials_grid.first_child() {
            self.dials_grid.remove(&child);
        }
        self.keys.borrow_mut().clear();
        let deck = self.deck.borrow().clone();
        let px = self.key_display_px();
        for i in 0..deck.key_count() {
            let image = gtk::Image::builder().pixel_size(px).build();
            let button =
                gtk::Button::builder().child(&image).css_classes(["deck-key"]).overflow(gtk::Overflow::Hidden).build();
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.select(Some(Selection::Key(i)));
                }
            });
            self.add_dnd(&button, &image, i);
            self.grid.attach(&button, (i % deck.cols) as i32, (i / deck.cols) as i32, 1, 1);
            self.keys.borrow_mut().push((button, image));
        }
        // Dials (Stream Deck +): round buttons under the strip, as on the hardware.
        self.dials.borrow_mut().clear();
        for i in 0..deck.dials {
            let label = gtk::Label::builder()
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .width_request(px - 24)
                .css_classes(["deck-dial-label"])
                .build();
            let button = gtk::Button::builder().child(&label).css_classes(["deck-dial"]).build();
            button.set_size_request(px, px);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.select(Some(Selection::Dial(i)));
                }
            });
            self.dials_grid.attach(&button, (i % deck.cols) as i32, (i / deck.cols) as i32, 1, 1);
            self.dials.borrow_mut().push((button, label));
        }
        // The strip picture, scaled to the grid's width like the hardware's 800×100.
        if let Some((w, h)) = deck.strip {
            let width = self.strip_width();
            self.strip.set_size_request(width, width * h as i32 / w as i32);
        }
        self.strip.set_visible(deck.strip.is_some());
        // The settings popover has a strip-background row only on a deck with a strip,
        // so rebuild it when a different deck (say, a Plus plugged in late) lays out.
        self.settings_btn.set_popover(Some(&self.settings_popover()));
        self.model_label.set_label(&deck.model.to_uppercase());
        if let Some(Selection::Key(i)) = self.selected.get()
            && i >= deck.key_count()
        {
            self.select(None);
        }
        self.refresh_all();
    }

    fn add_dnd(self: &Rc<Self>, button: &gtk::Button, image: &gtk::Image, i: u8) {
        let source = gtk::DragSource::new();
        source.set_actions(gdk::DragAction::MOVE);
        let img = image.clone();
        source.connect_prepare(move |src, _, _| {
            if let Some(p) = img.paintable() {
                src.set_icon(Some(&p), 48, 48);
            }
            Some(gdk::ContentProvider::for_value(&(i as u32).to_value()))
        });
        button.add_controller(source);

        let target = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
        let b = button.clone();
        target.connect_enter(move |_, _, _| {
            b.add_css_class("drop-target");
            gdk::DragAction::MOVE
        });
        let b = button.clone();
        target.connect_leave(move |_| b.remove_css_class("drop-target"));
        let weak = Rc::downgrade(self);
        let b = button.clone();
        target.connect_drop(move |_, value, _, _| {
            b.remove_css_class("drop-target");
            let (Some(app), Ok(from)) = (weak.upgrade(), value.get::<u32>()) else { return false };
            if from as u8 != i {
                app.with_page(|keys| keys.swap(from as u8, i));
                app.refresh_key(from as u8);
                app.refresh_key(i);
                app.select(Some(Selection::Key(i)));
                app.schedule_save();
            }
            true
        });
        button.add_controller(target);
    }

    pub fn refresh_key(&self, i: u8) {
        let keys = self.keys.borrow();
        let Some((button, image)) = keys.get(i as usize) else { return };
        let cfg = self.config.borrow();
        let key = cfg.key(self.page.borrow().as_deref(), i);
        let face = self.renderer.borrow_mut().render(key, &cfg.style, PREVIEW_PX, false);
        image.set_paintable(Some(&media::texture(face)));
        let tip = match key.filter(|k| !k.is_blank()) {
            Some(k) if !k.label.is_empty() => format!("{} — {}", k.label, k.action.summary()),
            Some(k) => k.action.summary(),
            None => format!("Key {} — empty", i + 1),
        };
        button.set_tooltip_text(Some(&tip));
    }

    pub fn refresh_all(&self) {
        let n = self.keys.borrow().len() as u8;
        for i in 0..n {
            self.refresh_key(i);
        }
        let n = self.dials.borrow().len() as u8;
        for i in 0..n {
            self.refresh_dial(i);
        }
        self.refresh_strip();
    }

    /// Redraw the strip picture: one panel per dial, the same picture the daemon paints.
    /// Volume dials show their device's current level, read once per refresh.
    /// The settings-popover row for the strip's background image. The popover is built
    /// once, so the row updates its own subtitle and thumbnail after a change.
    fn strip_background_row(self: &Rc<Self>) -> adw::ActionRow {
        let current = self.config.borrow().strip.background_image.clone();
        let subtitle_of = |cur: &Option<String>| -> String {
            cur.as_deref()
                .and_then(|p| paths::resolve(p).file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "None".into())
        };
        let row = adw::ActionRow::builder()
            .title("Touch strip background")
            .subtitle(subtitle_of(&current))
            .use_markup(false)
            .build();
        let preview = gtk::Image::builder().pixel_size(40).css_classes(["icon-preview"]).build();
        let show_preview = |preview: &gtk::Image, cur: &Option<String>| {
            if let Some(img) = cur.as_deref().and_then(|p| omadeck_core::render::load_cover(&paths::resolve(p), 80, 40))
            {
                preview.set_paintable(Some(&media::texture(img)));
            } else {
                let none: Option<&gdk::Texture> = None;
                preview.set_paintable(none);
                preview.set_icon_name(Some("image-x-generic-symbolic"));
            }
        };
        show_preview(&preview, &current);
        row.add_prefix(&preview);

        let refresh = {
            let row = row.clone();
            let preview = preview.clone();
            move |app: &Rc<Self>| {
                let cur = app.config.borrow().strip.background_image.clone();
                row.set_subtitle(&subtitle_of(&cur));
                show_preview(&preview, &cur);
            }
        };

        let clear = gtk::Button::builder()
            .icon_name("edit-clear-symbolic")
            .tooltip_text("Remove strip background")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        clear.connect_clicked({
            let a = self.clone();
            let refresh = refresh.clone();
            move |_| {
                a.set_strip_background(None);
                refresh(&a);
            }
        });
        row.add_suffix(&clear);

        let choose = gtk::Button::builder()
            .label("Choose…")
            .tooltip_text("800 × 100; fills the strip")
            .valign(gtk::Align::Center)
            .build();
        choose.connect_clicked({
            let a = self.clone();
            move |_| {
                let a2 = a.clone();
                let refresh = refresh.clone();
                let dialog = editor::file_dialog("Choose a strip background", Some(("Images", "image/*")));
                glib::spawn_future_local(async move {
                    let Ok(file) = dialog.open_future(Some(&a2.window)).await else { return };
                    let Some(path) = file.path() else { return };
                    match media::import_icon(&path) {
                        Ok(rel) => {
                            a2.set_strip_background(Some(rel));
                            refresh(&a2);
                        }
                        Err(e) => a2.toast(&format!("{e:#}")),
                    }
                });
            }
        });
        row.add_suffix(&choose);
        row
    }

    /// Set (or clear) the strip's background image, then repaint and save.
    pub fn set_strip_background(self: &Rc<Self>, image: Option<String>) {
        self.config.borrow_mut().strip.background_image = image;
        self.refresh_strip();
        self.schedule_save();
    }

    /// Elgato's right-click on the touch strip: a small popover that sets or removes the
    /// strip's background image.
    pub fn strip_menu(self: &Rc<Self>) {
        let popover = gtk::Popover::new();
        let list = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        let current = self.config.borrow().strip.background_image.clone();
        let row = editor::image_row(
            self,
            "Strip background",
            "800 × 100; fills the strip",
            current.as_deref(),
            current.as_deref().and_then(|p| omadeck_core::render::load_cover(&paths::resolve(p), 80, 40)),
            |_| Target::Strip,
            |a, t, v| {
                if matches!(t, Target::Strip) {
                    a.set_strip_background(v);
                }
            },
        );
        list.append(&row);
        popover.set_child(Some(&list));
        popover.set_parent(&self.strip);
        // A new popover per right-click; unparent it once closed, or each one stays.
        popover.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        popover.set_position(gtk::PositionType::Bottom);
        popover.popup();
    }

    /// Redraw the strip picture: one panel per dial, the same picture the daemon paints.
    pub fn refresh_strip(&self) {
        let Some((w, h)) = self.deck.borrow().strip else { return };
        let width = self.strip_width();
        if width <= 0 {
            return;
        }
        let cfg = self.config.borrow();
        let n = self.deck.borrow().dials;
        // Panels the daemon described show what the deck shows. The rest (no daemon, or
        // it hasn't said yet) are built here: brightness from the daemon's value or the
        // config's, volume as a dash. Only the daemon reads the system.
        let values = Values { brightness: self.live_brightness.get().unwrap_or(cfg.brightness), ..Values::default() };
        let mirrored = self.panel_status.borrow();
        let status: Vec<DialStatus> = (0..n)
            .map(|i| match mirrored.get(&i) {
                Some(panel) => DialStatus { pressed: false, panel: panel.clone() },
                None => probe::dial_status(cfg.dial(i), &values, false),
            })
            .collect();
        let img = self.renderer.borrow_mut().render_strip(&cfg, &status, (width as u32, width as u32 * h / w));
        self.strip.set_paintable(Some(&media::texture(img)));
    }

    /// Move the brightness slider to `value` without sending it back, unless the user
    /// moved it a moment ago (then this is the echo of an older value).
    fn sync_slider(&self, value: u8) {
        if self.slider_moved.get().is_some_and(|t| t.elapsed() < SLIDER_HOLD) {
            return;
        }
        // Cloned out, so no borrow is held while GTK runs the handler.
        let scale = self.brightness_scale.borrow().clone();
        if let Some(scale) = scale {
            self.syncing_brightness.set(true);
            scale.set_value(value as f64);
            self.syncing_brightness.set(false);
        }
    }

    pub fn refresh_dial(&self, i: u8) {
        let dials = self.dials.borrow();
        let Some((button, label)) = dials.get(i as usize) else { return };
        let cfg = self.config.borrow();
        let name =
            cfg.dial(i).map(|d| d.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| format!("Dial {}", i + 1));
        label.set_text(&name);
        let tip = match cfg.dial(i).filter(|d| !d.is_blank()) {
            Some(d) => format!("{} — press: {}", name, d.press.summary()),
            None => format!("Dial {} — empty", i + 1),
        };
        button.set_tooltip_text(Some(&tip));
    }

    pub fn select(self: &Rc<Self>, sel: Option<Selection>) {
        self.selected.set(sel);
        for (n, (button, _)) in self.keys.borrow().iter().enumerate() {
            if Some(Selection::Key(n as u8)) == sel {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
        }
        for (n, (button, _)) in self.dials.borrow().iter().enumerate() {
            if Some(Selection::Dial(n as u8)) == sel {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
        }
        let widget: gtk::Widget = match sel {
            Some(Selection::Key(i)) => editor::build_key(self, i),
            Some(Selection::Dial(i)) => editor::build_dial(self, i),
            None => adw::StatusPage::builder()
                .icon_name("input-keyboard-symbolic")
                .title("Pick a key")
                .description("Click a key on the deck, or press one on your Stream Deck.")
                .vexpand(true)
                .build()
                .upcast(),
        };
        self.editor_slot.set_child(Some(&widget));
    }

    /// Rebuild the editor for the selected key on the next idle (safe from signal handlers
    /// of widgets that are about to be replaced).
    pub fn rebuild_editor(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(app) = weak.upgrade() {
                app.select(app.selected.get());
            }
        });
    }

    pub fn deck_info(&self) -> DeckInfo {
        self.deck.borrow().clone()
    }

    // --- editing & saving -----------------------------------------------------------------

    /// A key on the page being edited.
    pub fn key(&self, i: u8) -> Option<KeyConfig> {
        self.config.borrow().key(self.page.borrow().as_deref(), i).cloned()
    }

    /// Change the keys of the page being edited.
    fn with_page<R>(&self, f: impl FnOnce(&mut Keys) -> R) -> R {
        let page = self.page.borrow();
        let mut cfg = self.config.borrow_mut();
        let cfg = &mut *cfg;
        f(match cfg.page_mut(page.as_deref()) {
            Some(keys) => keys,
            // The page vanished under us; edit home rather than lose the change.
            None => &mut cfg.keys,
        })
    }

    /// Mutate a key, refresh its preview and queue a save.
    pub fn edit(self: &Rc<Self>, i: u8, f: impl FnOnce(&mut KeyConfig)) {
        self.with_page(|keys| f(keys.get_mut(i)));
        self.refresh_key(i);
        self.schedule_save();
    }

    /// The key `i` of the page being edited, to edit later with [`App::edit_ref`].
    pub fn key_ref(&self, i: u8) -> KeyRef {
        KeyRef { page: self.page.borrow().clone(), index: i }
    }

    /// Like [`App::edit`], for a key that may no longer be on screen. Returns false (and
    /// says so) when its page has gone.
    pub fn edit_ref(self: &Rc<Self>, at: &KeyRef, f: impl FnOnce(&mut KeyConfig)) -> bool {
        if self.locked.borrow().is_some() {
            return false;
        }
        match self.config.borrow_mut().page_mut(at.page.as_deref()) {
            Some(keys) => f(keys.get_mut(at.index)),
            None => {
                self.toast("That key's page was deleted, so the change was dropped");
                return false;
            }
        }
        if *self.page.borrow() == at.page {
            self.refresh_key(at.index);
        }
        self.schedule_save();
        true
    }

    pub fn clear_key(self: &Rc<Self>, i: u8) {
        self.with_page(|keys| keys.clear(i));
        self.refresh_key(i);
        self.schedule_save();
        self.rebuild_editor();
    }

    /// Mutate a dial, refresh its preview and queue a save. Like [`App::edit_ref`], a
    /// late result (a picker) is dropped while config.toml can't be parsed.
    pub fn edit_dial(self: &Rc<Self>, i: u8, f: impl FnOnce(&mut DialConfig)) {
        if self.locked.borrow().is_some() {
            return;
        }
        f(self.config.borrow_mut().dial_mut(i));
        self.refresh_dial(i);
        self.refresh_strip();
        self.schedule_save();
    }

    /// The action `at` names; `None` if its page has gone.
    pub fn action_at(&self, at: &ActionTarget) -> Option<Action> {
        let cfg = self.config.borrow();
        match at.dial_slot() {
            Some((i, dir)) => Some(cfg.dial(i).map(|d| d.action(dir).clone()).unwrap_or_default()),
            None => {
                let ActionTarget::Key(k) = at else { return None };
                cfg.page(k.page()).map(|keys| keys.get(k.index).map(|kc| kc.action.clone()).unwrap_or_default())
            }
        }
    }

    /// Replace the action `at` names. False when it can't: config.toml is broken (both
    /// edit paths drop late results then) or the key's page has gone.
    pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool {
        match at.dial_slot() {
            Some((i, dir)) => {
                let ok = self.locked.borrow().is_none();
                self.edit_dial(i, |d| *d.action_mut(dir) = act);
                ok
            }
            None => match at {
                ActionTarget::Key(k) => self.edit_ref(k, |kc| kc.action = act),
                _ => false,
            },
        }
    }

    pub fn clear_dial(self: &Rc<Self>, i: u8) {
        self.config.borrow_mut().clear_dial(i);
        self.refresh_dial(i);
        self.refresh_strip();
        self.schedule_save();
        self.rebuild_editor();
    }

    pub fn schedule_save(self: &Rc<Self>) {
        if let Some(id) = self.save_source.borrow_mut().take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if let Some(app) = weak.upgrade() {
                app.save_source.borrow_mut().take();
                app.save_now();
            }
        });
        *self.save_source.borrow_mut() = Some(id);
    }

    fn flush_save(self: &Rc<Self>) {
        if let Some(id) = self.save_source.borrow_mut().take() {
            id.remove();
            self.save_now();
        }
    }

    fn save_now(self: &Rc<Self>) {
        if self.locked.borrow().is_some() {
            return;
        }
        // Someone edited config.toml since we last looked: theirs wins.
        let on_disk = std::fs::read_to_string(paths::config_file()).ok();
        let changed = on_disk != *self.disk.borrow();
        if changed {
            self.reload_from_disk(true);
            return;
        }
        let saved = self.config.borrow().save();
        match saved {
            Ok(body) => {
                *self.disk.borrow_mut() = Some(body);
                self.stamp.set(FileStamp::of(&paths::config_file()));
            }
            Err(e) => self.toast(&format!("Couldn't save: {e:#}")),
        }
    }

    // --- external edits -------------------------------------------------------------------

    /// Pick up edits made to config.toml outside this window (a text editor, a dotfile
    /// sync, a restored backup). Polling the file's stamp also covers symlinked configs.
    fn watch_config(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            let stamp = FileStamp::of(&paths::config_file());
            if stamp != app.stamp.get() {
                app.stamp.set(stamp);
                let on_disk = std::fs::read_to_string(paths::config_file()).ok();
                // A deleted file is left alone; the next edit here writes it back.
                let changed = on_disk.is_some() && on_disk != *app.disk.borrow();
                if changed {
                    let pending = app.save_source.borrow().is_some();
                    app.reload_from_disk(pending);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Replace the in-memory config with what's on disk. `dropping` says an unsaved edit
    /// made here is being discarded in favour of the file.
    fn reload_from_disk(self: &Rc<Self>, dropping: bool) {
        if let Some(id) = self.save_source.borrow_mut().take() {
            id.remove();
        }
        let file = paths::config_file();
        self.stamp.set(FileStamp::of(&file));
        let Ok(src) = std::fs::read_to_string(&file) else { return };
        match Config::parse(&src) {
            Ok(cfg) => {
                {
                    let mut r = self.renderer.borrow_mut();
                    r.update_style(&cfg.style);
                    r.clear_cache();
                }
                *self.config.borrow_mut() = cfg;
                *self.locked.borrow_mut() = None;
                let page = self.page.borrow().clone().filter(|p| self.config.borrow().has_page(p));
                *self.page.borrow_mut() = page;
                self.settings_btn.set_popover(Some(&self.settings_popover()));
                self.sync_page_list();
                self.refresh_all();
                self.rebuild_editor();
                self.toast(if dropping {
                    "config.toml was changed outside omadeck, so your last change here was discarded"
                } else {
                    "Loaded changes made to config.toml outside omadeck"
                });
            }
            Err(e) => *self.locked.borrow_mut() = Some(format!("{e:#}")),
        }
        *self.disk.borrow_mut() = Some(src);
        self.show_lock();
    }

    /// While config.toml can't be parsed, say why and stop editing, so nothing here can
    /// overwrite the user's file.
    fn show_lock(&self) {
        let locked = self.locked.borrow();
        if let Some(e) = locked.as_deref() {
            let first = e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(e);
            self.config_banner.set_title(&glib::markup_escape_text(&format!(
                "config.toml has a mistake ({first}). Fix it in a text editor; omadeck won't change it until then."
            )));
            log::warn!("config.toml can't be parsed, editing is off: {e}");
        }
        self.config_banner.set_revealed(locked.is_some());
        self.body.set_sensitive(locked.is_none());
        self.settings_btn.set_sensitive(locked.is_none());
    }

    // --- pages ----------------------------------------------------------------------------

    pub fn current_page(&self) -> Option<String> {
        self.page.borrow().clone()
    }

    /// Edit another page. With `tell_daemon` the deck shows it too, so edits land live.
    pub fn show_page(self: &Rc<Self>, page: Option<String>, tell_daemon: bool) {
        let page = page.filter(|p| self.config.borrow().has_page(p));
        let changed = *self.page.borrow() != page;
        *self.page.borrow_mut() = page.clone();
        if changed {
            self.sync_page_list();
            // Keys are per page, so a key's editor closes; dials aren't, so theirs stays.
            if matches!(self.selected.get(), Some(Selection::Key(_))) {
                self.select(None);
            }
            self.refresh_all();
        }
        if tell_daemon {
            self.send(DaemonCommand::ShowPage { page });
        }
    }

    /// Mirror the config's pages in the page switcher.
    fn sync_page_list(&self) {
        self.syncing_pages.set(true);
        let cfg = self.config.borrow();
        let names = cfg.page_names();
        let mut items: Vec<&str> = vec![HOME];
        items.extend(names.iter().map(String::as_str));
        self.page_list.splice(0, self.page_list.n_items(), &items);
        let page = self.page.borrow();
        let pos = page.as_deref().and_then(|p| names.iter().position(|n| n == p)).map(|n| n + 1).unwrap_or(0);
        self.page_dropdown.set_selected(pos as u32);
        self.page_actions.set_visible(page.is_some());
        self.folder_toggle.set_active(page.as_deref().is_some_and(|p| cfg.is_folder(p)));
        self.syncing_pages.set(false);
    }

    /// Add a page that starts with a Back key. Returns its name. A `folder` is skipped by
    /// flicks and Next/previous page: pages made for an Open-page action are folders,
    /// pages made with **+** are not.
    pub fn add_page(self: &Rc<Self>, name: &str, folder: bool) -> String {
        let name = self.config.borrow_mut().add_page(name, folder, back_key());
        self.sync_page_list();
        self.schedule_save();
        name
    }

    /// A dialog asking for a page name; `done` gets the accepted name.
    fn page_name_dialog(
        self: &Rc<Self>,
        heading: &str,
        accept: &str,
        initial: &str,
        current: Option<String>,
        done: impl Fn(&Rc<Self>, String) + 'static,
    ) {
        let dialog = adw::AlertDialog::new(Some(heading), None);
        dialog.add_responses(&[("cancel", "Cancel"), ("ok", accept)]);
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("cancel");
        let entry = gtk::Entry::builder().text(initial).activates_default(true).build();
        dialog.set_extra_child(Some(&entry));
        let weak = Rc::downgrade(self);
        let d = dialog.clone();
        let check = move |e: &gtk::Entry| {
            let Some(app) = weak.upgrade() else { return };
            let name = e.text();
            let problem = if current.as_deref() == Some(name.trim()) {
                None
            } else {
                app.config.borrow().page_name_problem(&name)
            };
            d.set_body(problem.unwrap_or(""));
            d.set_response_enabled("ok", problem.is_none());
        };
        check(&entry);
        entry.connect_changed(check);
        let weak = Rc::downgrade(self);
        let e = entry.clone();
        dialog.connect_response(Some("ok"), move |_, _| {
            if let Some(app) = weak.upgrade() {
                done(&app, e.text().trim().to_string());
            }
        });
        dialog.present(Some(&self.window));
        entry.grab_focus();
    }

    fn ask_new_page(self: &Rc<Self>) {
        let suggestion = self.config.borrow().unique_page_name("Page");
        self.page_name_dialog("New page", "Create", &suggestion, None, |app, name| {
            let name = app.add_page(&name, false);
            app.show_page(Some(name), true);
            app.toast("New page — give a key the “Open page” action to reach it from your deck");
        });
    }

    fn ask_rename_page(self: &Rc<Self>) {
        let Some(old) = self.current_page() else { return };
        self.page_name_dialog("Rename page", "Rename", &old.clone(), Some(old.clone()), move |app, new| {
            if new == old {
                return;
            }
            app.config.borrow_mut().rename_page(&old, &new);
            *app.page.borrow_mut() = Some(new.clone());
            app.sync_page_list();
            app.refresh_all();
            app.schedule_save();
            app.rebuild_editor();
            app.send(DaemonCommand::ShowPage { page: Some(new) });
        });
    }

    fn ask_delete_page(self: &Rc<Self>) {
        let Some(name) = self.current_page() else { return };
        let links = self.config.borrow().links_to(&name);
        let body = match links {
            0 => "Its keys will be deleted too.".to_string(),
            1 => {
                "Its keys will be deleted too, and the key or dial that opens it will stop doing anything.".to_string()
            }
            n => format!(
                "Its keys will be deleted too, and the {n} keys or dials that open it will stop doing anything."
            ),
        };
        let dialog = adw::AlertDialog::new(Some(&format!("Delete “{name}”?")), Some(&body));
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.connect_response(Some("delete"), move |_, _| {
            let Some(app) = weak.upgrade() else { return };
            app.config.borrow_mut().remove_page(&name);
            app.schedule_save();
            app.show_page(None, true);
            app.sync_page_list();
        });
        dialog.present(Some(&self.window));
    }

    fn send(&self, cmd: DaemonCommand) {
        if let Some(sender) = self.daemon.borrow_mut().as_mut()
            && let Err(e) = sender.send(&cmd)
        {
            log::warn!("telling omadeckd: {e}");
        }
    }

    pub fn toast(&self, msg: &str) {
        let t = adw::Toast::new(msg);
        t.set_timeout(4);
        self.toasts.add_toast(t);
    }

    // --- theme ----------------------------------------------------------------------------

    fn apply_theme_css(&self) {
        let r = self.renderer.borrow();
        let t = &r.theme;
        self.theme_css.load_from_string(&format!("@define-color deck_accent {};", t.accent.hex()));
    }

    /// Follow `omarchy theme set` live, like the daemon does.
    fn watch_theme(self: &Rc<Self>) {
        let file = gio::File::for_path(paths::omarchy_current_dir().join("theme.name"));
        let Ok(monitor) = file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) else { return };
        let weak = Rc::downgrade(self);
        monitor.connect_changed(move |_, _, _, ev| {
            if !matches!(ev, gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Created) {
                return;
            }
            if let Some(app) = weak.upgrade() {
                app.renderer.borrow_mut().reload_theme();
                app.apply_theme_css();
                app.refresh_all();
                app.rebuild_editor();
            }
        });
        *self.theme_monitor.borrow_mut() = Some(monitor);
    }

    // --- settings popover -----------------------------------------------------------------

    fn settings_popover(self: &Rc<Self>) -> gtk::Popover {
        let cfg = self.config.borrow();
        let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 5.0, 100.0, 5.0);
        scale.set_value(self.live_brightness.get().unwrap_or(cfg.brightness) as f64);
        scale.set_hexpand(true);
        scale.set_width_request(170);
        let weak = Rc::downgrade(self);
        scale.connect_value_changed(move |s| {
            let Some(app) = weak.upgrade() else { return };
            if app.syncing_brightness.get() {
                return;
            }
            let value = s.value().round() as u8;
            app.slider_moved.set(Some(std::time::Instant::now()));
            if app.daemon_up.get() {
                // The daemon owns the brightness: it applies and saves it, and its
                // `Event::Brightness` updates the preview. config.toml isn't touched.
                app.send(DaemonCommand::SetBrightness { value });
            } else {
                // No daemon: the config's value is the start value, as upstream.
                app.config.borrow_mut().brightness = value;
                app.schedule_save();
                app.refresh_strip();
            }
        });
        *self.brightness_scale.borrow_mut() = Some(scale.clone());
        let bright = adw::ActionRow::builder().title("Brightness").build();
        bright.add_suffix(&scale);
        list.append(&bright);

        if self.deck.borrow().strip.is_some() {
            list.append(&self.strip_background_row());
        }

        let follow = adw::SwitchRow::builder()
            .title("Match Omarchy theme")
            .subtitle("Key colours follow your current theme")
            .active(cfg.style.follow_theme)
            .build();
        let weak = Rc::downgrade(self);
        follow.connect_active_notify(move |row| {
            if let Some(app) = weak.upgrade() {
                app.config.borrow_mut().style.follow_theme = row.is_active();
                app.refresh_all();
                app.schedule_save();
                app.rebuild_editor();
            }
        });
        list.append(&follow);

        let folder =
            adw::ButtonRow::builder().title("Open config folder").end_icon_name("folder-open-symbolic").build();
        folder.connect_activated(|_| {
            let _ = std::fs::create_dir_all(paths::config_dir());
            let uri = gio::File::for_path(paths::config_dir()).uri();
            let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
        });
        list.append(&folder);

        let about = adw::ButtonRow::builder().title("About omadeck").build();
        let weak: Weak<Self> = Rc::downgrade(self);
        about.connect_activated(move |_| {
            if let Some(app) = weak.upgrade() {
                app.show_about();
            }
        });
        list.append(&about);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .width_request(380)
            .build();
        content.append(&list);
        gtk::Popover::builder().child(&content).build()
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("omadeck")
            .application_icon(crate::APP_ID)
            .developer_name("Raymond Brady & Claude")
            .version(env!("CARGO_PKG_VERSION"))
            .comments("Elgato Stream Deck for Omarchy: launch apps, run scripts, play sounds and open websites — with your theme on every key.")
            .license_type(gtk::License::MitX11)
            .website("https://github.com/thewizster/omadeck")
            .build();
        about.present(Some(&self.window));
    }

    // --- daemon ---------------------------------------------------------------------------

    fn start_daemon(self: &Rc<Self>) {
        let ok = Command::new("systemctl")
            .args(["--user", "enable", "--now", "omadeckd.service"])
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            self.toast("Started the omadeck service");
            return;
        }
        // Not installed as a service (e.g. running from a build tree): start it directly.
        let sibling = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("omadeckd")));
        let bin = sibling.filter(|p| p.exists()).map(|p| p.into_os_string()).unwrap_or_else(|| "omadeckd".into());
        match Command::new(&bin).stdin(std::process::Stdio::null()).spawn() {
            Ok(_) => self.toast("Started omadeckd (run scripts/install.sh to start it automatically at login)"),
            Err(e) => self.toast(&format!("Couldn't start omadeckd: {e}")),
        }
    }

    /// Follow the daemon's event feed on a worker thread.
    fn listen(self: &Rc<Self>) {
        let (tx, rx) = async_channel::unbounded::<Msg>();
        std::thread::Builder::new()
            .name("ipc".into())
            .spawn(move || {
                loop {
                    if let Ok((sender, events)) = ipc::connect() {
                        if tx.send_blocking(Msg::DaemonUp(sender)).is_err() {
                            return;
                        }
                        for e in events {
                            if tx.send_blocking(Msg::Event(e)).is_err() {
                                return;
                            }
                        }
                    }
                    if tx.send_blocking(Msg::DaemonDown).is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
            .expect("spawn ipc thread");
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                let Some(app) = weak.upgrade() else { break };
                app.handle(msg);
            }
        });
    }

    fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::DaemonUp(sender) => {
                *self.daemon.borrow_mut() = Some(sender);
                self.daemon_up.set(true);
                self.banner.set_revealed(false);
            }
            Msg::DaemonDown => {
                self.daemon.borrow_mut().take();
                self.daemon_up.set(false);
                self.connected.set(false);
                self.banner.set_revealed(true);
                self.live_brightness.set(None);
                self.panel_status.borrow_mut().clear();
                let b = self.config.borrow().brightness;
                self.sync_slider(b);
                self.refresh_strip();
            }
            Msg::Event(Event::Connected { deck }) => {
                self.connected.set(true);
                let relayout = {
                    let cur = self.deck.borrow();
                    cur.rows != deck.rows || cur.cols != deck.cols || cur.model != deck.model || cur.dials != deck.dials
                };
                // A replugged deck may be smaller: a selection past its keys or dials
                // would edit a slot the hardware doesn't have.
                let stale = match self.selected.get() {
                    Some(Selection::Key(i)) => i as usize >= deck.key_count() as usize,
                    Some(Selection::Dial(i)) => i as usize >= deck.dials as usize,
                    None => false,
                };
                *self.deck.borrow_mut() = deck;
                if relayout {
                    self.rebuild_grid();
                }
                if stale {
                    self.select(None);
                }
            }
            // The deck that showed the panels is gone: forget them and rebuild the preview.
            Msg::Event(Event::Disconnected) => {
                self.connected.set(false);
                self.panel_status.borrow_mut().clear();
                self.refresh_strip();
            }
            // Follow the deck: pressing a page key there shows that page here too.
            Msg::Event(Event::PageChanged { page }) => self.show_page(page, false),
            Msg::Event(Event::KeyDown { index, page }) => {
                self.show_page(page, false);
                if let Some((b, _)) = self.keys.borrow().get(index as usize) {
                    b.add_css_class("pressed");
                }
                if self.selected.get() != Some(Selection::Key(index)) {
                    self.select(Some(Selection::Key(index)));
                }
            }
            Msg::Event(Event::KeyUp { index }) => {
                if let Some((b, _)) = self.keys.borrow().get(index as usize) {
                    b.remove_css_class("pressed");
                }
            }
            Msg::Event(Event::ActionFailed { index, message }) => {
                self.toast(&format!("Key {}: {message}", index + 1));
            }
            Msg::Event(Event::DialDown { index }) => {
                if let Some((b, _)) = self.dials.borrow().get(index as usize) {
                    b.add_css_class("pressed");
                }
                if self.selected.get() != Some(Selection::Dial(index)) {
                    self.select(Some(Selection::Dial(index)));
                }
            }
            Msg::Event(Event::DialUp { index }) => {
                if let Some((b, _)) = self.dials.borrow().get(index as usize) {
                    b.remove_css_class("pressed");
                }
            }
            Msg::Event(Event::DialActionFailed { index, message }) => {
                self.toast(&format!("Dial {}: {message}", index + 1));
            }
            Msg::Event(Event::Brightness { value }) => {
                self.live_brightness.set(Some(value));
                self.sync_slider(value);
                self.refresh_strip();
            }
            Msg::Event(Event::DialStatus { index, status }) => {
                self.panel_status.borrow_mut().insert(index, status);
                self.refresh_strip();
            }
            // Dial turns and strip touches: nothing in the configurator reacts to them yet.
            Msg::Event(
                Event::DialTurn { .. }
                | Event::StripTap { .. }
                | Event::StripLongPress { .. }
                | Event::StripSwipe { .. },
            ) => {}
        }
        self.update_title();
    }

    fn update_title(&self) {
        let sub = if !self.daemon_up.get() {
            "Service not running".to_string()
        } else if self.connected.get() {
            format!("{} · connected", self.deck.borrow().model)
        } else {
            "Waiting for a Stream Deck…".to_string()
        };
        self.title.set_subtitle(&sub);
    }
}
