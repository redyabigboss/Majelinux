use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::core::{Error, Result, net, paths, process, sha256};
use crate::i18n::{t, tf};

pub const BINARY_ASSET: &str = "majelinux-linux-x86_64";
pub const SUMS_ASSET: &str = "SHA256SUMS";
pub const WAIT_PID_FLAG: &str = "--wait-pid";
const API_URL: &str = "https://api.github.com/repos/redyabigboss/majelinux/releases/latest";
const MIN_SIZE: u64 = 64 * 1024;
const WAIT_LIMIT_MS: u64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct Staged {
    pub path: PathBuf,
}

pub fn asset_url(tag: &str, name: &str) -> String {
    format!("{}/releases/download/{tag}/{name}", paths::REPO_URL)
}

fn work_dir() -> PathBuf {
    paths::state().join("update")
}

fn previous_dir() -> PathBuf {
    paths::state().join("previous")
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = text.find(&needle)? + needle.len();
    let rest = text[at..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let end = rest.find('"')?;
    let value = &rest[..end];
    if value.contains('\\') {
        return None;
    }
    Some(value.to_string())
}

fn parse_version(text: &str) -> Option<Vec<u64>> {
    let trimmed = text.trim().trim_start_matches(['v', 'V']);
    let end = trimmed
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(trimmed.len());
    let core = &trimmed[..end];
    if core.is_empty() {
        return None;
    }
    core.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (parse_version(remote), parse_version(local)) else {
        return false;
    };
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

fn safe_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

pub fn latest() -> Result<Option<Release>> {
    let body = net::fetch_text(API_URL)?;
    let tag = json_string(&body, "tag_name").ok_or_else(|| Error(t("update.err.parse")))?;
    if !safe_tag(&tag) {
        return Err(Error(t("update.err.parse")));
    }
    if !is_newer(&tag, paths::APP_VERSION) {
        return Ok(None);
    }
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    Ok(Some(Release { tag, version }))
}

fn sum_for(sums: &str, name: &str) -> Option<String> {
    for line in sums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(file)) = (parts.next(), parts.next()) else {
            continue;
        };
        if file.trim_start_matches('*') != name {
            continue;
        }
        if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

fn check_binary(path: &Path, version: &str) -> Result<()> {
    let meta = fs::metadata(path)?;
    if meta.len() < MIN_SIZE {
        return Err(Error(t("update.err.invalid")));
    }
    let mut head = [0u8; 4];
    fs::File::open(path)?.read_exact(&mut head)?;
    if head != [0x7f, b'E', b'L', b'F'] {
        return Err(Error(t("update.err.invalid")));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    let output = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| Error(tf("update.err.run", &[("err", &error.to_string())])))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let expected = format!("{} {version}", paths::APP_NAME);
    if !output.status.success() || text.trim() != expected {
        return Err(Error(t("update.err.invalid")));
    }
    Ok(())
}

pub fn stage(release: &Release) -> Result<Staged> {
    if !safe_tag(&release.tag) {
        return Err(Error(t("update.err.parse")));
    }
    let dir = work_dir().join(&release.tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let result = stage_into(release, &dir);
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

fn stage_into(release: &Release, dir: &Path) -> Result<Staged> {
    let sums_path = dir.join(SUMS_ASSET);
    net::download(&asset_url(&release.tag, SUMS_ASSET), &sums_path)?;
    let sums = fs::read_to_string(&sums_path)?;
    let expected = sum_for(&sums, BINARY_ASSET).ok_or_else(|| Error(t("update.err.no_sum")))?;
    let binary = dir.join(BINARY_ASSET);
    net::download(&asset_url(&release.tag, BINARY_ASSET), &binary)?;
    let actual = sha256::file_hex(&binary)?;
    if actual != expected {
        return Err(Error(t("update.err.checksum")));
    }
    check_binary(&binary, &release.version)?;
    Ok(Staged { path: binary })
}

pub fn target() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|_| Error(t("update.err.target")))?;
    let text = exe.to_string_lossy().into_owned();
    match text.strip_suffix(" (deleted)") {
        Some(stripped) => Ok(PathBuf::from(stripped)),
        None => Ok(exe),
    }
}

pub fn package_managed(target: &Path) -> bool {
    ["/usr", "/nix/store", "/app"]
        .iter()
        .any(|root| target.starts_with(root))
        && !target.starts_with("/usr/local")
}

pub fn disabled() -> bool {
    std::env::var_os("MAJELINUX_NO_SELF_UPDATE")
        .is_some_and(|value| !value.is_empty() && value != "0")
}

fn replace_error(error: std::io::Error) -> Error {
    Error(tf("update.err.replace", &[("err", &error.to_string())]))
}

fn keep_previous(target: &Path) {
    let dir = previous_dir();
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::copy(target, dir.join(paths::APP_NAME));
    }
}

pub fn apply(staged: &Staged) -> Result<PathBuf> {
    if disabled() {
        return Err(Error(t("update.err.disabled")));
    }
    let target = target()?;
    if package_managed(&target) {
        return Err(Error(t("update.err.package")));
    }
    let parent = target
        .parent()
        .ok_or_else(|| Error(t("update.err.target")))?
        .to_path_buf();
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| paths::APP_NAME.to_string());
    let incoming = parent.join(format!(".{name}.update"));
    let _ = fs::remove_file(&incoming);
    fs::copy(&staged.path, &incoming).map_err(replace_error)?;
    let mode = fs::metadata(&target)
        .map(|meta| meta.permissions().mode())
        .unwrap_or(0o755)
        | 0o111;
    if let Err(error) = fs::set_permissions(&incoming, fs::Permissions::from_mode(mode)) {
        let _ = fs::remove_file(&incoming);
        return Err(replace_error(error));
    }
    keep_previous(&target);
    if let Err(error) = fs::rename(&incoming, &target) {
        let _ = fs::remove_file(&incoming);
        return Err(replace_error(error));
    }
    let _ = fs::remove_dir_all(work_dir());
    Ok(target)
}

pub fn restart(target: &Path) -> Result<()> {
    let args = vec![WAIT_PID_FLAG.to_string(), std::process::id().to_string()];
    process::spawn_detached(target, &args, &[], None).map(|_| ())
}

fn alive(pid: u32) -> bool {
    let Ok(text) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    match text.rfind(')') {
        Some(at) => !text[at + 1..].trim_start().starts_with('Z'),
        None => true,
    }
}

pub fn wait_for_exit(pid: u32) {
    let mut waited = 0;
    while waited < WAIT_LIMIT_MS && alive(pid) {
        std::thread::sleep(Duration::from_millis(100));
        waited += 100;
    }
}

pub fn wait_pid_arg(args: &[String]) -> Option<u32> {
    let at = args.iter().position(|arg| arg == WAIT_PID_FLAG)?;
    args.get(at + 1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.1", "1.0.9"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v1.0.0", "1.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn reads_tag_from_release_json() {
        let body = "{\n  \"url\": \"x\",\n  \"tag_name\": \"v1.2.3\",\n  \"name\": \"y\"\n}";
        assert_eq!(json_string(body, "tag_name").as_deref(), Some("v1.2.3"));
        assert_eq!(json_string(body, "missing"), None);
        assert_eq!(json_string("{\"tag_name\":\"a\\\"b\"}", "tag_name"), None);
    }

    #[test]
    fn accepts_only_plain_tags() {
        assert!(safe_tag("v1.0.1"));
        assert!(safe_tag("1.0.0-rc.1"));
        assert!(!safe_tag("../x"));
        assert!(!safe_tag("v1 0"));
        assert!(!safe_tag(""));
    }

    #[test]
    fn finds_checksum_line() {
        let hash = "a".repeat(64);
        let sums = format!(
            "{hash}  majelinux-linux-x86_64\n{} *other\n",
            "b".repeat(64)
        );
        assert_eq!(sum_for(&sums, "majelinux-linux-x86_64"), Some(hash));
        assert_eq!(sum_for(&sums, "other"), Some("b".repeat(64)));
        assert_eq!(sum_for(&sums, "none"), None);
        assert_eq!(
            sum_for("zz  majelinux-linux-x86_64", "majelinux-linux-x86_64"),
            None
        );
    }

    #[test]
    fn package_roots_are_matched_as_components() {
        assert!(package_managed(Path::new("/usr/bin/majelinux")));
        assert!(!package_managed(Path::new("/usr/local/bin/majelinux")));
        assert!(!package_managed(Path::new("/home/u/.local/bin/majelinux")));
        assert!(!package_managed(Path::new("/usr-backup/majelinux")));
    }

    #[test]
    fn reads_wait_pid_argument() {
        let args = vec!["--wait-pid".to_string(), "42".to_string()];
        assert_eq!(wait_pid_arg(&args), Some(42));
        assert_eq!(wait_pid_arg(&[]), None);
        assert_eq!(wait_pid_arg(&["--wait-pid".to_string()]), None);
    }
}
