mod core;
mod inference;
#[cfg(test)]
mod test_support;
mod tui;

use clap::Parser;
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use std::fs::File;

#[derive(Parser)]
#[command(name = "navi", about = "Model-agnostic AI assistant")]
struct Args {
    /// LLM provider to use (overrides config file and env vars)
    #[arg(short, long)]
    provider: Option<String>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    dotenv::dotenv().ok();

    // Initialize file logger - writes to navi.log in current directory
    let log_config = ConfigBuilder::new().set_time_format_rfc3339().build();

    if let Ok(log_file) = File::create("navi.log") {
        let _ = WriteLogger::init(LevelFilter::Debug, log_config, log_file);
    }

    // Load and resolve config: defaults → config file → env vars → CLI flags
    let config = core::config::load_config().unwrap_or_else(|e| {
        log::warn!("Config error: {}, using defaults", e);
        core::config::NaviConfig::default()
    });
    let resolved = core::config::resolve(&config, args.provider.as_deref());

    log::info!(
        "Navi starting up: provider={}, model={}",
        resolved.provider,
        resolved.model_name,
    );

    tui::run(resolved)
}
