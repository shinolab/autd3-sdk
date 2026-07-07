use std::io::Write;

const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

pub fn check(msg: &str) {
    for line in msg.lines() {
        println!("{YELLOW}{BOLD}Check{RESET}: {line}");
    }
}

pub async fn wait_enter(msg: &str) {
    check(msg);
    println!("Press Enter to continue...");
    read_line().await;
}

pub async fn prompt(msg: &str) -> String {
    print!("{GREEN}{BOLD}{msg}{RESET}: ");
    let _ = std::io::stdout().flush();
    read_line().await
}

async fn read_line() -> String {
    tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        s
    })
    .await
    .unwrap_or_default()
}
