use std::path::PathBuf;

pub fn tool_bin(name: &str) -> std::io::Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(exe_dir()?.join(exe_name))
}

fn exe_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve the console executable directory",
        )
    })
}
