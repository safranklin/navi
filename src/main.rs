mod api;
mod core;
mod inference;
#[cfg(test)]
mod test_support;
mod tui;

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use std::fs::File;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    // Initialize file logger - writes to navi.log in current directory
    let log_config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    if let Ok(log_file) = File::create("navi.log") {
        let _ = WriteLogger::init(LevelFilter::Debug, log_config, log_file);
    }

    log::info!("Navi starting up");

    tui::run()
}