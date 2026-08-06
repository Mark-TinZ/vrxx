use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    vrxx::tui::run_tui().await
}
