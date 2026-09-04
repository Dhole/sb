use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

mod bwrap;

use bwrap::{Options, run};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[derive(Debug, Parser)] // requires `derive` feature
#[command(name = "sb")]
#[command(about = "Sandbox environment", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Setup env
    #[command(arg_required_else_help = true)]
    Setup { name: String },
}

fn cli_parse() -> Result<Cli, clap::Error> {
    let args = Cli::try_parse()?;
    Ok(args)
}

#[derive(Serialize, Deserialize)]
struct EnvConfig {
    tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(deserialize_with = "deserialize_path")]
    envs_dir: PathBuf,
    envs: HashMap<String, EnvConfig>,
}

pub fn deserialize_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PathBuf::from(shellexpand::tilde(&s).to_string()))
}

impl Default for Config {
    fn default() -> Self {
        Config {
            envs_dir: PathBuf::from("~/sb/envs"),
            envs: HashMap::new(),
        }
    }
}

impl Config {
    fn load(path: &Path) -> Config {
        let config_toml = fs::read_to_string(path).unwrap();
        toml::from_str(&config_toml).unwrap()
    }
}

#[derive(Serialize)]
struct ChezmoiConfigData {
    tags: Vec<String>,
}

#[derive(Serialize)]
struct ChezmoiConfig {
    data: ChezmoiConfigData,
}

fn main() {
    let args = cli_parse().unwrap_or_else(|error| error.exit());

    let xdg_dirs = xdg::BaseDirectories::with_prefix("sb");
    let config_dir = xdg_dirs.get_config_home().expect("home dir found");
    fs::create_dir_all(&config_dir).unwrap();
    let config_envs_dir = config_dir.join("envs");
    fs::create_dir_all(&config_envs_dir).unwrap();

    let config_file = config_dir.join("config.toml");
    if !fs::exists(&config_file).unwrap() {
        let config = Config::default();
        let config_toml = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_file, config_toml).unwrap();
    }
    let config = Config::load(&config_file);

    match args.command {
        Commands::Setup { name } => {
            let env_config = &config.envs[&name];
            let env_dir = PathBuf::from(&config.envs_dir).join(&name);
            fs::create_dir_all(&env_dir).unwrap();

            let chezmoi_config_dir = tempdir().unwrap();
            let chezmoi_config_path = chezmoi_config_dir.path().to_path_buf().join("chezmoi.toml");
            let chezmoi_config = ChezmoiConfig {
                data: ChezmoiConfigData {
                    tags: env_config.tags.clone(),
                },
            };
            let chezmoi_config_toml = toml::to_string_pretty(&chezmoi_config).unwrap();
            fs::write(chezmoi_config_path, &chezmoi_config_toml).unwrap();
            run(Options::new(name)
                .src_home(env_dir.to_string_lossy().to_string())
                .user(env::var("USER").unwrap())
                .cmd(["chezmoi", "apply"])
                .bind(
                    chezmoi_config_dir.path().to_string_lossy(),
                    "~/.config/chezmoi/",
                )
                .ro_bind("~/.local/share/chezmoi/", "~/.local/share/chezmoi/"))
        }
    }
}
