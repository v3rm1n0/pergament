//! jwlinux-gtk: GTK4/libadwaita reader for the jwlinux library.

mod online;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

const APP_ID: &str = "io.github.jwlinux.Reader";

fn main() -> glib::ExitCode {
    // `jwlinux-gtk [PUBLICATION [DOCUMENT|BOOK:CHAPTER]]` opens a page directly.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let initial = args.first().map(|p| (p.clone(), args.get(1).cloned()));

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_startup(|app| {
        setup_theme_action(app);
        app.set_accels_for_action("win.import", &["<Ctrl>o"]);
        app.set_accels_for_action("win.search-online", &["<Ctrl>f"]);
        app.set_accels_for_action("win.back", &["<Alt>Left"]);
        app.set_accels_for_action("win.forward", &["<Alt>Right"]);
        app.set_accels_for_action("window.close", &["<Ctrl>w"]);
    });
    app.connect_activate(move |app| match window::build(app, initial.clone()) {
        Ok(win) => win.present(),
        Err(e) => {
            eprintln!("jwlinux-gtk: {e}");
            app.quit();
        }
    });
    app.run_with_args(&["jwlinux-gtk"])
}

/// `app.theme` with state "system" | "light" | "dark".
fn setup_theme_action(app: &adw::Application) {
    let action = gio::SimpleAction::new_stateful(
        "theme",
        Some(glib::VariantTy::STRING),
        &"system".to_variant(),
    );
    action.connect_activate(|action, param| {
        let Some(value) = param.and_then(|p| p.get::<String>()) else {
            return;
        };
        let scheme = match value.as_str() {
            "light" => adw::ColorScheme::ForceLight,
            "dark" => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
        action.set_state(&value.to_variant());
    });
    app.add_action(&action);
}
