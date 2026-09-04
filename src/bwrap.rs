use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

const ENV_VARS: &'static [&'static str] = &[
    "EDITOR",
    "HOME",
    "INFOPATH",
    "LD_LIBRARY_PATH",
    "LIBEXEC_PATH",
    "PAGER",
    "PATH",
    "PKG_CONFIG_PATH",
    "PWD",
    "SHELL",
    "TERM",
    "TERMINFO",
    "TERMINFO_DIRS",
    "XDG_BIN_HOME",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_DIRS",
    "XDG_CONFIG_HOME",
    "XDG_CURRENT_DESKTOP",
    "XDG_DATA_DIRS",
    "XDG_DATA_HOME",
    "XDG_DESKTOP_PORTAL_DIR",
    "XDG_RUNTIME_DIR",
    "XDG_SEAT",
    "XDG_SESSION_CLASS",
    "XDG_SESSION_ID",
    "XDG_SESSION_TYPE",
    "XDG_VTNR",
    "NIXPKGS_ALLOW_UNFREE",
    "NIXPKGS_CONFIG",
    "NIX_PATH",
    "NIX_PROFILES",
    "NIX_USER_PROFILE_DIR",
    "LANG",
    "LC_ADDRESS",
    "LC_IDENTIFICATION",
    "LC_MEASUREMENT",
    "LC_MONETARY",
    "LC_NAME",
    "LC_NUMERIC",
    "LC_PAPER",
    "LC_TELEPHONE",
    "LC_TIME",
    "LOCALE_ARCHIVE",
    "LOCALE_ARCHIVE_2_27",
    "TZDIR",
];

const ENV_VARS_DISPLAY: &'static [&'static str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DESKTOP_STARTUP_ID",
    "BROWSER",
    "GDK_BACKEND",
    "GTK2_RC_FILES",
    "GTK_A11Y",
    "GTK_PATH",
    "MOZ_ENABLE_WAYLAND",
    "NIXOS_OZONE_WL",
    "QT_QPA_PLATFORM",
    "QT_WAYLAND_DISABLE_WINDOWDECORATION",
    "QT_WAYLAND_FORCE_DPI",
    "SDL_VIDEODRIVER",
    "VDPAU_DRIVER",
    "WLR_LIBINPUT_NO_DEVICES",
    "XCURSOR_PATH",
    "XCURSOR_SIZE",
    "XCURSOR_THEME",
];

const PATHS_GENERAL: &'static [&'static str] = &[
    "/nix",
    "/etc/nix",
    "/etc/static/nix",
    "/bin",
    "/usr",
    "/lib",
    "/lib64",
    "/etc/alternatives",
    "/etc/man_db.conf",
    "/etc/localtime",
    "/etc/passwd",
    "/etc/crypto-policies",
    "/sys",
];

const PATHS_DISPLAY: &'static [&'static str] = &[
    "/tmp/.X11-unix",
    "~/.Xauthority",
    "/run/opengl-driver",
    "/etc/fonts",
];

const PATHS_NET: &'static [&'static str] = &[
    "/run/systemd/resolve",
    "/etc/resolv.conf",
    "/etc/ssl",
    "/etc/pki",
];

#[derive(Default)]
pub struct Options {
    pub name: String,
    pub src_home: String,
    pub user: String,
    pub net: bool,
    pub display: bool,
    pub cmd: Vec<String>,
    pub bind: Vec<(String, String)>,
    pub ro_bind: Vec<(String, String)>,
    pub dev_bind: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
}

impl Options {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    pub fn src_home(mut self, src_home: impl Into<String>) -> Self {
        self.src_home = src_home.into();
        self
    }
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = user.into();
        self
    }
    pub fn cmd<I, S>(mut self, cmd: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.cmd = cmd.into_iter().map(|s| s.into()).collect();
        self
    }
    pub fn ro_bind(mut self, src: impl Into<String>, dst: impl Into<String>) -> Self {
        self.ro_bind.push((src.into(), dst.into()));
        self
    }
    pub fn bind(mut self, src: impl Into<String>, dst: impl Into<String>) -> Self {
        self.bind.push((src.into(), dst.into()));
        self
    }
    pub fn dev_bind(mut self, src: impl Into<String>, dst: impl Into<String>) -> Self {
        self.dev_bind.push((src.into(), dst.into()));
        self
    }
    pub fn env(mut self, var: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((var.into(), value.into()));
        self
    }
}

pub fn run(mut opts: Options) {
    let mut cmd = Command::new("bwrap");
    cmd.arg("--new-session");
    cmd.arg("--clearenv");
    cmd.args(["--dev", "/dev"]);
    cmd.args(["--proc", "/proc"]);
    cmd.args(["--tmpfs", "/tmp"]);
    let home = format!("/home/{}", opts.user);
    cmd.args(["--dir", &home]);
    cmd.args(["--hostname", &opts.name]);
    cmd.arg("--unshare-all");
    cmd.args(["--bind", &opts.src_home, &home]);

    for p in PATHS_GENERAL {
        opts = opts.ro_bind(*p, *p);
    }
    for e in ENV_VARS {
        if let Ok(value) = env::var(e) {
            opts = opts.env(*e, value);
        }
    }
    if opts.net {
        cmd.arg("--share-net");
        for p in PATHS_NET {
            opts = opts.ro_bind(*p, *p);
        }
    }
    if opts.display {
        for e in ENV_VARS_DISPLAY {
            if let Ok(value) = env::var(e) {
                opts = opts.env(*e, value);
            }
        }
        for p in PATHS_DISPLAY {
            opts = opts.ro_bind(*p, *p);
        }
        opts = opts.dev_bind("/dev/dri", "/dev/dri");
        if let (Ok(wayland_display), Ok(xdg_runtime_dir)) =
            (env::var("WAYLAND_DISPLAY"), env::var("XDG_RUNTIME_DIR"))
        {
            let wayland_socket = format!("{xdg_runtime_dir}/{wayland_display}");
            opts = opts.bind(&wayland_socket, &wayland_socket);
        }
    }

    for (src, dst) in opts.bind {
        let (src, dst) = (
            shellexpand::tilde(&src).to_string(),
            shellexpand::tilde(&dst).to_string(),
        );
        if matches!(fs::exists(&src), Ok(true)) {
            cmd.args(["--bind", &src, &dst]);
        }
    }
    for (src, dst) in opts.ro_bind {
        let (src, dst) = (
            shellexpand::tilde(&src).to_string(),
            shellexpand::tilde(&dst).to_string(),
        );
        if matches!(fs::exists(&src), Ok(true)) {
            cmd.args(["--ro-bind", &src, &dst]);
        }
    }
    for (src, dst) in opts.dev_bind {
        let (src, dst) = (
            shellexpand::tilde(&src).to_string(),
            shellexpand::tilde(&dst).to_string(),
        );
        if matches!(fs::exists(&src), Ok(true)) {
            cmd.args(["--dev-bind", &src, &dst]);
        }
    }
    for (var, value) in opts.env {
        cmd.args(["--setenv", &var, &value]);
    }
    let output = cmd.args(&opts.cmd).output().unwrap();
    println!("status: {}", output.status);
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
}
