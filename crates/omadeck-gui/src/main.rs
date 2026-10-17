//! omadeck — Stream Deck configurator for Omarchy.

mod app;
mod apps;
mod editor;
mod media;
mod pages;
mod starter;

use adw::prelude::*;

/// The configurator's application id, which is also its window class: the daemon ignores
/// focus on it.
pub const APP_ID: &str = omadeck_core::follow::CONFIGURATOR_CLASS;

fn main() -> gtk::glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).format_timestamp(None).init();
    let application = adw::Application::builder().application_id(APP_ID).build();
    application.connect_activate(|application| {
        // Single window: re-activating just raises it.
        if let Some(win) = application.active_window() {
            win.present();
            return;
        }
        app::App::build(application);
    });
    application.run()
}
