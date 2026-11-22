use std::cmp::Ordering;
use std::fs;
use std::iter::Peekable;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::Chars;
use std::sync::{Arc, Mutex};

use crate::core::{Error, Result, log, paths, steam, vdf};
use crate::i18n::{t, tf};

pub const SUFFIX: &str = "-Majelinux";

const OLD: &[u8] = &[
    0x4c, 0x87, 0x14, 0x24, 0x41, 0x53, 0x48, 0x81, 0x24, 0x24, 0xff, 0xbf, 0xff, 0xff, 0x9d, 0x48,
    0xcf,
];
const NEW: &[u8] = &[
    0x4c, 0x89, 0xd1, 0x4c, 0x87, 0x14, 0x24, 0x41, 0x53, 0x80, 0x64, 0x24, 0x01, 0xbf, 0x9d, 0x48,
    0xcf,
];
const MODERN: &[u8] = &[
    0x48, 0x8b, 0x0c, 0x24, 0x41, 0x53, 0x48, 0x81, 0x24, 0x24, 0xff, 0xbf, 0xff, 0xff, 0x9d, 0x48,
    0xcf,
];

const ANCHOR: [u8; 3] = [0x9d, 0x48, 0xcf];
const ANCHOR_OFFSET: usize = 14;

const STOCK_TOOL_NAME: &str = "proton_experimental";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchState {
    Missing,
    Patchable,
    Patched,
    Modern,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Broken,
    Missing,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub source: PathBuf,
    pub patched: PathBuf,
    pub name: String,
    pub display: String,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub readiness: Readiness,
    pub stock: bool,
}

#[derive(Default, Clone, Copy)]
struct Scan {
    old: usize,
    new: usize,
    modern: usize,
    old_at: Option<usize>,
}

impl Scan {
    fn state(&self) -> PatchState {
        if self.new > 0 && self.old == 0 {
            PatchState::Patched
        } else if self.old == 1 {
            PatchState::Patchable
        } else if self.old == 0 && self.modern > 0 {
            PatchState::Modern
        } else {
            PatchState::Unknown
        }
    }
}

fn find_anchor(haystack: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = haystack[from..].iter().position(|byte| *byte == ANCHOR[0]) {
        let index = from + offset;
        if haystack[index..].starts_with(&ANCHOR) {
            return Some(index);
        }
        from = index + 1;
    }
    None
}

fn scan(data: &[u8]) -> Scan {
    let mut result = Scan::default();
    let mut from = 0;
    while let Some(found) = find_anchor(&data[from..]) {
        let anchor = from + found;
        from = anchor + 1;
        let Some(start) = anchor.checked_sub(ANCHOR_OFFSET) else {
            continue;
        };
        let Some(window) = data.get(start..start + OLD.len()) else {
            continue;
        };
        if window == OLD {
            result.old += 1;
            result.old_at.get_or_insert(start);
        } else if window == NEW {
            result.new += 1;
        } else if window == MODERN {
            result.modern += 1;
        }
    }
    result
}

pub fn runtime_dir(root: &Path) -> PathBuf {
    let files = root.join("files");
    if files.is_dir() {
        return files;
    }
    let dist = root.join("dist");
    if dist.is_dir() {
        return dist;
    }
    files
}

const NTDLL_SUBPATHS: [&str; 3] = [
    "wine/x86_64-unix/ntdll.so",
    "wine/ntdll.so",
    "wine/ntdll.dll.so",
];

pub fn find_ntdll(root: &Path) -> Option<PathBuf> {
    let mut best: Option<PathBuf> = None;
    for entry in fs::read_dir(runtime_dir(root)).ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("lib") {
            continue;
        }
        let base = entry.path();
        let Some(candidate) = NTDLL_SUBPATHS
            .iter()
            .map(|sub| base.join(sub))
            .find(|path| path.is_file())
        else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.as_path() < current.as_path())
        {
            best = Some(candidate);
        }
    }
    best
}

type CacheEntry = (PathBuf, u64, u64, PatchState);

static STATE_CACHE: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());

pub fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    Some((mtime, meta.len()))
}

pub fn patch_state(root: &Path) -> PatchState {
    let Some(so) = find_ntdll(root) else {
        return PatchState::Missing;
    };
    let Some((mtime, len)) = file_stamp(&so) else {
        return PatchState::Missing;
    };
    if let Ok(cache) = STATE_CACHE.lock()
        && let Some(entry) = cache
            .iter()
            .find(|entry| entry.0 == so && entry.1 == mtime && entry.2 == len)
    {
        return entry.3;
    }
    let Ok(data) = fs::read(&so) else {
        return PatchState::Missing;
    };
    let state = scan(&data).state();
    if let Ok(mut cache) = STATE_CACHE.lock() {
        cache.retain(|entry| entry.0 != so);
        cache.push((so, mtime, len, state));
    }
    state
}

pub fn is_patched(root: &Path) -> bool {
    matches!(
        patch_state(root),
        PatchState::Patched | PatchState::Modern | PatchState::Unknown
    )
}

fn identity(root: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(root.join("compatibilitytool.vdf")).ok()?;
    let entries = vdf::parse(&text).ok()?;
    let compat = entries
        .iter()
        .find(|entry| entry.key.text == "compatibilitytools" && entry.children.is_some())?;
    let tools = compat
        .children
        .as_ref()?
        .iter()
        .find(|entry| entry.key.text == "compat_tools" && entry.children.is_some())?;
    let first = tools.children.as_ref()?.first()?;
    let name = first.key.text.clone();
    let display = first
        .children
        .as_ref()
        .and_then(|children| {
            children
                .iter()
                .find(|entry| entry.key.text == "display_name" && entry.value.is_some())
        })
        .and_then(|entry| entry.value.as_ref().map(|value| value.text.clone()))
        .unwrap_or_else(|| name.clone());
    Some((name, display))
}

fn folder_name(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Proton".to_string())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Stock,
    Steam,
    Tools,
    System,
    Custom,
}

#[derive(Clone, Debug)]
struct Found {
    dir: PathBuf,
    origin: Origin,
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub candidates: Vec<Candidate>,
    pub hint: Option<&'static str>,
}

static FOUND: Mutex<Option<Arc<Vec<Found>>>> = Mutex::new(None);
static CUSTOM: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    !needle.is_empty()
        && haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn is_own_copy(name: &str) -> bool {
    name.eq_ignore_ascii_case(paths::PATCHED_NAME)
        || name.eq_ignore_ascii_case(paths::LEGACY_PATCHED_NAME)
        || name
            .get(name.len().saturating_sub(SUFFIX.len())..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(SUFFIX))
}

fn is_experimental(dir: &Path) -> bool {
    if !dir.join("proton").is_file() {
        return false;
    }
    match fs::read_to_string(dir.join("version")) {
        Ok(text) => contains_ascii_case_insensitive(&text, "experimental"),
        Err(_) => {
            let name = folder_name(dir);
            contains_ascii_case_insensitive(&name, "experimental")
                && !contains_ascii_case_insensitive(&name, "-ge")
                && !contains_ascii_case_insensitive(&name, "ge-proton")
        }
    }
}

pub fn is_proton_dir(dir: &Path) -> bool {
    dir.join("proton").is_file()
        && (dir.join("files").is_dir()
            || dir.join("dist").is_dir()
            || dir.join("version").is_file()
            || dir.join("compatibilitytool.vdf").is_file())
}

fn collect(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    if is_proton_dir(dir) {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    for child in children {
        collect(&child, depth - 1, out);
    }
}

pub fn count_in(dir: &Path) -> usize {
    let mut out = Vec::new();
    collect(dir, 2, &mut out);
    out.len()
}

fn take_digits(iter: &mut Peekable<Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(ch) = iter.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        out.push(ch);
        iter.next();
    }
    out
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let xs = take_digits(&mut left);
                    let ys = take_digits(&mut right);
                    let xt = xs.trim_start_matches('0');
                    let yt = ys.trim_start_matches('0');
                    let order = xt.len().cmp(&yt.len()).then_with(|| xt.cmp(yt));
                    if order != Ordering::Equal {
                        return order;
                    }
                } else {
                    let order = x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase());
                    if order != Ordering::Equal {
                        return order;
                    }
                    left.next();
                    right.next();
                }
            }
        }
    }
}

fn push_found(list: &mut Vec<Found>, dir: PathBuf, origin: Origin) {
    let real = fs::canonicalize(&dir).unwrap_or(dir);
    if !list.iter().any(|known| known.dir == real) {
        list.push(Found { dir: real, origin });
    }
}

fn extra_tool_dirs() -> Vec<PathBuf> {
    let home = paths::home();
    let mut list: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/steam/compatibilitytools.d"),
        PathBuf::from("/usr/local/share/steam/compatibilitytools.d"),
        PathBuf::from("/usr/lib/steam/compatibilitytools.d"),
        PathBuf::from("/usr/lib64/steam/compatibilitytools.d"),
        PathBuf::from("/opt/steam/compatibilitytools.d"),
        PathBuf::from("/opt/proton"),
        home.join(".config/heroic/tools/proton"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/tools/proton"),
        home.join(".local/share/lutris/runners/proton"),
        home.join(".var/app/net.lutris.Lutris/data/lutris/runners/proton"),
    ];
    if let Some(value) = std::env::var_os("STEAM_EXTRA_COMPAT_TOOLS_PATHS") {
        for part in std::env::split_paths(&value) {
            if part.is_absolute() {
                list.push(part);
            }
        }
    }
    list
}

fn scan_disk() -> Vec<Found> {
    let layout = steam::layout();
    let mut found: Vec<Found> = Vec::new();
    let mut batch: Vec<PathBuf> = Vec::new();

    for library in &layout.libraries {
        batch.clear();
        collect(&library.join("steamapps/common"), 1, &mut batch);
        for dir in batch.drain(..) {
            let stock = folder_name(&dir) == paths::STOCK_FOLDER || is_experimental(&dir);
            let origin = if stock { Origin::Stock } else { Origin::Steam };
            push_found(&mut found, dir, origin);
        }
    }

    let mut tool_dirs: Vec<(PathBuf, Origin)> = Vec::new();
    for root in &layout.roots {
        tool_dirs.push((root.join("compatibilitytools.d"), Origin::Tools));
    }
    tool_dirs.push((layout.root.join("compatibilitytools.d"), Origin::Tools));
    for dir in extra_tool_dirs() {
        tool_dirs.push((dir, Origin::System));
    }
    for (dir, origin) in tool_dirs {
        batch.clear();
        collect(&dir, 1, &mut batch);
        for item in batch.drain(..) {
            if is_own_copy(&folder_name(&item)) {
                continue;
            }
            push_found(&mut found, item, origin);
        }
    }

    let custom = CUSTOM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    for dir in custom {
        batch.clear();
        collect(&dir, 2, &mut batch);
        for item in batch.drain(..) {
            push_found(&mut found, item, Origin::Custom);
        }
    }

    found.sort_by(|a, b| {
        a.origin
            .cmp(&b.origin)
            .then_with(|| natural_cmp(&folder_name(&b.dir), &folder_name(&a.dir)))
    });
    found
}

pub fn set_custom(list: &[String]) {
    let mut cleaned: Vec<PathBuf> = Vec::new();
    for item in list {
        let path = PathBuf::from(item);
        if path.is_absolute() && !cleaned.contains(&path) {
            cleaned.push(path);
        }
    }
    *CUSTOM.lock().unwrap_or_else(|poison| poison.into_inner()) = cleaned;
    *FOUND.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
}

fn discover() -> Arc<Vec<Found>> {
    let mut guard = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    Arc::clone(guard.get_or_insert_with(|| Arc::new(scan_disk())))
}

fn rescan() -> Arc<Vec<Found>> {
    steam::refresh();
    let fresh = Arc::new(scan_disk());
    let mut guard = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    *guard = Some(Arc::clone(&fresh));
    fresh
}

fn default_of(list: &[Found]) -> Option<&Found> {
    list.iter()
        .find(|item| item.origin == Origin::Stock)
        .or_else(|| list.first())
}

fn default_dir() -> PathBuf {
    let list = discover();
    default_of(&list)
        .map(|item| item.dir.clone())
        .unwrap_or_else(|| paths::common().join(paths::STOCK_FOLDER))
}

fn target_from(folder: &Path) -> Target {
    let folder_name = folder_name(folder);
    let is_stock = folder_name == paths::STOCK_FOLDER;
    let label = if is_stock {
        paths::DEFAULT_LABEL.to_string()
    } else {
        folder_name.clone()
    };
    let (name, display) = identity(folder).unwrap_or_else(|| {
        if is_stock {
            (STOCK_TOOL_NAME.to_string(), label.clone())
        } else {
            (folder_name.clone(), label.clone())
        }
    });
    Target {
        source: folder.to_path_buf(),
        patched: folder.to_path_buf(),
        name,
        display,
    }
}

pub fn target_for(source_id: &str) -> Target {
    let id = normalize_id(source_id);
    let folder = match id.strip_prefix("path:") {
        Some(dir) => PathBuf::from(dir),
        None => default_dir(),
    };
    target_from(&folder)
}

pub fn normalize_id(id: &str) -> String {
    let Some(dirname) = id.strip_prefix("path:") else {
        return String::new();
    };
    if !dirname.starts_with('/') {
        return String::new();
    }
    let raw = PathBuf::from(dirname.trim_end_matches('/'));
    let list = discover();
    let dir = match list.iter().find(|item| same_dir(&item.dir, &raw)) {
        Some(hit) => hit.dir.clone(),
        None => {
            if is_proton_dir(&raw) {
                raw
            } else {
                return String::new();
            }
        }
    };
    if same_dir(&dir, &default_dir()) {
        return String::new();
    }
    let real = fs::canonicalize(&dir).unwrap_or(dir);
    format!("path:{}", real.to_string_lossy())
}

pub fn is_listed(id: &str) -> bool {
    let Some(dirname) = id.strip_prefix("path:") else {
        return id.is_empty();
    };
    let raw = PathBuf::from(dirname);
    discover().iter().any(|item| same_dir(&item.dir, &raw))
}

fn candidate_for_folder(folder: &Path, id: String, stock: bool) -> Candidate {
    let readiness = match patch_state(folder) {
        PatchState::Missing => {
            if folder.join("proton").is_file() {
                Readiness::Broken
            } else {
                Readiness::Missing
            }
        }
        _ => Readiness::Ready,
    };
    Candidate {
        id,
        name: folder_name(folder),
        path: folder.to_path_buf(),
        readiness,
        stock,
    }
}

pub fn listing() -> Listing {
    let found = rescan();
    let layout = steam::layout();
    let (default, default_stock) = match default_of(&found) {
        Some(item) => (item.dir.clone(), item.origin == Origin::Stock),
        None => (paths::common().join(paths::STOCK_FOLDER), true),
    };
    let mut candidates = vec![candidate_for_folder(&default, String::new(), default_stock)];
    for item in found.iter() {
        if item.dir == default {
            continue;
        }
        let id = format!("path:{}", item.dir.to_string_lossy());
        candidates.push(candidate_for_folder(&item.dir, id, false));
    }
    let denied = matches!(
        fs::read_dir(layout.root.join("steamapps")),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied
    );
    let hint = if found.is_empty() {
        if !layout.found {
            Some("settings.proton.no_steam")
        } else if denied {
            Some("settings.proton.denied")
        } else {
            Some("settings.proton.none")
        }
    } else if denied {
        Some("settings.proton.denied")
    } else {
        None
    };
    Listing { candidates, hint }
}

pub fn check_source(src: &Path) -> Result<()> {
    if !src.is_dir() {
        return Err(Error(tf(
            "err.proton_missing",
            &[("name", &folder_name(src))],
        )));
    }
    let Some(so) = find_ntdll(src) else {
        return Err(Error(t("pp.err.no_ntdll_src")));
    };
    let occurrences = scan(&fs::read(so)?).old;
    if occurrences != 1 {
        return Err(Error(tf(
            "pp.err.pattern",
            &[("count", &occurrences.to_string())],
        )));
    }
    Ok(())
}

fn apply_patch(so: &Path) -> Result<()> {
    let data = fs::read(so)?;
    let found = scan(&data);
    let at = match found.old_at {
        Some(at) if found.old == 1 => at,
        _ => {
            return Err(Error(tf(
                "pp.err.pattern",
                &[("count", &found.old.to_string())],
            )));
        }
    };
    let mode = fs::metadata(so)?.permissions().mode();
    let mut patched = data;
    patched[at..at + NEW.len()].copy_from_slice(NEW);
    let tmp = so.with_extension("so.Majelinux.tmp");
    fs::write(&tmp, &patched)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, so)?;
    let check = scan(&fs::read(so)?);
    if check.new == 0 || check.old != 0 {
        return Err(Error(t("pp.err.verify")));
    }
    Ok(())
}

pub fn ensure_patched(target: &Target) -> Result<()> {
    let src = &target.source;
    let Some(so) = find_ntdll(src) else {
        return Err(Error(t("pp.err.no_ntdll_src")));
    };
    match patch_state(src) {
        PatchState::Patched => {
            log::line(&t("pp.log.already"));
            return Ok(());
        }
        PatchState::Modern => {
            log::line(&tf("pp.log.modern", &[("name", &folder_name(src))]));
            return Ok(());
        }
        PatchState::Unknown => {
            log::line(&tf("pp.log.as_is", &[("name", &folder_name(src))]));
            return Ok(());
        }
        PatchState::Missing => {
            return Err(Error(t("pp.err.no_ntdll_src")));
        }
        PatchState::Patchable => {}
    }
    if !dir_writable(src) {
        log::line(&tf("pp.log.readonly", &[("name", &folder_name(src))]));
        return Ok(());
    }
    check_source(src)?;
    backup_original(src, &so)?;
    apply_patch(&so)?;
    log::line(&t("pp.log.patched"));
    Ok(())
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".Majelinux-write-test");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn backup_original(src: &Path, so: &Path) -> Result<()> {
    fs::create_dir_all(paths::state())?;
    let path = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    let mut hasher = crate::core::sha256::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let key = crate::core::sha256::hex(&hasher.finish());
    let dir = paths::state().join("proton-backup").join(key);
    fs::create_dir_all(&dir)?;
    let data = fs::read(so)?;
    let backup = dir.join("ntdll.so");
    let same = fs::read(&backup).is_ok_and(|old| old == data);
    if !same {
        fs::write(&backup, &data)?;
        fs::write(dir.join("source.txt"), path.to_string_lossy().as_bytes())?;
        log::line(&tf("pp.log.backup", &[("path", &backup.to_string_lossy())]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_for(bytes: &[u8]) -> PatchState {
        let dir = std::env::temp_dir().join(format!(
            "Majelinux-state-{}-{}",
            std::process::id(),
            crate::core::util::unique_suffix()
        ));
        let so = dir.join("files/lib64/wine/x86_64-unix/ntdll.so");
        fs::create_dir_all(so.parent().unwrap()).unwrap();
        fs::write(&so, bytes).unwrap();
        let state = patch_state(&dir);
        let _ = fs::remove_dir_all(&dir);
        state
    }

    #[test]
    fn classifies_states() {
        assert_eq!(state_for(OLD), PatchState::Patchable);
        assert_eq!(state_for(NEW), PatchState::Patched);
        assert_eq!(state_for(MODERN), PatchState::Modern);
        assert_eq!(state_for(b"nothing"), PatchState::Unknown);
    }

    #[test]
    fn finds_pattern_inside_noise() {
        let mut data = vec![0x9d; 40];
        data.extend_from_slice(OLD);
        data.extend_from_slice(&[0x48, 0xcf, 0x9d]);
        let found = scan(&data);
        assert_eq!(found.old, 1);
        assert_eq!(found.old_at, Some(40));
        assert_eq!(found.state(), PatchState::Patchable);
    }

    #[test]
    fn patch_lengths_are_stable() {
        assert_eq!(OLD.len(), 17);
        assert_eq!(NEW.len(), 17);
        assert_eq!(MODERN.len(), 17);
        assert_eq!(&OLD[ANCHOR_OFFSET..], &ANCHOR[..]);
        assert_eq!(&NEW[ANCHOR_OFFSET..], &ANCHOR[..]);
        assert_eq!(&MODERN[ANCHOR_OFFSET..], &ANCHOR[..]);
    }

    #[test]
    fn applies_patch_in_place() {
        let dir = std::env::temp_dir().join(format!(
            "Majelinux-apply-{}-{}",
            std::process::id(),
            crate::core::util::unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        let so = dir.join("ntdll.so");
        let mut data = vec![0u8; 8];
        data.extend_from_slice(OLD);
        fs::write(&so, &data).unwrap();
        apply_patch(&so).unwrap();
        let after = fs::read(&so).unwrap();
        assert_eq!(&after[8..], NEW);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_experimental_by_version_file() {
        let dir = std::env::temp_dir().join(format!(
            "Majelinux-exp-{}-{}",
            std::process::id(),
            crate::core::util::unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proton"), b"#!/bin/sh\n").unwrap();
        fs::write(
            dir.join("version"),
            b"1736352321 experimental-10.0-20250107\n",
        )
        .unwrap();
        assert!(is_experimental(&dir));
        fs::write(dir.join("version"), b"1736352321 GE-Proton9-20\n").unwrap();
        assert!(!is_experimental(&dir));
        fs::write(dir.join("version"), b"1736352321 proton-9.0-4\n").unwrap();
        assert!(!is_experimental(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_relative_ids() {
        assert_eq!(normalize_id("path:relative"), "");
        assert_eq!(normalize_id(""), "");
    }

    #[test]
    fn recognises_own_copies() {
        assert!(is_own_copy("proton-exp-Majelinux"));
        assert!(is_own_copy("proton-exp-mjlx"));
        assert!(is_own_copy("proton---experimental-Majelinux"));
        assert!(!is_own_copy("proton-experimental-bleeding"));
    }

    #[test]
    fn orders_versions_naturally() {
        assert_eq!(natural_cmp("Proton 9.0", "Proton 10.0"), Ordering::Less);
        assert_eq!(natural_cmp("GE-Proton9-2", "GE-Proton9-10"), Ordering::Less);
        assert_eq!(natural_cmp("proton-a", "Proton-A"), Ordering::Equal);
    }

    #[test]
    fn collects_nested_protons() {
        let root = std::env::temp_dir().join(format!(
            "Majelinux-collect-{}-{}",
            std::process::id(),
            crate::core::util::unique_suffix()
        ));
        let first = root.join("tools/GE-Proton9-1");
        let second = root.join("tools/Wrapper/Inner");
        for dir in [&first, &second] {
            fs::create_dir_all(dir.join("files")).unwrap();
            fs::write(dir.join("proton"), b"").unwrap();
        }
        fs::create_dir_all(root.join("tools/NotProton")).unwrap();
        assert_eq!(count_in(&root.join("tools")), 2);
        assert!(is_proton_dir(&first));
        assert!(!is_proton_dir(&root.join("tools/NotProton")));
        let _ = fs::remove_dir_all(&root);
    }
}
