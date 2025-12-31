mod api;
mod core;
mod tui;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    tui::run()
}