use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::core::{paths, process};

pub const FLAG: &str = "--trim-launcher";
const FIND_LIMIT: Duration = Duration::from_secs(1800);
const LAUNCHER_GRACE: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_secs(1);
const RENDERER_POLL: Duration = Duration::from_secs(3);
const AFTER_KILL: Duration = Duration::from_secs(5);
const EXIT_GRACE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Renderers,
    Suspend,
    All,
}

impl Mode {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" | "no" => Mode::Off,
            "all" | "full" | "kill" => Mode::All,
            "suspend" | "quiet" | "pause" | "freeze" => Mode::Suspend,
            _ => Mode::Renderers,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Renderers => "renderers",
            Mode::Suspend => "suspend",
            Mode::All => "all",
        }
    }
}

fn displays() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(value) = std::env::var("DISPLAY") {
        if !value.is_empty() {
            list.push(value);
        }
    }
    for value in [":0", ":1"] {
        if !list.iter().any(|item| item == value) {
            list.push(value.to_string());
        }
    }
    list
}

fn hide_windows(pids: &[i32]) {
    for display in displays() {
        for pid in pids {
            let mut search = Command::new("xdotool");
            search
                .args(["search", "--onlyvisible", "--pid", &pid.to_string()])
                .env("DISPLAY", &display)
                .stdin(Stdio::null())
                .stderr(Stdio::null());
            let Ok(output) = search.output() else {
                continue;
            };
            for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                let mut unmap = Command::new("xdotool");
                unmap
                    .args(["windowunmap", id])
                    .env("DISPLAY", &display)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = unmap.status();
            }
        }
    }
}

fn suspend_launcher() -> Vec<i32> {
    let pids = process::launcher_pids();
    if pids.is_empty() {
        return pids;
    }
    hide_windows(&pids);
    sleep(Duration::from_millis(100));
    for pid in &pids {
        unsafe {
            crate::ffi::kill(*pid, process::SIGSTOP);
        }
    }
    pids
}

fn terminate_launcher(pids: &[i32]) {
    for pid in pids {
        unsafe {
            crate::ffi::kill(*pid, process::SIGCONT);
        }
    }
    sleep(Duration::from_millis(100));
    for pid in pids {
        unsafe {
            crate::ffi::kill(*pid, process::SIGTERM);
        }
    }
    sleep(Duration::from_secs(2));
    kill_all(pids);
}

fn note(text: &str) {
    let dir = paths::logs_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("trim.log"))
    {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{secs}] {text}");
    }
}

fn kill_all(pids: &[i32]) {
    for pid in pids {
        unsafe {
            crate::ffi::kill(*pid, process::SIGKILL);
        }
    }
}

fn guard_path() -> std::path::PathBuf {
    paths::state().join("trim-all-failed")
}

pub fn run(requested: Mode, delay: u64) -> i32 {
    let mode = if requested == Mode::All && guard_path().exists() {
        Mode::Renderers
    } else {
        requested
    };
    let _ = fs::create_dir_all(paths::logs_dir());
    let _ = fs::write(paths::logs_dir().join("trim.log"), "");
    note(&format!(
        "requested={} mode={} delay={delay}",
        requested.as_str(),
        mode.as_str()
    ));
    let started = Instant::now();
    loop {
        if !process::game_pids().is_empty() {
            break;
        }
        let waited = started.elapsed();
        if waited > FIND_LIMIT || (waited > LAUNCHER_GRACE && !process::launcher_running()) {
            note("game was not found");
            return 0;
        }
        sleep(POLL);
    }
    note("game found");
    let seen = Instant::now();
    while seen.elapsed() < Duration::from_secs(delay) {
        if process::game_pids().is_empty() {
            note("game ended before the launcher was trimmed");
            return 0;
        }
        sleep(POLL);
    }
    match mode {
        Mode::Off => {}
        Mode::All => {
            let pids = process::launcher_pids();
            note(&format!("killing {} launcher processes", pids.len()));
            kill_all(&pids);
            sleep(AFTER_KILL);
            if process::game_pids().is_empty() {
                note("game ended after the launcher was killed");
                let _ = fs::write(guard_path(), "");
            } else {
                note("game is still running after the launcher was killed");
            }
        }
        Mode::Renderers => loop {
            if process::game_pids().is_empty() {
                note("game ended");
                break;
            }
            let pids = process::launcher_renderer_pids();
            if !pids.is_empty() {
                note(&format!(
                    "killing {} launcher renderer processes",
                    pids.len()
                ));
                kill_all(&pids);
            }
            sleep(RENDERER_POLL);
        },
        Mode::Suspend => {
            let pids = suspend_launcher();
            if pids.is_empty() {
                note("launcher is not running");
                return 0;
            }
            note(&format!("suspended {} launcher processes", pids.len()));
            loop {
                while !process::game_pids().is_empty() {
                    sleep(POLL);
                }
                let ended = Instant::now();
                while ended.elapsed() < EXIT_GRACE {
                    if !process::game_pids().is_empty() {
                        break;
                    }
                    sleep(POLL);
                }
                if process::game_pids().is_empty() {
                    break;
                }
            }
            terminate_launcher(&pids);
            note("launcher terminated after game exit");
        }
    }
    0
}

pub fn from_args(args: &[String]) -> Option<i32> {
    let at = args.iter().position(|arg| arg == FLAG)?;
    let mode = Mode::parse(args.get(at + 1).map(String::as_str).unwrap_or("renderers"));
    let delay = args
        .get(at + 2)
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(90);
    Some(run(mode, delay))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes() {
        assert_eq!(Mode::parse("off"), Mode::Off);
        assert_eq!(Mode::parse(" ALL "), Mode::All);
        assert_eq!(Mode::parse("renderers"), Mode::Renderers);
        assert_eq!(Mode::parse("suspend"), Mode::Suspend);
        assert_eq!(Mode::parse("anything"), Mode::Renderers);
    }

    #[test]
    fn reads_arguments() {
        assert_eq!(from_args(&[]), None);
    }
}
