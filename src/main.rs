#![forbid(unsafe_code)]

use fanwaave_cli::{args, commands, config, error::CliError, flags};
use fanwaave_lib_core::fanwaave_config::{parse_fanwaave_config, resolve_fanwaave_config};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(err.exit_code());
    }
}

fn run() -> Result<(), CliError> {
    let argv = std::env::args().collect::<Vec<_>>();
    if argv
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.as_str(), "-h" | "--help" | "help"))
    {
        print!("{}", args::help_text());
        return Ok(());
    }

    let applied = flags::apply_cli_flags()?;
    let fanwaave_text = std::fs::read_to_string(".fanwaave-cfg.toml")
        .map_err(|error| CliError::Config(format!("cannot read .fanwaave-cfg.toml: {error}")))?;
    let fanwaave = parse_fanwaave_config(&fanwaave_text)
        .map_err(|error| CliError::Config(error.to_string()))?;
    let resolved = resolve_fanwaave_config(
        &fanwaave,
        &applied.ambient,
        &applied.argv_overrides,
    )
    .map_err(|error| CliError::Config(error.to_string()))?;
    let cfg = config::Config::from_sources(&applied.merged, &resolved)?;
    commands::dispatch(&cfg, applied.command)
}
