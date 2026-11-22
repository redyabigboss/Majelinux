use std::collections::HashSet;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::core::process;
use crate::core::{log, paths};
use crate::i18n::{t, tf};

const SKIP: [&str; 13] = [
    "wineserver",
    "services.exe",
    "winedevice.exe",
    "plugplay.exe",
    "svchost.exe",
    "explorer.exe",
    "rpcss.exe",
    "conhost.exe",
    "tabtip",
    "winemenubuilder",
    "preloader",
    "steam.exe",
    "",
];

struct Info {
    cmdline: String,
    lc_all: Option<String>,
    shim_loaded: bool,
}

struct Scratch {
    cmdline_path: String,
    cmdline: Vec<u8>,
    environ_path: String,
    environ: Vec<u8>,
    environ_entry: Vec<u8>,
    maps_path: String,
    maps: Vec<u8>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            cmdline_path: String::with_capacity(32),
            cmdline: Vec::with_capacity(256),
            environ_path: String::with_capacity(32),
            environ: Vec::with_capacity(4096),
            environ_entry: Vec::with_capacity(128),
            maps_path: String::with_capacity(32),
            maps: Vec::with_capacity(8192),
        }
    }
}

fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    let mut out = String::with_capacity(limit + 1);
    out.push('…');
    out.extend(text.chars().skip(count - limit));
    out
}

fn contains_ascii_case_insensitive(text: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && text
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

fn inspect(pid: i32, scratch: &mut Scratch, prefix: &[u8], shim_path: &[u8]) -> Option<Info> {
    if !process::proc_cmdline_into(pid, &mut scratch.cmdline_path, &mut scratch.cmdline) {
        return None;
    }
    let cmdline_text = String::from_utf8_lossy(&scratch.cmdline);
    if cmdline_text
        .chars()
        .all(|ch| ch == '\0' || ch.is_whitespace())
        || contains_ascii_case_insensitive(&scratch.cmdline, b"--type=")
        || SKIP.iter().any(|needle| {
            !needle.is_empty()
                && contains_ascii_case_insensitive(&scratch.cmdline, needle.as_bytes())
        })
    {
        return None;
    }

    let lc_all = process::proc_environ_value_if_contains(
        pid,
        prefix,
        b"LC_ALL=",
        &mut scratch.environ_path,
        &mut scratch.environ,
        &mut scratch.environ_entry,
    )?;
    let shim_loaded =
        process::proc_maps_contains(pid, shim_path, &mut scratch.maps_path, &mut scratch.maps);
    let cmdline = cmdline_text.replace('\0', " ").trim().to_string();
    Some(Info {
        cmdline,
        lc_all,
        shim_loaded,
    })
}

pub fn watch(stop: &AtomicBool) {
    let mut seen: HashSet<i32> = HashSet::new();
    let mut present: HashSet<i32> = HashSet::new();
    let prefix = format!("WINEPREFIX={}", paths::pfx().to_string_lossy());
    let shim_path = paths::shim_so();
    let shim_path = shim_path.to_string_lossy().into_owned();
    let mut scratch = Scratch::new();
    while !stop.load(Ordering::Acquire) {
        present.clear();
        let Ok(entries) = fs::read_dir("/proc") else {
            thread::park_timeout(Duration::from_secs(3));
            continue;
        };
        for entry in entries.flatten() {
            if stop.load(Ordering::Acquire) {
                return;
            }
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|text| text.parse::<i32>().ok()) else {
                continue;
            };
            present.insert(pid);
            if !seen.insert(pid) {
                continue;
            }
            let Some(info) = inspect(pid, &mut scratch, prefix.as_bytes(), shim_path.as_bytes())
            else {
                continue;
            };
            let lc = info.lc_all.as_deref().unwrap_or("__unset__");
            let lc = if lc == "__unset__" {
                t("diag.unset")
            } else {
                lc.to_string()
            };
            let state = if info.shim_loaded {
                t("diag.loaded")
            } else {
                t("diag.not_loaded")
            };
            log::line(&tf(
                "diag.shim",
                &[
                    ("pid", &pid.to_string()),
                    ("name", &tail(&info.cmdline, 90)),
                    ("lc", &lc),
                    ("state", &state),
                ],
            ));
        }
        seen.retain(|pid| present.contains(pid));
        thread::park_timeout(Duration::from_secs(3));
    }
}
