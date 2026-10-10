//! Configuration and wiring. Behaviour lives in the crates under `crates/`.

mod config;

use config::Config;

fn main() -> std::process::ExitCode {
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!(
        "watcher starting: model {} via {}, prices from {}",
        config.model_name, config.model_provider, config.price_source
    );
    std::process::ExitCode::SUCCESS
}
