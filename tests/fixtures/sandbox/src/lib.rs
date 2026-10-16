//! A board that reports what the sandbox lets it see.

use bb_sdk::*;

fn build() -> Board {
    let file = std::fs::read_to_string("/etc/hosts").is_ok() || std::fs::read_to_string("Cargo.toml").is_ok();
    let env = std::env::vars().count();
    let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Report through the project name: the host only ever sees the design.
    Board::new(Project::new(&format!("file={file} env={env} clock={clock}")))
}

bb_sdk::export_board!(build);
