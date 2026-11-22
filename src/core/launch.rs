use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::core::settings::Settings;
use crate::core::util::shlex_split;
use crate::core::{
    Error, Result, diag, install, launchopts, log, paths, process, proton, trim, update,
};
use crate::i18n::{t, tf};

const UPDATE_WAIT: f64 = 45.0;
const LAUNCH_GRACE: f64 = 4.0;
const UPDATE_ARGS: [&str; 3] = ["--updated", "/S", "--force-run"];

fn launch_env(preload: bool) -> Vec<(String, String)> {
    let mut env = process::proton_env();
    env.push(("PROTON_ENABLE_WAYLAND".to_string(), "0".to_string()));
    if preload && paths::shim_so().is_file() {
        let shim = paths::shim_so().to_string_lossy().into_owned();
        let old = std::env::var("LD_PRELOAD").unwrap_or_default();
        let value = if old.is_empty() {
            shim
        } else {
            format!("{shim}:{old}")
        };
        env.push(("LD_PRELOAD".to_string(), value));
    }
    env
}

pub fn ensure_compat_tool(settings: &Settings) -> bool {
    let name = install::target(settings).name;
    match install::apply_compat_tool(&name) {
        Ok(()) => true,
        Err(error) => {
            log::line(&error.to_string());
            false
        }
    }
}

fn start_trim(settings: &Settings) {
    let mode = trim::Mode::parse(&settings.trim_mode);
    if mode == trim::Mode::Off {
        return;
    }
    let Ok(exe) = update::target() else {
        return;
    };
    let args = vec![
        trim::FLAG.to_string(),
        mode.as_str().to_string(),
        settings.trim_delay.to_string(),
    ];
    if let Err(error) = process::spawn_detached(&exe, &args, &[], None) {
        log::line(&error.to_string());
    }
}

fn ensure_steam_running() {
    if process::steam_running() {
        return;
    }
    log::line(&t("gui.steam_opening"));
    process::steam_start();
}

fn link_steam_drive() {
    let s_drive = paths::pfx().join("dosdevices/s:");
    let metadata = fs::symlink_metadata(&s_drive);
    match metadata {
        Ok(meta) if meta.is_dir() => {
            log::line(&tf(
                "gui.drive_s_fail",
                &[("err", "S: is a real directory")],
            ));
            return;
        }
        Ok(_) => {
            if let Err(error) = fs::remove_file(&s_drive) {
                log::line(&tf("gui.drive_s_fail", &[("err", &error.to_string())]));
                return;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            log::line(&tf("gui.drive_s_fail", &[("err", &error.to_string())]));
            return;
        }
    }
    if let Err(error) = std::os::unix::fs::symlink(paths::library(), &s_drive) {
        log::line(&tf("gui.drive_s_fail", &[("err", &error.to_string())]));
    }
}

pub fn launcher_cmd(settings: &Settings) -> Result<(PathBuf, Vec<String>)> {
    let exe = install::find_launcher().ok_or_else(|| Error(t("err.launcher_missing")))?;
    let mut cmd = vec!["protontricks-launch".to_string()];
    cmd.extend(shlex_split(&settings.protontricks_args));
    cmd.push("--appid".to_string());
    cmd.push(paths::APP_ID_STR.to_string());
    cmd.push(exe.to_string_lossy().into_owned());
    cmd.extend(shlex_split(&settings.extra_args));
    Ok((exe, cmd))
}

fn update_signature(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy();
    Some(format!(
        "{}|{}|{}",
        name,
        meta.len(),
        crate::core::util::mtime_secs(path)
    ))
}

fn pending_files() -> Vec<PathBuf> {
    let base = paths::launcher_base();
    let mut files: Vec<PathBuf> = Vec::new();
    let push_dir = |dir: PathBuf, files: &mut Vec<PathBuf>| {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|ext| ext == "exe").unwrap_or(false) && path.is_file() {
                files.push(path);
            }
        }
    };
    push_dir(base.join("new-launcher-updater/pending"), &mut files);
    if let Ok(entries) = fs::read_dir(&base) {
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("updater"))
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            push_dir(dir.join("pending"), &mut files);
        }
    }
    files
}

pub fn find_pending_update() -> Option<PathBuf> {
    if !paths::launcher_base().is_dir() {
        return None;
    }
    let mut files = pending_files();
    if files.is_empty() {
        return None;
    }
    files.sort();
    files.dedup();
    let newest = files
        .into_iter()
        .max_by_key(|path| crate::core::util::mtime_secs(path))?;
    let applied = fs::read_to_string(paths::update_state())
        .unwrap_or_default()
        .trim()
        .to_string();
    let signature = update_signature(&newest)?;
    if signature == applied {
        None
    } else {
        Some(newest)
    }
}

pub fn apply_pending_update(_settings: &Settings) -> Result<bool> {
    let Some(updater) = find_pending_update() else {
        return Ok(false);
    };
    log::line(&tf(
        "log.update_found",
        &[(
            "name",
            &updater
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )],
    ));
    process::kill_launcher();
    let _ = process::wait_until(|| !process::launcher_running(), 10.0);
    let mut args = vec![
        "--appid".to_string(),
        paths::APP_ID_STR.to_string(),
        updater.to_string_lossy().into_owned(),
    ];
    args.extend(UPDATE_ARGS.iter().map(|arg| (*arg).to_string()));
    let code = process::run_streaming(
        Path::new("protontricks-launch"),
        &args,
        &launch_env(false),
        None,
        false,
    )?;
    install::invalidate_launcher_cache();
    fs::create_dir_all(paths::state())?;
    if let Some(signature) = update_signature(&updater) {
        fs::write(paths::update_state(), signature)?;
    }
    log::line(&tf("log.update_exit", &[("code", &code.to_string())]));
    if process::wait_until(process::launcher_running, UPDATE_WAIT) {
        log::line(&t("log.update_started"));
        Ok(true)
    } else {
        log::line(&t("log.update_not_started"));
        Ok(false)
    }
}

fn run_watched(settings: &Settings, mut updated: bool) -> Result<i32> {
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || diag::watch(&stop))
    };
    let watcher_thread = watcher.thread().clone();
    let mut code = 0;
    let result = loop {
        let step = (|| -> Result<()> {
            if updated {
                process::wait_until(|| !process::launcher_running(), f64::INFINITY);
            } else {
                let (exe, cmd) = launcher_cmd(settings)?;
                let cwd = exe.parent().map(Path::to_path_buf);
                code = process::run_streaming(
                    Path::new(&cmd[0]),
                    &cmd[1..],
                    &launch_env(true),
                    cwd.as_deref(),
                    false,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = step {
            break Err(error);
        }
        if process::cancelled() {
            break Ok(code);
        }
        updated = match apply_pending_update(settings) {
            Ok(updated) => updated,
            Err(error) => break Err(error),
        };
        if !updated {
            break Ok(code);
        }
    };
    stop.store(true, Ordering::Release);
    watcher_thread.unpark();
    let _ = watcher.join();
    result
}

fn spawn_detached(settings: &Settings, output: &Path) -> Result<Option<i32>> {
    let (exe, cmd) = launcher_cmd(settings)?;
    log::line(&format!(
        "$ {}",
        cmd.iter()
            .map(|part| crate::core::util::shell_join(Path::new(part), &[]))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    let cwd = exe.parent().map(Path::to_path_buf);
    let mut child = process::spawn_child_to(
        Path::new(&cmd[0]),
        &cmd[1..],
        &launch_env(true),
        cwd.as_deref(),
        Some(output),
    )?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status.code().unwrap_or(-1))),
            Ok(None) => {}
            Err(_) => return Ok(Some(1)),
        }
        if started.elapsed().as_secs_f64() >= LAUNCH_GRACE {
            return Ok(None);
        }
        process::sleep_ms(50);
    }
}

pub fn launch(settings: &Settings, detach_out: Option<&Path>) -> Result<Option<i32>> {
    if install::find_launcher().is_none() {
        return Err(Error(t("err.launcher_missing")));
    }
    ensure_compat_tool(settings);
    if let Err(error) = launchopts::ensure() {
        log::line(&error.to_string());
    }
    ensure_steam_running();
    install::prepare_prefix(settings)?;
    process::clear_cancel();
    fs::create_dir_all(paths::state())?;
    link_steam_drive();
    proton::ensure_patched(&install::target(settings))?;
    let updated = apply_pending_update(settings)?;
    start_trim(settings);
    if detach_out.is_none() {
        return run_watched(settings, updated).map(Some);
    }
    if updated {
        return Ok(None);
    }
    spawn_detached(settings, detach_out.expect("detach path checked"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_runs_inside_the_protontricks_runtime() {
        let settings = Settings::default();
        let Ok((_exe, cmd)) = launcher_cmd(&settings) else {
            return;
        };
        assert_eq!(cmd[0], "protontricks-launch");
        assert_eq!(cmd[1], "--appid");
        assert_eq!(cmd[2], paths::APP_ID_STR);
        assert_eq!(cmd[3], _exe.to_string_lossy());
    }
}
