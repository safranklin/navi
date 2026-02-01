mod core;
mod inference;
#[cfg(test)]
mod test_support;
mod tui;

use clap::Parser;
use navi::Provider;
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use std::fs::File;

#[derive(Parser)]
#[command(name = "navi", about = "Model-agnostic AI assistant")]
struct Args {
    /// LLM provider to use
    #[arg(short, long, default_value_t, value_enum)]
    provider: Provider,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    dotenv::dotenv().ok();

    // Initialize file logger - writes to navi.log in current directory
    let log_config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    if let Ok(log_file) = File::create("navi.log") {
        let _ = WriteLogger::init(LevelFilter::Debug, log_config, log_file);
    }

    log::info!("Navi starting up with provider: {:?}", args.provider);

    tui::run(args.provider)
}