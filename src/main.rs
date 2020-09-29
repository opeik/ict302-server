mod data_fetcher;
mod types;

use tokio::sync::Mutex;
use tracing::{error, info};
use tracing_subscriber;
use types::*;

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Server started");

    let state = Arc::new(Mutex::new(State::new()));

    {
        let state = state.clone();

        tokio::spawn(async move {
            match start_server(state).await {
                Ok(_) => info!("Server exiting"),
                Err(e) => error!("Error occurred: {}", e),
            }
        })
        .await?;
    }

    Ok(())
}

async fn start_server(state: Arc<Mutex<State>>) -> Result<()> {
    match data_fetcher::fetch_entities(state).await {
        Ok(_) => info!("Fetched entities from API"),
        Err(e) => error!("Failed to fetch entities from API: {}", e),
    };

    Ok(())
}
