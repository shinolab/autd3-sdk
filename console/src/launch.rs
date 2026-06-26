use std::path::PathBuf;

pub fn tool_bin(subdir: &str, name: &str) -> std::io::Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(exe_dir()?.join(subdir).join(exe_name))
}

pub fn tool_path(subdir: &str, rel: &str) -> std::io::Result<PathBuf> {
    Ok(exe_dir()?.join(subdir).join(rel))
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
