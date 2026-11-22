use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::core::log;
use crate::core::util::which;
use crate::core::{Error, Result};
use crate::i18n::tf;

fn part_path(dst: &Path) -> PathBuf {
    let mut name = OsString::from(dst.as_os_str());
    name.push(".part");
    PathBuf::from(name)
}

fn run_downloader(program: &str, args: &[String]) -> std::io::Result<std::process::Output> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
}

pub fn download(url: &str, dst: &Path) -> Result<()> {
    log::line(&tf("log.downloading", &[("url", url)]));
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = part_path(dst);
    let _ = fs::remove_file(&tmp);
    let tmp_text = tmp.to_string_lossy().into_owned();

    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    if which("curl").is_some() {
        attempts.push((
            "curl",
            vec![
                "--fail".into(),
                "--location".into(),
                "--silent".into(),
                "--show-error".into(),
                "--retry".into(),
                "3".into(),
                "--connect-timeout".into(),
                "20".into(),
                "--max-time".into(),
                "1800".into(),
                "--output".into(),
                tmp_text.clone(),
                url.into(),
            ],
        ));
    }
    if which("wget").is_some() {
        attempts.push((
            "wget",
            vec![
                "-q".into(),
                "--timeout=60".into(),
                "-O".into(),
                tmp_text.clone(),
                url.into(),
            ],
        ));
    }
    if attempts.is_empty() {
        return Err(Error(tf(
            "err.download",
            &[("url", url), ("err", "curl and wget are not installed")],
        )));
    }

    let mut last_error = String::new();
    for (program, args) in attempts {
        match run_downloader(program, &args) {
            Ok(output) if output.status.success() => {
                if fs::metadata(&tmp).map(|m| m.len() > 0).unwrap_or(false) {
                    fs::rename(&tmp, dst)?;
                    return Ok(());
                }
                last_error = format!("{program}: empty response");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                last_error = if stderr.is_empty() {
                    format!("{program}: exit code {:?}", output.status.code())
                } else {
                    stderr
                };
            }
            Err(error) => last_error = format!("{program}: {error}"),
        }
        let _ = fs::remove_file(&tmp);
    }
    Err(Error(tf(
        "err.download",
        &[("url", url), ("err", &last_error)],
    )))
}

pub fn fetch_text(url: &str) -> Result<String> {
    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    if which("curl").is_some() {
        attempts.push((
            "curl",
            vec![
                "--fail".into(),
                "--location".into(),
                "--silent".into(),
                "--show-error".into(),
                "--connect-timeout".into(),
                "15".into(),
                "--max-time".into(),
                "30".into(),
                "--header".into(),
                "Accept: application/vnd.github+json".into(),
                url.into(),
            ],
        ));
    }
    if which("wget").is_some() {
        attempts.push((
            "wget",
            vec![
                "-q".into(),
                "--timeout=30".into(),
                "--header=Accept: application/vnd.github+json".into(),
                "-O".into(),
                "-".into(),
                url.into(),
            ],
        ));
    }
    if attempts.is_empty() {
        return Err(Error(tf(
            "err.download",
            &[("url", url), ("err", "curl and wget are not installed")],
        )));
    }
    let mut last_error = String::new();
    for (program, args) in attempts {
        let output = Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        match output {
            Ok(out) if out.status.success() && !out.stdout.is_empty() => {
                return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                last_error = if stderr.is_empty() {
                    format!("{program}: exit code {:?}", out.status.code())
                } else {
                    stderr
                };
            }
            Err(error) => last_error = format!("{program}: {error}"),
        }
    }
    Err(Error(tf(
        "err.download",
        &[("url", url), ("err", &last_error)],
    )))
}
