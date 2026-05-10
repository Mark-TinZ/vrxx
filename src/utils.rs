use std::fs::DirBuilder;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;

/// Recursively creates a directory and all of its parent components if they are missing,
/// enforcing restricted permissions (0o700) on Unix systems to ensure security for sensitive data.
pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);

    #[cfg(unix)]
    builder.mode(0o700);

    builder.create(path)
}
