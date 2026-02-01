//! Navi library exports for testing

use clap::ValueEnum;

pub mod core;
pub mod inference;
pub mod tui;

#[cfg(test)]
pub mod test_support;

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum Provider {
    #[default]
    OpenRouter,
    LmStudio,
}
