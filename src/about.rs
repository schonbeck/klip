//! The "About klip" dialog.

use crate::VERSION;
use gtk::prelude::*;

/// Show the About dialog (raises it if it's already open).
pub fn show(app: &gtk::Application, parent: Option<&gtk::Window>, token: Option<&str>) {
    if let Some(dialog) = app
        .windows()
        .into_iter()
        .find(|w| w.is::<gtk::AboutDialog>())
    {
        dialog.present();
        return;
    }
    let dialog = gtk::AboutDialog::builder()
        .application(app)
        .program_name("klip")
        .version(VERSION)
        .comments("A simple Wayland screenshot tool")
        .copyright("© 2026 Stephan Victor Schönbeck / Schönbeck ApS")
        .license_type(gtk::License::MitX11)
        .website("https://github.com/schonbeck/klip")
        .website_label("github.com/schonbeck/klip")
        .authors(["Stephan Victor Schönbeck"])
        .logo_icon_name(crate::desktop::icon_name())
        .modal(parent.is_some())
        .build();
    dialog.set_transient_for(parent);
    if let Some(token) = token {
        dialog.set_startup_id(token);
    }
    dialog.present();
}
