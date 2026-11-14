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
    // flags-2-env may interpret an unknown leading bare argument as an
    // empty/help command. Reject it before structured flag parsing, without
    // including user-controlled input in the error.
    if let Some(first) = argv.get(1) {
        if !first.starts_with('-') && !matches!(first.as_str(), "health" | "status" | "help") {
            return Err(CliError::Usage("unknown command".into()));
        }
    }
    let config_path = config_path
        .to_str()
        .ok_or_else(|| CliError::Config(".cli-flags.toml path is not valid UTF-8".into()))?;
    let parser = BundledFlags2Env::new();
    parser.audit_config(Some(config_path)).map_err(|error| {
        CliError::Config(format!("flags-2-env configuration audit failed: {error}"))
    })?;
    let parsed = parser
        .parse_structured(argv, Some(config_path))
        // Parser errors may contain raw flag values (including credentials).
        .map_err(|_| CliError::Config("flags-2-env parse failed".into()))?;
    if !parsed.unknown_options.is_empty() {
        return Err(CliError::Usage("unknown command-line option".into()));
    }
    if !parsed.errors.is_empty() {
        return Err(CliError::Usage("invalid command-line value".into()));
    }
    let command = if parsed.command.is_empty() || parsed.command == "help" {
        Command::Help
    } else if parsed.command == "health" {
        Command::Health
    } else if parsed.command == "status" {
        Command::Status
    } else {
        return Err(CliError::Usage("unknown command".into()));
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
    fn health_command_merges_without_mutating_process_environment() -> Result<(), CliError> {
        let before = std::env::var_os("ENV_MAP_PROBE");
        let applied = apply_cli_flags_from(
            vec!["cli".into(), "health".into()],
            EnvMap::from([("ENV_MAP_PROBE".into(), "keep".into())]),
            &config_path(),
        )
        ?;
        assert_eq!(applied.command, Command::Health);
        assert_eq!(value(&applied.merged, "ENV_MAP_PROBE"), Some("keep"));
        assert_eq!(std::env::var_os("ENV_MAP_PROBE"), before);
        Ok(())
    }

    #[test]
    fn ambient_env_beats_toml_default_when_argv_is_silent() -> Result<(), CliError> {
        let applied = apply_cli_flags_from(
            vec!["cli".into(), "health".into()],
            EnvMap::from([(
                "FANWAAVE_API_BASE_URL".into(),
                "https://ambient.example".into(),
            )]),
            &config_path(),
        )
        ?;
        assert!(
            applied
                .argv_overrides
                .get("FANWAAVE_API_BASE_URL")
                .is_none()
        );
        assert_eq!(
            value(&applied.merged, "FANWAAVE_API_BASE_URL"),
            Some("https://ambient.example")
        );
        Ok(())
    }

    #[test]
    fn explicit_argv_beats_ambient_env() -> Result<(), CliError> {
        let applied = apply_cli_flags_from(
            vec![
                "cli".into(),
                "health".into(),
                "--api-base=https://argv.example".into(),
            ],
            EnvMap::from([(
                "FANWAAVE_API_BASE_URL".into(),
                "https://ambient.example".into(),
            )]),
            &config_path(),
        )
        ?;
        assert_eq!(
            value(&applied.argv_overrides, "FANWAAVE_API_BASE_URL"),
            Some("https://argv.example")
        );
        assert_eq!(
            value(&applied.merged, "FANWAAVE_API_BASE_URL"),
            Some("https://argv.example")
        );
        Ok(())
    }

    #[test]
    fn generated_env_contracts_track_cli_authority() {
        let artifacts = [
            include_str!("../generated/rust/env.rs"),
            include_str!("../generated/rust/runtime.rs"),
            include_str!("../generated/typescript/env.ts"),
            include_str!("../generated/typescript/runtime.ts"),
            include_str!("../generated/dart/env.dart"),
            include_str!("../generated/dart/runtime.dart"),
            include_str!("../generated/gleam/env.gleam"),
            include_str!("../generated/gleam/runtime.gleam"),
        ];
        for artifact in artifacts {
            assert!(artifact.contains("FANWAAVE_API_BASE_URL"));
            assert!(!artifact.contains("\"FANWAAVE_API_BASE\""));
            assert!(!artifact.contains("'FANWAAVE_API_BASE'"));
        }
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
    fn parser_errors_do_not_echo_secret_like_cli_values() {
        let sentinel = "DO_NOT_ECHO_TOKEN_SECRET";
        for argv in [
            vec!["cli".into(), "health".into(), format!("--unknown={sentinel}")],
            vec!["cli".into(), sentinel.into()],
        ] {
            let error = parse_cli_flags(&argv, &config_path()).expect_err("invalid invocation");
            assert!(!error.to_string().contains(sentinel));
        }
    }

    #[test]
    fn source_does_not_mutate_process_environment() {
        const SRC: &str = include_str!("flags.rs");
        let production = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        assert!(!production.contains("std::env::set_var"));
        assert!(!production.contains("env::set_var"));
    }
}
