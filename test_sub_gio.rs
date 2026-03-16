use gtk::gio;

fn main() {
    let sub = gio::Subprocess::newv(&[std::ffi::OsStr::new("ls")], gio::SubprocessFlags::NONE).unwrap();
    let ok = sub.is_successful();
}
