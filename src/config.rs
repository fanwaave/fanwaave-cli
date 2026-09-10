#![forbid(unsafe_code)]

#[path = "../generated/rust/env.rs"]
mod env;

use crate::env_map::{truthy, EnvMap};
use crate::error::CliError;
use fanwaave_lib_core::fanwaave_config::{ConfigValue, ResolvedFanwaaveConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub api_base: String,
    pub json: bool,
}

impl Config {
    pub fn from_sources(
        env_map: &EnvMap,
        fanwaave: &ResolvedFanwaaveConfig,
    ) -> Result<Self, CliError> {
        let api_base = match fanwaave.binding("api_base_url").map(|binding| binding.value()) {
            Some(ConfigValue::Url(value)) => value.clone(),
            Some(_) => {
                return Err(CliError::Config(
                    "Fanwaave api_base_url binding did not resolve as a URL".into(),
                ))
            }
            None => {
                return Err(CliError::Config(
                    "Fanwaave api_base_url binding is unresolved".into(),
                ))
            }
        };
        if api_base.trim().is_empty() {
            return Err(CliError::Config("API base is empty".into()));
        }
        Ok(Self {
            api_base,
            json: truthy(env_map, env::JSON),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fanwaave_lib_core::fanwaave_config::{parse_fanwaave_config, resolve_fanwaave_config};
    use std::collections::BTreeMap;

    const CFG: &str = include_str!("../.fanwaave-cfg.toml");

    #[test]
    fn domain_default_supplies_api_base_when_env_and_argv_are_silent() {
        let fanwaave = parse_fanwaave_config(CFG).expect("tracked config parses");
        let resolved = resolve_fanwaave_config(&fanwaave, &BTreeMap::new(), &BTreeMap::new())
            .expect("tracked config resolves");
        let config = Config::from_sources(&BTreeMap::new(), &resolved).expect("CLI config resolves");
        assert_eq!(config.api_base, "http://127.0.0.1:8080");
        assert!(!config.json);
    }

    #[test]
    fn resolved_fanwaave_value_and_cli_json_flag_compose() {
        let fanwaave = parse_fanwaave_config(CFG).expect("tracked config parses");
        let ambient = BTreeMap::from([(
            "FANWAAVE_API_BASE".to_owned(),
            "https://ambient.example".to_owned(),
        )]);
        let argv = BTreeMap::from([(
            "FANWAAVE_API_BASE".to_owned(),
            "https://argv.example".to_owned(),
        )]);
        let resolved = resolve_fanwaave_config(&fanwaave, &ambient, &argv)
            .expect("tracked config resolves");
        let merged = BTreeMap::from([
            ("FANWAAVE_API_BASE".to_owned(), "https://argv.example".to_owned()),
            ("FANWAAVE_JSON".to_owned(), "true".to_owned()),
        ]);
        let config = Config::from_sources(&merged, &resolved).expect("CLI config resolves");
        assert_eq!(config.api_base, "https://argv.example");
        assert!(config.json);
    }
}
