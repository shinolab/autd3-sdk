use std::path::PathBuf;

pub fn tool_bin(name: &str) -> std::io::Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(exe_dir()?.join(exe_name))
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[url])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", "", url])
    } else {
        ("xdg-open", &[url])
    };
    let mut command = std::process::Command::new(program);
    command.args(args);
    crate::process::no_window(&mut command);
    command.spawn()?;
    Ok(())
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
