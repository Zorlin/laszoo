use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use crate::config::LoggingConfig;
use crate::error::Result;

pub fn init_logging(config: &LoggingConfig, verbose: bool) -> Result<()> {
    // Set up env filter with the configured level or RUST_LOG env var
    let env_filter = if verbose {
        EnvFilter::from_default_env()
            .add_directive("laszoo=debug".parse().unwrap())
    } else {
        match std::env::var("RUST_LOG") {
            Ok(_) => EnvFilter::from_default_env(),
            Err(_) => {
                let level = match config.level.as_str() {
                    "trace" => Level::TRACE,
                    "debug" => Level::DEBUG,
                    "info" => Level::INFO,
                    "warn" => Level::WARN,
                    "error" => Level::ERROR,
                    _ => Level::INFO,
                };
                EnvFilter::from_default_env()
                    .add_directive(format!("laszoo={}", level).parse().unwrap())
            }
        }
    };
    
    // Configure format
    let format = config.format.clone();
    
    // Set up subscriber based on format
    match format.as_str() {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_target(true))
                .init();
        }
        "compact" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().compact().with_target(false))
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().pretty().with_target(true))
                .init();
        }
    }
    
    Ok(())
}