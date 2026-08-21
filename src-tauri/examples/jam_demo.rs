//! End-to-end wiring demo for the experimental Jams module.
//!
//! Run with:
//! ```text
//! cargo run --example jam_demo -- [path/to/jams.toml]
//! ```
//!
//! To do anything against Spotify you need a real OAuth access token. Export
//! it as `RUSTIFY_ACCESS_TOKEN` (obtain one through the app's login flow).
//! Without one the demo still runs: it prints the configuration surface and
//! exercises the error paths, which is useful while endpoint paths and
//! persisted-query hashes are still being captured.
//!
//! Tracing output defaults to `info`; for the module's debug logs:
//! ```text
//! $env:RUST_LOG = "rustify_lib::jams=debug"
//! ```

use std::sync::Arc;
use std::time::Duration;

use rustify_lib::jams::{AccessToken, JamConfig, JamCredentials, JamManager, TokenProvider};
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();

    // Optional path to a jams.toml; falls back to defaults (no endpoints).
    let config = std::env::args()
        .nth(1)
        .map(JamConfig::from_file)
        .transpose()?
        .unwrap_or_default();

    // Authenticate. In the real app this is the live OAuth session; here it is
    // read from the environment so the demo needs no other wiring.
    let access: Arc<dyn TokenProvider> = match std::env::var("RUSTIFY_ACCESS_TOKEN").ok() {
        Some(token) => {
            println!("using access token from RUSTIFY_ACCESS_TOKEN");
            Arc::new(AccessToken::new(token))
        }
        None => {
            println!("NOTE: RUSTIFY_ACCESS_TOKEN not set. Calls against Spotify will fail with a");
            println!("      clear token error; configuration checks still work.");
            Arc::new(AccessToken::default())
        }
    };

    println!("spclient base url : {}", config.spclient_base_url);
    println!("pathfinder url    : {}", config.pathfinder_url);
    println!("client-token url  : {}", config.client_token_url);
    println!("dealer url        : {}", config.dealer_url);
    println!(
        "spclient endpoints: {}",
        match config.spclient_endpoints.is_empty() {
            true => "<none configured>".to_owned(),
            false => config
                .spclient_endpoints
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        }
    );
    println!(
        "pathfinder hashes : {}",
        match config.pathfinder_hashes.is_empty() {
            true => "<none - capture them with a proxy>".to_owned(),
            false => config
                .pathfinder_hashes
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        }
    );

    let http = reqwest::Client::new();
    // Standalone credentials: a random device id, a self-minted client token
    // and no dealer connection id. Enough to exercise the wiring; NOT enough
    // to create a real jam, which Spotify binds to a live Connect device and
    // the dealer connection that device is registered on. The app supplies all
    // three from librespot (see `src/jams_bridge.rs`).
    let credentials = JamCredentials::standalone(access);
    println!(
        "device id         : {} (random - demo only)",
        credentials.identity.device_id
    );

    let manager = JamManager::new(http, config, credentials)?;
    let mut events = manager.events();

    // Best-effort create. Expect a 400 here even with a valid OAuth token:
    // the device id is invented and there is no connection id. Run it from
    // the app's Jams tab for the real thing.
    match manager.create(None).await {
        Ok(session) => println!("created jam {}", session.id),
        Err(e) => println!("create(): {e}"),
    }

    // Raw probe: the generic post/get accept any path, so once you have a real
    // capture you can try it without touching the business methods:
    //   let probe = manager.api().post("/jam/v1/some/path", &serde_json::json!({})).await;
    //   println!("probe: {probe:?}");

    println!("listening for dealer events for 15s...");
    let _ = timeout(Duration::from_secs(15), async {
        loop {
            match events.recv().await {
                Some(event) => println!("event: {event:?}"),
                None => {
                    println!("event stream closed");
                    break;
                }
            }
        }
    })
    .await;

    println!("demo done.");
    Ok(())
}
