use std::path::Path;

pub fn secure_create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}
