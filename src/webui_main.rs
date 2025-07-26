use laszoo::config::Config;
use laszoo::webui::WebUI;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let webui = WebUI::new(Arc::new(config));
    
    println!("Starting Laszoo Web UI on http://localhost:8080");
    webui.start(8080).await?;
    
    Ok(())
}