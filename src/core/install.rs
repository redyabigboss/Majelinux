use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::core::font;
use crate::core::settings::Settings;
use crate::core::util::which;
use crate::core::{Error, Result, log, net, paths, process, proton, sha256, vdf};
use crate::i18n::{t, tf};

pub const STEP_KEYS: [&str; 7] = [
    "step.proton",
    "step.compat",
    "step.powershell",
    "step.launcher",
    "step.handler",
    "step.emoji",
    "step.ru_fix",
];

pub const OPTIONAL_STEP: usize = 5;

const STEAM_BAT: &str = "@echo off\r\n\
set \"GAMEARGS=\"\r\n\
for /f \"tokens=4 delims=/\" %%a in (\"%~1\") do set \"GAMEARGS=%%a\"\r\n\
cd /d \"S:\\steamapps\\common\\Grand Theft Auto V\"\r\n\
start \"\" PlayGTAV.exe %GAMEARGS%\r\n";
const STEAM_CMD: &str = r#"C:\windows\system32\cmd.exe /c C:\majestic\steam-run.bat "%1""#;
const NOLCALL_C: &str = r#"
#include <stdlib.h>
#include <string.h>
__attribute__((constructor))
static void nolcall_init(void) {
    const char *v = getenv("LC_ALL");
    if (v && strcmp(v, "C") == 0)
        unsetenv("LC_ALL");
}
"#;

#[derive(Clone, Debug)]
pub enum Event {
    Step(usize, &'static str),
    Packages(Vec<String>, String, String),
    Status(&'static str),
}

impl Event {
    pub fn status_running() -> Self {
        Event::Status("install.status.running")
    }
}

pub fn target(settings: &Settings) -> proton::Target {
    proton::target_for(&settings.proton)
}

pub fn active_proton_dir(settings: &Settings) -> PathBuf {
    proton_dir(settings)
}

fn proton_dir(settings: &Settings) -> PathBuf {
    target(settings).source
}

fn proton_bin(settings: &Settings) -> Result<PathBuf> {
    let dir = proton_dir(settings);
    let bin = dir.join("proton");
    if !bin.is_file() {
        return Err(Error(tf(
            "err.proton_missing",
            &[(
                "name",
                &dir.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )],
        )));
    }
    Ok(bin)
}

fn proton_run(settings: &Settings, args: &[&str]) -> Result<i32> {
    let bin = proton_bin(settings)?;
    let mut full = Vec::with_capacity(args.len() + 1);
    full.push("run".to_string());
    full.extend(args.iter().map(|arg| (*arg).to_string()));
    process::run_streaming(&bin, &full, &process::proton_env(), None, true)
}

fn prefix_ready(root: &Path) -> bool {
    let marker = proton::runtime_dir(root).join("share/default_pfx");
    let marker = marker.to_string_lossy();
    let marker = marker.trim_end_matches('/');
    let Ok(info) = fs::read_to_string(paths::compat().join("config_info")) else {
        return false;
    };
    info.lines()
        .any(|line| line.trim_end_matches('/') == marker)
        && paths::pfx()
            .join("drive_c/windows/system32/kernel32.dll")
            .is_file()
}

pub fn prepare_prefix(settings: &Settings) -> Result<()> {
    if prefix_ready(&proton_dir(settings)) {
        return Ok(());
    }
    log::line(&t("log.prefix_prepare"));
    let _ = proton_run(settings, &["cmd", "/c", "exit"])?;
    Ok(())
}

pub fn detect_package_manager() -> Option<&'static str> {
    ["pacman", "apt", "dnf", "zypper", "xbps-install"]
        .into_iter()
        .find(|name| which(name).is_some())
}

fn manager_prefix(manager: &str) -> Vec<&'static str> {
    match manager {
        "pacman" => vec!["pacman", "-S", "--needed"],
        "apt" => vec!["apt", "install"],
        "dnf" => vec!["dnf", "install"],
        "zypper" => vec!["zypper", "install"],
        "xbps-install" => vec!["xbps-install", "-S"],
        _ => Vec::new(),
    }
}

pub fn install_command(packages: &[String]) -> Option<String> {
    let manager = detect_package_manager()?;
    let mut parts: Vec<String> = Vec::new();

    if unsafe { crate::ffi::geteuid() } != 0 {
        parts.push("sudo".to_string());
    }
    parts.extend(manager_prefix(manager).into_iter().map(str::to_string));
    parts.extend(packages.iter().cloned());
    Some(parts.join(" "))
}

pub fn missing_packages() -> Vec<String> {
    let mut missing = Vec::new();
    if which("protontricks").is_none() {
        missing.push("protontricks".to_string());
    }
    if !paths::shim_so().is_file() && which("gcc").is_none() {
        missing.push("gcc".to_string());
    }
    missing
}

fn wait_for_packages<F: FnMut(Event)>(mut emit: F) -> Result<()> {
    let mut shown: Vec<String> = Vec::new();
    loop {
        let missing = missing_packages();
        if missing.is_empty() {
            if !shown.is_empty() {
                emit(Event::Packages(Vec::new(), String::new(), String::new()));
                log::line(&t("gui.packages_ok"));
            }
            return Ok(());
        }
        if missing != shown {
            shown = missing.clone();
            let command = install_command(&missing).unwrap_or_default();
            let manager = detect_package_manager().unwrap_or("").to_string();
            emit(Event::Packages(missing.clone(), command.clone(), manager));
            log::line(&tf(
                "gui.waiting_packages",
                &[("pkgs", &missing.join(", "))],
            ));
            if !command.is_empty() {
                log::line(&format!("$ {command}"));
            }
        }
        for _ in 0..20 {
            if process::cancelled() {
                emit(Event::Packages(Vec::new(), String::new(), String::new()));
                return Err(Error(t("err.cancelled")));
            }
            process::sleep_ms(100);
        }
    }
}

enum LauncherCache {
    Empty,
    Missing(Instant),
    Found(PathBuf, Instant),
}

static LAUNCHER: Mutex<LauncherCache> = Mutex::new(LauncherCache::Empty);

fn scan_launcher() -> Option<PathBuf> {
    let base = paths::launcher_base();
    if !base.is_dir() {
        return None;
    }

    const WANTED: &str = "Majestic Launcher.exe";
    let mut pending = BinaryHeap::new();
    pending.push(Reverse(base));

    while let Some(Reverse(path)) = pending.pop() {
        if path.file_name().and_then(|name| name.to_str()) == Some(WANTED) && path.is_file() {
            return Some(path);
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() && !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            if file_type.is_dir() || name.to_str() == Some(WANTED) {
                pending.push(Reverse(path.join(name)));
            }
        }
    }
    None
}

pub fn invalidate_launcher_cache() {
    if let Ok(mut guard) = LAUNCHER.lock() {
        *guard = LauncherCache::Empty;
    }
}

pub fn find_launcher() -> Option<PathBuf> {
    if let Ok(guard) = LAUNCHER.lock() {
        match &*guard {
            LauncherCache::Found(path, checked)
                if checked.elapsed().as_secs() < 1 && path.is_file() =>
            {
                return Some(path.clone());
            }
            LauncherCache::Missing(checked) if checked.elapsed().as_secs() < 1 => return None,
            LauncherCache::Empty | LauncherCache::Missing(_) | LauncherCache::Found(_, _) => {}
        }
    }
    let found = scan_launcher();
    if let Ok(mut guard) = LAUNCHER.lock() {
        *guard = match &found {
            Some(path) => LauncherCache::Found(path.clone(), Instant::now()),
            None => LauncherCache::Missing(Instant::now()),
        };
    }
    found
}

fn is_proton_patched(target: &proton::Target) -> bool {
    proton::is_patched(&target.patched)
}

fn is_compat_tool_set(target: &proton::Target) -> bool {
    let mapped = vdf::compat_tool(paths::APP_ID)
        .ok()
        .flatten()
        .map(|name| name == target.name)
        .unwrap_or(false);
    mapped || prefix_ready(&target.source)
}

pub fn apply_compat_tool(name: &str) -> Result<()> {
    if vdf::compat_tool(paths::APP_ID).ok().flatten().as_deref() == Some(name) {
        return Ok(());
    }
    let was_running = process::steam_running();
    if was_running {
        log::line(&t("gui.steam_closing"));
        if !process::steam_shutdown() {
            return Err(Error(t("err.steam_close")));
        }
    }
    let result = vdf::set_compat_tool(paths::APP_ID, name);
    if was_running {
        process::sleep_ms(1500);
        log::line(&t("gui.steam_starting"));
        process::steam_start();
    }
    result?;
    log::line(&tf("gui.compat_set", &[("name", name)]));
    Ok(())
}

fn set_compat_tool(target: &proton::Target) -> Result<()> {
    apply_compat_tool(&target.name)
}

fn is_powershell_installed() -> bool {
    let Ok(file) = File::open(paths::winetricks_log()) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut line = Vec::with_capacity(64);
    let mut found = false;
    let mut valid_utf8 = true;
    loop {
        line.clear();
        let read = match reader.read_until(b'\n', &mut line) {
            Ok(read) => read,
            Err(_) => return false,
        };
        if read == 0 {
            break;
        }
        match std::str::from_utf8(&line) {
            Ok(text) => found |= text.trim() == "powershell",
            Err(_) => valid_utf8 = false,
        }
    }
    valid_utf8 && found
}

fn install_powershell() -> Result<()> {
    process::run_streaming(
        Path::new("protontricks"),
        &[paths::APP_ID_STR.to_string(), "powershell".to_string()],
        &[],
        None,
        true,
    )?;
    if !is_powershell_installed() {
        return Err(Error(t("err.powershell")));
    }
    Ok(())
}

fn is_launcher_installed() -> bool {
    find_launcher().is_some()
}

fn install_launcher() -> Result<()> {
    let installer = paths::cache().join("MajesticLauncherSetup.exe");
    net::download(paths::SETUP_URL, &installer)?;
    process::run_streaming(
        Path::new("protontricks-launch"),
        &[
            "--appid".to_string(),
            paths::APP_ID_STR.to_string(),
            installer.to_string_lossy().into_owned(),
        ],
        &process::proton_env(),
        None,
        true,
    )?;
    log::line(&t("log.wait_install"));
    for _ in 0..50 {
        if process::cancelled() {
            break;
        }
        process::sleep_ms(100);
    }
    invalidate_launcher_cache();
    if !is_launcher_installed() {
        return Err(Error(t("err.launcher_after_install")));
    }
    Ok(())
}

type RegCache = Option<(PathBuf, u64, u64, bool, bool)>;

static REG_CACHE: Mutex<RegCache> = Mutex::new(None);

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

fn read_reg_flags(path: &Path) -> io::Result<(bool, bool)> {
    const CHUNK: usize = 64 * 1024;
    const HANDLER: &[u8] = b"majestic\\\\steam-run.bat";
    const EMOJI: &[u8] = b"seguiemj.ttf";
    let overlap = HANDLER.len().max(EMOJI.len()).saturating_sub(1);
    let mut storage = vec![0u8; CHUNK + overlap];
    let mut carry = 0usize;
    let mut handler = false;
    let mut emoji = false;
    let mut file = File::open(path)?;

    loop {
        let read = file.read(&mut storage[carry..])?;
        if read == 0 {
            break;
        }
        let end = carry + read;
        let chunk = &storage[..end];
        if !handler {
            handler = contains_ascii_case_insensitive(chunk, HANDLER);
        }
        if !emoji {
            emoji = contains_ascii_case_insensitive(chunk, EMOJI);
        }
        if handler && emoji {
            break;
        }

        carry = end.min(overlap);
        storage.copy_within(end - carry..end, 0);
    }
    Ok((handler, emoji))
}

fn reg_flags() -> (bool, bool) {
    let path = paths::pfx().join("system.reg");
    let Some((mtime, len)) = proton::file_stamp(&path) else {
        return (false, false);
    };
    if let Ok(cache) = REG_CACHE.lock()
        && let Some((cached_path, cached_mtime, cached_len, handler, emoji)) = cache.as_ref()
        && *cached_path == path
        && *cached_mtime == mtime
        && *cached_len == len
    {
        return (*handler, *emoji);
    }
    let Ok(flags) = read_reg_flags(&path) else {
        return (false, false);
    };
    if let Ok(mut cache) = REG_CACHE.lock() {
        *cache = Some((path, mtime, len, flags.0, flags.1));
    }
    flags
}

fn is_steam_handler_patched() -> bool {
    paths::steam_bat().is_file() && reg_flags().0
}

fn patch_steam_handler(settings: &Settings) -> Result<()> {
    process::kill_launcher();
    if let Some(parent) = paths::steam_bat().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(paths::steam_bat(), STEAM_BAT.as_bytes())?;
    let _ = fs::remove_file(paths::old_steam_bat());
    let base = r"HKLM\Software\Classes\steam";
    proton_run(
        settings,
        &[
            "reg",
            "add",
            base,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            "URL:steam protocol",
            "/f",
        ],
    )?;
    proton_run(
        settings,
        &[
            "reg",
            "add",
            base,
            "/v",
            "URL Protocol",
            "/t",
            "REG_SZ",
            "/d",
            "",
            "/f",
        ],
    )?;
    let command = format!(r"{base}\shell\open\command");
    proton_run(
        settings,
        &[
            "reg", "add", &command, "/ve", "/t", "REG_SZ", "/d", STEAM_CMD, "/f",
        ],
    )?;
    process::flush_registry(&proton_dir(settings));
    if !process::wait_until(is_steam_handler_patched, 10.0) {
        return Err(Error(t("err.steam_handler")));
    }
    Ok(())
}

fn is_shim_built() -> bool {
    paths::shim_so().is_file()
}

fn build_shim() -> Result<()> {
    fs::create_dir_all(paths::state())?;
    fs::write(paths::shim_c(), NOLCALL_C.as_bytes())?;
    process::run_streaming(
        Path::new("gcc"),
        &[
            "-shared".to_string(),
            "-fPIC".to_string(),
            "-O2".to_string(),
            "-o".to_string(),
            paths::shim_so().to_string_lossy().into_owned(),
            paths::shim_c().to_string_lossy().into_owned(),
        ],
        &[],
        None,
        true,
    )?;
    Ok(())
}

fn is_emoji_installed() -> bool {
    let emoji = paths::emoji_ttf();
    emoji.is_file()
        && fs::metadata(&emoji)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
        && reg_flags().1
}

fn emoji_source() -> Result<PathBuf> {
    let cached = paths::cache().join("Twemoji.Mozilla.ttf");
    if cached.is_file() && sha256::file_hex(&cached).ok().as_deref() == Some(paths::EMOJI_SHA256) {
        return Ok(cached);
    }
    let _ = fs::remove_file(&cached);
    net::download(paths::EMOJI_URL, &cached)?;
    if sha256::file_hex(&cached).ok().as_deref() != Some(paths::EMOJI_SHA256) {
        let _ = fs::remove_file(&cached);
        return Err(Error(t("err.emoji_checksum")));
    }
    Ok(cached)
}

fn install_emoji(settings: &Settings) -> Result<()> {
    let source = emoji_source()?;
    process::kill_launcher();
    let emoji = paths::emoji_ttf();
    if let Some(parent) = emoji.parent() {
        fs::create_dir_all(parent)?;
    }
    font::rename_font(&source, &emoji)
        .map_err(|error| Error(tf("err.font_rename", &[("err", &error.to_string())])))?;
    let value = format!("{} (TrueType)", paths::EMOJI_FAMILY);
    proton_run(
        settings,
        &[
            "reg",
            "add",
            r"HKLM\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
            "/v",
            &value,
            "/t",
            "REG_SZ",
            "/d",
            "seguiemj.ttf",
            "/f",
        ],
    )?;
    process::flush_registry(&proton_dir(settings));
    if !process::wait_until(is_emoji_installed, 10.0) {
        return Err(Error(t("err.emoji_registry")));
    }
    Ok(())
}

fn step_done_for(target: &proton::Target, index: usize) -> bool {
    match index {
        0 => is_proton_patched(target),
        1 => is_compat_tool_set(target),
        2 => is_powershell_installed(),
        3 => is_launcher_installed(),
        4 => is_steam_handler_patched(),
        5 => is_emoji_installed(),
        6 => is_shim_built(),
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct InstallationStatus {
    pub steps: [bool; STEP_KEYS.len()],
    pub installed: bool,
    pub proton_display: String,
}

pub fn installation_status(settings: &Settings) -> InstallationStatus {
    let target = target(settings);
    let packages_ready = missing_packages().is_empty();
    let steps: [bool; STEP_KEYS.len()] = std::array::from_fn(|index| step_done_for(&target, index));
    let installed = packages_ready
        && steps
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != OPTIONAL_STEP)
            .all(|(_, done)| *done);
    InstallationStatus {
        steps,
        installed,
        proton_display: target.display.replace(paths::DISPLAY_SUFFIX, ""),
    }
}

fn run_step(settings: &Settings, target: &proton::Target, index: usize) -> Result<()> {
    match index {
        0 => proton::ensure_patched(target),
        1 => set_compat_tool(target),
        2 => install_powershell(),
        3 => install_launcher(),
        4 => patch_steam_handler(settings),
        5 => install_emoji(settings),
        6 => build_shim(),
        _ => Ok(()),
    }
}

pub fn install<F: FnMut(Event)>(settings: &Settings, mut emit: F) -> Result<()> {
    process::clear_cancel();
    emit(Event::status_running());
    if !paths::gta_dir().is_dir() {
        return Err(Error(t("err.gta_missing")));
    }
    if !paths::pfx().is_dir() {
        return Err(Error(t("err.no_prefix")));
    }
    wait_for_packages(&mut emit)?;
    let selected_target = target(settings);
    for (index, key) in STEP_KEYS.iter().enumerate() {
        if process::cancelled() {
            return Err(Error(t("err.cancelled")));
        }
        if step_done_for(&selected_target, index) {
            emit(Event::Step(index, "done"));
            log::line(&tf("gui.step_done", &[("title", &t(key))]));
            continue;
        }
        emit(Event::Step(index, "running"));
        log::line(&tf("gui.step_run", &[("title", &t(key))]));
        match run_step(settings, &selected_target, index) {
            Ok(()) => emit(Event::Step(index, "done")),
            Err(error) => {
                if index != OPTIONAL_STEP {
                    emit(Event::Step(index, "error"));
                    return Err(error);
                }
                emit(Event::Step(index, "warn"));
                log::line(&tf(
                    "gui.step_warn",
                    &[("title", &t(key)), ("err", &error.to_string())],
                ));
            }
        }
    }
    log::line(&t("gui.install_done"));
    emit(Event::Status("install.status.done"));
    Ok(())
}

pub fn open_logs_dir() -> Result<()> {
    fs::create_dir_all(paths::logs_dir())?;
    process::spawn_detached(
        Path::new("xdg-open"),
        &[paths::logs_dir().to_string_lossy().into_owned()],
        &[],
        None,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_source_is_stable() {
        assert!(NOLCALL_C.starts_with('\n'));
        assert!(NOLCALL_C.contains("unsetenv(\"LC_ALL\")"));
        assert!(NOLCALL_C.ends_with('\n'));
    }

    #[test]
    fn steam_batch_uses_crlf_and_exact_arguments() {
        assert!(STEAM_BAT.starts_with("@echo off\r\n"));
        assert!(STEAM_BAT.ends_with("%GAMEARGS%\r\n"));
        assert!(STEAM_BAT.contains("tokens=4 delims=/"));
        assert_eq!(
            STEAM_CMD,
            "C:\\windows\\system32\\cmd.exe /c C:\\majestic\\steam-run.bat \"%1\""
        );
    }

    #[test]
    fn install_command_starts_with_manager() {
        if let Some(command) = install_command(&["protontricks".to_string()]) {
            let manager = detect_package_manager().unwrap();
            assert!(command.contains(manager));
            assert!(command.ends_with("protontricks"));
        }
    }
}
