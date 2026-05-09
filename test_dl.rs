use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    
    let res = client.get("https://github.com/SagerNet/sing-box/releases/latest").send().await?;
    let mut version = "1.10.1".to_string(); // fallback
    if res.status().is_redirection() {
        if let Some(loc) = res.headers().get(reqwest::header::LOCATION) {
            let loc_str = loc.to_str().unwrap_or("");
            if let Some(tag) = loc_str.split('/').last() {
                version = tag.trim_start_matches('v').to_string();
            }
        }
    }
    println!("Latest version: {}", version);
    Ok(())
}
