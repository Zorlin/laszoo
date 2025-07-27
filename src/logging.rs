use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use crate::config::LoggingConfig;
use crate::error::Result;

pub fn init_logging(config: &LoggingConfig, verbose: bool) -> Result<()> {
    // Load environment variables from /etc/default/laszoo if it exists
    if let Ok(content) = std::fs::read_to_string("/etc/default/laszoo") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                
                // Only set if not already set in environment
                if std::env::var(key).is_err() {
                    std::env::set_var(key, value);
                }
            }
        }
    }
    
    // Set up env filter with the configured level or RUST_LOG/LASZOO_LOG_LEVEL env var
    let env_filter = if verbose {
        EnvFilter::from_default_env()
            .add_directive("laszoo=debug".parse().unwrap())
    } else {
        // Check LASZOO_LOG_LEVEL first, then RUST_LOG
        let log_level = std::env::var("LASZOO_LOG_LEVEL")
            .or_else(|_| std::env::var("RUST_LOG"));
            
        match log_level {
            Ok(level) => {
                // If it's a simple level like "debug", convert to "laszoo=debug"
                if matches!(level.as_str(), "trace" | "debug" | "info" | "warn" | "error") {
                    EnvFilter::from_default_env()
                        .add_directive(format!("laszoo={}", level).parse().unwrap())
                } else {
                    // Otherwise assume it's a full RUST_LOG style filter
                    EnvFilter::try_new(&level).unwrap_or_else(|_| EnvFilter::from_default_env())
                }
            },
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