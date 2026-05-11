use std::io;
use std::path::Path;

#[cfg(unix)]
pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    use std::fs::DirBuilder;
    use std::os::unix::fs::DirBuilderExt;

    let path = path.as_ref();
    if path.exists() {
        return Ok(());
    }

    let mut builder = DirBuilder::new();
    builder.recursive(true);
    builder.mode(0o700);

    builder.create(path)
}

#[cfg(not(unix))]
pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std::fs::create_dir_all(path)
}
