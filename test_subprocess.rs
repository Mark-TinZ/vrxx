use gtk::{gio, glib};

fn main() {
    let sub = gio::Subprocess::new(&["echo", "hello"], gio::SubprocessFlags::STDOUT_PIPE).unwrap();
}
