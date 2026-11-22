use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::util::{shell_join, strip_ansi};
use crate::core::{Error, Result};
use crate::core::{log, paths};
use crate::i18n::tf;

pub const SIGTERM: i32 = 15;
pub const SIGKILL: i32 = 9;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;

pub static CANCEL: AtomicBool = AtomicBool::new(false);
static CURRENT: Mutex<Option<i32>> = Mutex::new(None);

pub fn clear_cancel() {
    CANCEL.store(false, Ordering::SeqCst);
}

pub fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

pub fn sleep_ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

fn file_name(program: &Path) -> String {
    program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.display().to_string())
}

fn spawn_error(program: &Path, error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::NotFound {
        Error(tf("err.cmd_not_found", &[("cmd", &file_name(program))]))
    } else {
        Error(error.to_string())
    }
}

fn apply_common(cmd: &mut Command, envs: &[(String, String)], cwd: Option<&Path>, detached: bool) {
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    if detached {
        unsafe {
            cmd.pre_exec(|| {
                crate::ffi::setsid();
                Ok(())
            });
        }
    }
}

pub fn run_streaming(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: Option<&Path>,
    check: bool,
) -> Result<i32> {
    log::line(&format!("$ {}", shell_join(program, args)));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_common(&mut cmd, envs, cwd, true);

    unsafe {
        cmd.pre_exec(|| {
            crate::ffi::dup2(1, 2);
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|error| spawn_error(program, error))?;
    let pid = child.id() as i32;
    if let Ok(mut current) = CURRENT.lock() {
        *current = Some(pid);
    }
    if cancelled() {
        signal_group(pid, SIGTERM);
    }

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            log::line(&strip_ansi(line.trim_end_matches('\r')));
        }
    }
    let status = child.wait()?;
    if let Ok(mut current) = CURRENT.lock() {
        *current = None;
    }
    let code = status.code().unwrap_or(-1);
    if check && code != 0 {
        return Err(Error(tf(
            "err.exit_code",
            &[("name", &file_name(program)), ("code", &code.to_string())],
        )));
    }
    Ok(code)
}

pub fn spawn_detached(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: Option<&Path>,
) -> Result<u32> {
    spawn_detached_to(program, args, envs, cwd, None)
}

pub fn spawn_detached_to(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: Option<&Path>,
    output: Option<&Path>,
) -> Result<u32> {
    let mut child = spawn_child_to(program, args, envs, cwd, output)?;
    let pid = child.id();
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

pub fn spawn_child_to(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: Option<&Path>,
    output: Option<&Path>,
) -> Result<Child> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let clone = file.try_clone()?;
        cmd.stdout(Stdio::from(file)).stderr(Stdio::from(clone));
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    apply_common(&mut cmd, envs, cwd, true);
    cmd.spawn().map_err(|error| spawn_error(program, error))
}

pub fn signal_group(pid: i32, signal: i32) {
    unsafe {
        let group = crate::ffi::getpgid(pid);
        if group > 0 {
            crate::ffi::killpg(group, signal);
        } else {
            crate::ffi::kill(pid, signal);
        }
    }
}

pub fn stop_current_only(force: bool) {
    CANCEL.store(true, Ordering::SeqCst);
    for pid in launcher_pids() {
        unsafe {
            crate::ffi::kill(pid, SIGCONT);
        }
    }
    let pid = CURRENT.lock().ok().and_then(|current| *current);
    if let Some(pid) = pid {
        signal_group(pid, if force { SIGKILL } else { SIGTERM });
    }
}

pub fn wineserver_kill(proton_dir: &Path) {
    let wineserver = crate::core::proton::runtime_dir(proton_dir).join("bin/wineserver");
    if !wineserver.is_file() {
        return;
    }
    let mut cmd = Command::new(&wineserver);
    cmd.arg("-k")
        .env("WINEPREFIX", paths::pfx())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(mut child) = cmd.spawn() {
        let _ = child.wait();
    }
}

pub fn stop(force: bool, proton_dir: Option<&Path>) {
    stop_current_only(force);
    if let Some(dir) = proton_dir {
        wineserver_kill(dir);
    }
}

pub fn flush_registry(proton_dir: &Path) {
    let wineserver = crate::core::proton::runtime_dir(proton_dir).join("bin/wineserver");
    if !wineserver.is_file() {
        return;
    }
    let mut cmd = Command::new(&wineserver);
    cmd.arg("-w")
        .env("WINEPREFIX", paths::pfx())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                if started.elapsed() >= Duration::from_secs(30) {
                    signal_group(child.id() as i32, SIGKILL);
                    let _ = child.wait();
                    return;
                }
                sleep_ms(100);
            }
            Err(_) => return,
        }
    }
}

fn for_each_pid(mut visit: impl FnMut(i32) -> bool) {
    let Ok(entries) = fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(pid) = name.to_str().and_then(|text| text.parse::<i32>().ok())
            && !visit(pid)
        {
            break;
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

pub fn proc_cmdline_into(pid: i32, path: &mut String, buffer: &mut Vec<u8>) -> bool {
    read_proc_file(pid, "cmdline", path, buffer) && !buffer.is_empty()
}

fn inspect_env_entry(
    entry: &[u8],
    required_entry: &[u8],
    value_prefix: &[u8],
    required_found: &mut bool,
    value: &mut Option<String>,
) {
    let required_matches = entry == required_entry
        || (std::str::from_utf8(entry).is_err()
            && String::from_utf8_lossy(entry).as_bytes() == required_entry);
    if required_matches {
        *required_found = true;
    }
    if value.is_none()
        && let Some(raw) = entry.strip_prefix(value_prefix)
    {
        *value = Some(String::from_utf8_lossy(raw).into_owned());
    }
}

pub fn proc_environ_value_if_contains(
    pid: i32,
    required_entry: &[u8],
    value_prefix: &[u8],
    path: &mut String,
    chunk: &mut Vec<u8>,
    entry: &mut Vec<u8>,
) -> Option<Option<String>> {
    path.clear();
    entry.clear();
    let _ = write!(path, "/proc/{pid}/environ");
    let mut file = File::open(path.as_str()).ok()?;

    const CHUNK: usize = 4096;
    chunk.resize(CHUNK, 0);

    let mut required_found = false;
    let mut value = None;
    loop {
        let read = file.read(&mut chunk[..]).ok()?;
        if read == 0 {
            break;
        }
        for byte in &chunk[..read] {
            if *byte == 0 {
                inspect_env_entry(
                    entry,
                    required_entry,
                    value_prefix,
                    &mut required_found,
                    &mut value,
                );
                entry.clear();
                if required_found && value.is_some() {
                    break;
                }
            } else {
                entry.push(*byte);
            }
        }
        if required_found && value.is_some() {
            break;
        }
    }
    if !(required_found && value.is_some()) && !entry.is_empty() {
        inspect_env_entry(
            entry,
            required_entry,
            value_prefix,
            &mut required_found,
            &mut value,
        );
    }
    entry.clear();
    required_found.then_some(value)
}

pub fn proc_maps_contains(
    pid: i32,
    needle: &[u8],
    path: &mut String,
    buffer: &mut Vec<u8>,
) -> bool {
    if needle.is_empty() {
        return false;
    }
    path.clear();
    let _ = write!(path, "/proc/{pid}/maps");
    let Ok(mut file) = File::open(path.as_str()) else {
        return false;
    };

    const CHUNK: usize = 8192;
    let overlap = needle.len().saturating_sub(1);
    let capacity = CHUNK.saturating_add(overlap);
    buffer.resize(capacity, 0);

    let mut carry = 0usize;
    loop {
        let read = match file.read(&mut buffer[carry..]) {
            Ok(read) => read,
            Err(_) => return false,
        };
        if read == 0 {
            return false;
        }
        let end = carry + read;
        if buffer[..end]
            .windows(needle.len())
            .any(|window| window == needle)
        {
            return true;
        }
        carry = end.min(overlap);
        buffer.copy_within(end - carry..end, 0);
    }
}

fn read_proc_file(pid: i32, name: &str, path: &mut String, buffer: &mut Vec<u8>) -> bool {
    path.clear();
    let _ = write!(path, "/proc/{pid}/{name}");
    let Ok(mut file) = File::open(path.as_str()) else {
        return false;
    };
    buffer.clear();
    file.read_to_end(buffer).is_ok()
}

pub fn steam_running() -> bool {
    let mut path = String::with_capacity(32);
    let mut bytes = Vec::with_capacity(32);
    let mut found = false;
    for_each_pid(|pid| {
        if read_proc_file(pid, "comm", &mut path, &mut bytes)
            && bytes
                .as_slice()
                .strip_suffix(b"\n")
                .unwrap_or(bytes.as_slice())
                == b"steam"
        {
            found = true;
            false
        } else {
            true
        }
    });
    found
}

fn steam_pids() -> Vec<i32> {
    let mut pids = Vec::new();
    let mut path = String::with_capacity(32);
    let mut bytes = Vec::with_capacity(32);
    for_each_pid(|pid| {
        if read_proc_file(pid, "comm", &mut path, &mut bytes)
            && bytes
                .as_slice()
                .strip_suffix(b"\n")
                .unwrap_or(bytes.as_slice())
                == b"steam"
        {
            pids.push(pid);
        }
        true
    });
    pids
}

fn steam_command() -> Option<(std::path::PathBuf, Vec<String>)> {
    if let Some(path) = crate::core::util::which("steam") {
        return Some((path, Vec::new()));
    }
    let flatpak = crate::core::util::which("flatpak")?;
    if paths::home()
        .join(".var/app/com.valvesoftware.Steam")
        .is_dir()
    {
        return Some((
            flatpak,
            vec!["run".to_string(), "com.valvesoftware.Steam".to_string()],
        ));
    }
    None
}

pub fn steam_shutdown() -> bool {
    if !steam_running() {
        return true;
    }
    if let Some((program, mut args)) = steam_command() {
        args.push("-shutdown".to_string());
        let _ = spawn_detached(&program, &args, &[], None);
    }
    if wait_until(|| !steam_running(), 45.0) {
        return true;
    }
    for pid in steam_pids() {
        unsafe {
            crate::ffi::kill(pid, SIGTERM);
        }
    }
    wait_until(|| !steam_running(), 15.0)
}

pub fn steam_start() {
    if let Some((program, args)) = steam_command() {
        let _ = spawn_detached(&program, &args, &[], None);
    }
}

fn is_launcher_pid(pid: i32, path: &mut String, bytes: &mut Vec<u8>) -> bool {
    const NAME: &[u8] = b"Majestic Launcher.exe";
    read_proc_file(pid, "cmdline", path, bytes)
        && bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && contains(bytes, NAME)
}

fn windows_cmdline_pids(name: &[u8], extra: Option<&[u8]>) -> Vec<i32> {
    let mut pids = Vec::new();
    let mut path = String::with_capacity(32);
    let mut bytes = Vec::with_capacity(256);
    for_each_pid(|pid| {
        if read_proc_file(pid, "cmdline", &mut path, &mut bytes)
            && bytes.len() > 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
            && contains(&bytes, name)
            && extra.is_none_or(|needle| contains(&bytes, needle))
        {
            pids.push(pid);
        }
        true
    });
    pids
}

pub fn game_pids() -> Vec<i32> {
    windows_cmdline_pids(b"GTA5", None)
}

pub fn launcher_renderer_pids() -> Vec<i32> {
    windows_cmdline_pids(b"Majestic Launcher.exe", Some(b"--type=renderer"))
}

pub fn launcher_pids() -> Vec<i32> {
    let mut pids = Vec::new();
    let mut path = String::with_capacity(32);
    let mut bytes = Vec::with_capacity(256);
    for_each_pid(|pid| {
        if is_launcher_pid(pid, &mut path, &mut bytes) {
            pids.push(pid);
        }
        true
    });
    pids
}

pub fn launcher_running() -> bool {
    let mut path = String::with_capacity(32);
    let mut bytes = Vec::with_capacity(256);
    let mut found = false;
    for_each_pid(|pid| {
        if is_launcher_pid(pid, &mut path, &mut bytes) {
            found = true;
            false
        } else {
            true
        }
    });
    found
}

pub fn kill_launcher() {
    for pid in launcher_pids() {
        unsafe {
            crate::ffi::kill(pid, SIGTERM);
        }
    }
}

pub fn wait_until<F>(mut condition: F, seconds: f64) -> bool
where
    F: FnMut() -> bool,
{
    let started = Instant::now();
    let interval = if seconds.is_infinite() { 1000 } else { 100 };
    loop {
        if condition() {
            return true;
        }
        if cancelled() || started.elapsed().as_secs_f64() >= seconds {
            return false;
        }
        sleep_ms(interval);
    }
}

pub fn proton_env() -> Vec<(String, String)> {
    vec![
        (
            "STEAM_COMPAT_DATA_PATH".into(),
            paths::compat().to_string_lossy().into_owned(),
        ),
        (
            "STEAM_COMPAT_CLIENT_INSTALL_PATH".into(),
            paths::steam().to_string_lossy().into_owned(),
        ),
        ("SteamAppId".into(), paths::APP_ID_STR.into()),
        ("SteamGameId".into(), paths::APP_ID_STR.into()),
    ]
}
