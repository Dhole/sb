use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

mod bwrap;

use bwrap::{Options, run, exec};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

const DIR_PREFI: &'static str = "sb";

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
    #[command(arg_required_else_help = true)]
    Term { name: String },
}

fn cli_parse() -> Result<Cli, clap::Error> {
    let args = Cli::try_parse()?;
    Ok(args)
}

#[derive(Clone, Serialize, Deserialize)]
struct EnvConfig {
    user: String,
    id: usize,
    terminal: Vec<String>,
    shell: String,
    tags: Vec<String>,
    env_vars: HashMap<String, String>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            user: "user".to_string(),
            id: 1024,
            tags: Vec::new(),
            terminal: vec!["/usr/bin/xterm".to_string()],
            shell: "/bin/bash".to_string(),
            env_vars: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct EnvOptConfig {
    user: Option<String>,
    shell: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    env_vars: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(deserialize_with = "deserialize_path")]
    envs_dir: PathBuf,
    env_default: EnvConfig,
    envs: HashMap<String, EnvOptConfig>,
}

impl Config {
    fn env(&self, name: &str) -> EnvConfig {
        let env_config = &self.envs[name];
        let mut result = self.env_default.clone();
        result.user = env_config.user.clone().unwrap_or(result.user);
        result.shell = env_config.shell.clone().unwrap_or(result.shell);
        result.tags.extend_from_slice(&env_config.tags);
        result.env_vars.extend(env_config.env_vars.clone().into_iter());
        result
    }
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
            env_default: EnvConfig::default(),
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

fn env_dir(config: &Config, name: &str) -> PathBuf {
    PathBuf::from(&config.envs_dir).join(name)
}

fn cache_env_dir(name: &str) -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(DIR_PREFI);
    let cache_dir = xdg_dirs.get_cache_home().expect("home dir found");
    cache_dir.join("envs").join(name)
}

fn env_run_opts(config: &Config, name: &str, env_config: &EnvConfig) -> Options {
    let env_dir = env_dir(config, name);
    let mut opts = Options::new(name)
        .ro_bind(cache_env_dir(name).join("passwd").to_string_lossy(), "/etc/passwd")
        .ro_bind(cache_env_dir(name).join("group").to_string_lossy(), "/etc/group")
        .src_home(env_dir.to_string_lossy())
        .user(env_config.user.clone())
        .id(env_config.id)
        .env("HOME", format!("/home/{}", env_config.user))
        .env("SHELL", env_config.shell.clone());
    for (env_var, value) in &env_config.env_vars {
        opts = opts.env(env_var, value);
    }
    opts
}

fn gen_passwd(user: &str, id: usize, shell: &str) -> String {
    format!(
"root:*:0:0:root:/root:/bin/bash
{user}:x:{id}:{id}:{user}:/home/{user}:{shell}
")
}

fn gen_group(user: &str, id: usize) -> String {
    format!(
"root:x:0:
{user}:x:{id}:
")
}

fn prelude(name: &str, env_config: &EnvConfig) {
    let cache_env_dir = cache_env_dir(&name);
    fs::create_dir_all(&cache_env_dir).unwrap();
    fs::write(cache_env_dir.join("passwd"), gen_passwd(&env_config.user, env_config.id, &env_config.shell)).unwrap();
    fs::write(cache_env_dir.join("group"), gen_group(&env_config.user, env_config.id)).unwrap();
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
            let env_config = &config.env(&name);
            let env_dir = env_dir(&config, &name);
            fs::create_dir_all(&env_dir).unwrap();

            prelude(&name, &env_config);

            let chezmoi_config_dir = tempdir().unwrap();
            let chezmoi_config_path = chezmoi_config_dir.path().to_path_buf().join("chezmoi.toml");
            let chezmoi_config = ChezmoiConfig {
                data: ChezmoiConfigData {
                    tags: env_config.tags.clone(),
                },
            };
            let chezmoi_config_toml = toml::to_string_pretty(&chezmoi_config).unwrap();
            fs::write(chezmoi_config_path, &chezmoi_config_toml).unwrap();
            run(env_run_opts(&config, &name, &env_config)
                .cmd(["chezmoi", "--debug", "apply"])
                .bind(
                    chezmoi_config_dir.path().to_string_lossy(),
                    "~/.config/chezmoi/",
                )
                .ro_bind("~/.local/share/chezmoi/", "~/.local/share/chezmoi/"))
        }
        Commands::Term { name } => {
            let env_config = config.env(&name);
            prelude(&name, &env_config);
            let mut opts = env_run_opts(&config, &name, &env_config)
                .cmd(env_config.terminal);
            opts.display = true;
            exec(opts)
        }
    }
}
