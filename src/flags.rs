#![forbid(unsafe_code)]

use std::path::Path;

use crate::args::Command;
use crate::env_map::{merge_env, EnvMap};
use crate::error::CliError;
use flags2env::BundledFlags2Env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedCliFlags {
    pub command: Command,
    pub ambient: EnvMap,
    pub argv_overrides: EnvMap,
    pub merged: EnvMap,
}

pub fn parse_cli_flags(argv: &[String], config_path: &Path) -> Result<(Command, EnvMap), CliError> {
    let config_path = config_path
        .to_str()
        .ok_or_else(|| CliError::Config(".cli-flags.toml path is not valid UTF-8".into()))?;
    let parser = BundledFlags2Env::new();
    parser.audit_config(Some(config_path)).map_err(|error| {
        CliError::Config(format!("flags-2-env configuration audit failed: {error}"))
    })?;
    let parsed = parser
        .parse_structured(argv, Some(config_path))
        .map_err(|error| CliError::Config(format!("flags-2-env parse failed: {error}")))?;
    if !parsed.unknown_options.is_empty() {
        return Err(CliError::Usage(format!(
            "unknown command-line option(s): {}",
            parsed.unknown_options.join(", ")
        )));
    }
    if !parsed.errors.is_empty() {
        return Err(CliError::Usage(format!(
            "invalid command-line value(s): {}",
            parsed.errors.join("; ")
        )));
    }
    let command = match parsed.command.as_str() {
        "" | "help" => Command::Help,
        "health" => Command::Health,
        "status" => Command::Status,
        other => return Err(CliError::Usage(format!("unknown command {other}"))),
    };

    // `flags` contains TOML defaults and can incorrectly shadow a real process
    // environment when merged after it. `provided_flags` is the canonical
    // argv-only channel exposed by flags-2-env for this precedence boundary.
    Ok((command, parsed.provided_flags.into_iter().collect()))
}

pub fn apply_cli_flags() -> Result<AppliedCliFlags, CliError> {
    apply_cli_flags_from(
        std::env::args().collect(),
        std::env::vars().collect(),
        Path::new(".cli-flags.toml"),
    )
}

pub fn apply_cli_flags_from(
    argv: Vec<String>,
    ambient: EnvMap,
    config_path: &Path,
) -> Result<AppliedCliFlags, CliError> {
    let (command, argv_overrides) = parse_cli_flags(&argv, config_path)?;
    let merged = merge_env(ambient.clone(), argv_overrides.clone());
    Ok(AppliedCliFlags {
        command,
        ambient,
        argv_overrides,
        merged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_map::value;

    fn config_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".cli-flags.toml")
    }

    #[test]
    fn health_command_merges_without_mutating_process_environment() {
        let before = std::env::var_os("ENV_MAP_PROBE");
        let applied = apply_cli_flags_from(
            vec!["cli".into(), "health".into()],
            EnvMap::from([("ENV_MAP_PROBE".into(), "keep".into())]),
            &config_path(),
        )
        .expect("valid flags");
        assert_eq!(applied.command, Command::Health);
        assert_eq!(value(&applied.merged, "ENV_MAP_PROBE"), Some("keep"));
        assert_eq!(std::env::var_os("ENV_MAP_PROBE"), before);
    }

    #[test]
    fn ambient_env_beats_toml_default_when_argv_is_silent() {
        let applied = apply_cli_flags_from(
            vec!["cli".into(), "health".into()],
            EnvMap::from([(
                "FANWAAVE_API_BASE".into(),
                "https://ambient.example".into(),
            )]),
            &config_path(),
        )
        .expect("valid flags");
        assert!(applied.argv_overrides.get("FANWAAVE_API_BASE").is_none());
        assert_eq!(
            value(&applied.merged, "FANWAAVE_API_BASE"),
            Some("https://ambient.example")
        );
    }

    #[test]
    fn explicit_argv_beats_ambient_env() {
        let applied = apply_cli_flags_from(
            vec![
                "cli".into(),
                "health".into(),
                "--api-base=https://argv.example".into(),
            ],
            EnvMap::from([(
                "FANWAAVE_API_BASE".into(),
                "https://ambient.example".into(),
            )]),
            &config_path(),
        )
        .expect("valid flags");
        assert_eq!(
            value(&applied.argv_overrides, "FANWAAVE_API_BASE"),
            Some("https://argv.example")
        );
        assert_eq!(
            value(&applied.merged, "FANWAAVE_API_BASE"),
            Some("https://argv.example")
        );
    }

    #[test]
    fn parse_failure_does_not_mutate_process_environment() {
        let before = std::env::var_os("ENV_MAP_PROBE");
        assert!(apply_cli_flags_from(
            vec![
                "cli".into(),
                "health".into(),
                "--this-flag-is-not-declared".into()
            ],
            EnvMap::from([("ENV_MAP_PROBE".into(), "keep".into())]),
            &config_path(),
        )
        .is_err());
        assert_eq!(std::env::var_os("ENV_MAP_PROBE"), before);
    }

    #[test]
    fn source_does_not_mutate_process_environment() {
        const SRC: &str = include_str!("flags.rs");
        let production = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        assert!(!production.contains("std::env::set_var"));
        assert!(!production.contains("env::set_var"));
    }
}
