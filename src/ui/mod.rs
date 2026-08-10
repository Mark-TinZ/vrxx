pub mod components;
pub mod import_dialog;
pub mod models;
pub mod pages;
pub mod qr_dialog;

use gtk::prelude::*;

/// Helper function to configure the primary menu button for a page.
/// It loads the shared menu model and injects the Theme Switcher widget.
pub fn setup_primary_menu(menu_button: &gtk::MenuButton) {
    // 1. Load the shared menu model
    let builder = gtk::Builder::from_resource("/ru/mark/vrxx/ui/menus.ui");
    if let Some(model) = builder.object::<gtk::gio::MenuModel>("primary_menu") {
        menu_button.set_menu_model(Some(&model));
    }

    // 2. Add the custom theme switcher widget to the popover
    // Note: set_menu_model automatically creates a GtkPopoverMenu if one doesn't exist
    if let Some(popover) = menu_button
        .popover()
        .and_then(|p| p.downcast::<gtk::PopoverMenu>().ok())
    {
        let switcher = components::theme_switcher::VrxxThemeSwitcher::new();
        popover.add_child(&switcher, "theme_switcher");
    }
}
mod proxy_tests;
mod tests;
