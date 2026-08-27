#![forbid(unsafe_code)]

use crate::config::Config;
use crate::error::CliError;

pub fn run(config: &Config) -> Result<(), CliError> {
    let body = serde_json::json!({
        "service": "fanwaave",
        "api_base": config.api_base,
    });
    if config.json {
        println!("{body}");
    } else {
        println!("fanwaave @ {}", config.api_base);
    }
    Ok(())
}
