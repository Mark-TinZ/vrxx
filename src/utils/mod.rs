use std::fs;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;
pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = path.as_ref();
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        builder.mode(0o700);
    }
    builder.create(path)
}
