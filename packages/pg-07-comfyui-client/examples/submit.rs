// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_07_comfyui_client::{Client, SubmitRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(base_url) = std::env::args().nth(1) else {
        return Err("usage: cargo run --example submit -- <ComfyUI base URL>".into());
    };
    let request: SubmitRequest = serde_json::from_str(include_str!("../samples/submit.json"))?;
    let client = Client::new(&base_url)?;
    let job = client.submit(&request).await?;
    println!("Submitted {}", job.prompt_id);
    let history = client.wait(&job.prompt_id).await?;
    for file in history.files() {
        let bytes = client.fetch_output(file).await?;
        println!("Fetched {} bytes", bytes.len());
    }
    Ok(())
}
