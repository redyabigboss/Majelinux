use std::path::{Path, PathBuf};

use crate::core::steam as discovery;

pub const APP_ID: u32 = 271590;
pub const APP_ID_STR: &str = "271590";
pub const DEFAULT_ARGS: &str = "-disable-gpu";
pub const APP_NAME: &str = "Majelinux";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/redyabigboss/Majelinux";
pub const GAME_DIR: &str = "Grand Theft Auto V";
pub const STOCK_FOLDER: &str = "Proton - Experimental";

pub const PATCHED_NAME: &str = "Proton-Exp-Majelinux";
pub const LEGACY_PATCHED_NAME: &str = "Proton-Exp-MJLX";
pub const DISPLAY_SUFFIX: &str = " (Majelinux RCX fix)";
pub const DEFAULT_LABEL: &str = "Proton Experimental";

pub const SETUP_URL: &str = "https://cdn.majestic-files.net/launcher/cis/MajesticLauncherSetup.exe";
pub const EMOJI_URL: &str =
    "https://github.com/mozilla/twemoji-colr/releases/download/v0.7.0/Twemoji.Mozilla.ttf";
pub const EMOJI_SHA256: &str = "6d90152ee0d29e82fe2a87793af5aa4b7ad13e6538360889e141e81ed299ee8e";
pub const EMOJI_FAMILY: &str = "Segoe UI Emoji";

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(variable: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(variable) {
        Some(value) if !value.is_empty() && Path::new(&value).is_absolute() => PathBuf::from(value),
        _ => home().join(fallback),
    }
}

pub fn steam() -> PathBuf {
    discovery::root()
}

pub fn library() -> PathBuf {
    discovery::game_library()
}

pub fn common() -> PathBuf {
    library().join("steamapps/common")
}

pub fn gta_dir() -> PathBuf {
    common().join(GAME_DIR)
}

pub fn compat() -> PathBuf {
    library().join("steamapps/compatdata").join(APP_ID_STR)
}

pub fn pfx() -> PathBuf {
    compat().join("pfx")
}

pub fn state() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join(APP_NAME)
}

pub fn legacy_state() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("majesticrplinux")
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join(APP_NAME)
}

pub fn logs_dir() -> PathBuf {
    state().join("logs")
}

pub fn legacy_log() -> PathBuf {
    state().join("launcher.log")
}

pub fn update_state() -> PathBuf {
    state().join("applied_update.txt")
}

pub fn shim_so() -> PathBuf {
    state().join("nolcall.so")
}

pub fn shim_c() -> PathBuf {
    state().join("nolcall.c")
}

pub fn cache() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join(APP_NAME)
}

pub fn config_ini() -> PathBuf {
    config_dir().join("config.ini")
}

pub fn steam_bat() -> PathBuf {
    pfx().join("drive_c/majestic/steam-run.bat")
}

pub fn old_steam_bat() -> PathBuf {
    pfx().join("drive_c/steam-run.bat")
}

pub fn emoji_ttf() -> PathBuf {
    pfx().join("drive_c/windows/Fonts/seguiemj.ttf")
}

pub fn winetricks_log() -> PathBuf {
    pfx().join("winetricks.log")
}

pub fn config_vdf() -> PathBuf {
    steam().join("config/config.vdf")
}

pub fn backup_vdf() -> PathBuf {
    steam().join("config/config.vdf.Majelinux.bak")
}

pub fn launcher_base() -> PathBuf {
    pfx().join("drive_c/users/steamuser/AppData/Local")
}

pub fn migrate_state() {
    let legacy = legacy_state();
    if legacy.is_dir() && !state().exists() {
        let _ = std::fs::rename(&legacy, state());
    }
    let old_config = state().join("config.ini");
    if old_config.is_file() && !config_ini().exists() {
        let _ = std::fs::create_dir_all(config_dir());
        if std::fs::rename(&old_config, config_ini()).is_err()
            && std::fs::copy(&old_config, config_ini()).is_ok()
        {
            let _ = std::fs::remove_file(&old_config);
        }
    }
    let old_cache = state().join("cache");
    if old_cache.is_dir() {
        let _ = std::fs::remove_dir_all(&old_cache);
    }
}
