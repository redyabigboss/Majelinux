use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::core::{paths, vdf};

#[derive(Clone, Debug)]
pub struct Layout {
    pub root: PathBuf,
    pub roots: Vec<PathBuf>,
    pub libraries: Vec<PathBuf>,
    pub game: PathBuf,
    pub found: bool,
}

static LAYOUT: Mutex<Option<Arc<Layout>>> = Mutex::new(None);

fn data_home() -> PathBuf {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() && Path::new(&value).is_absolute() => PathBuf::from(value),
        _ => paths::home().join(".local/share"),
    }
}

fn root_candidates() -> Vec<PathBuf> {
    let home = paths::home();
    let flatpak = home.join(".var/app/com.valvesoftware.Steam");
    let snap = home.join("snap/steam/common");
    let mut list = Vec::new();
    if let Some(value) = std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH") {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            list.push(path);
        }
    }
    list.extend([
        home.join(".steam/steam"),
        data_home().join("Steam"),
        home.join(".local/share/Steam"),
        home.join(".steam/root"),
        home.join(".steam/debian-installation"),
        flatpak.join(".local/share/Steam"),
        flatpak.join("data/Steam"),
        flatpak.join(".steam/steam"),
        flatpak.join(".steam/root"),
        snap.join(".local/share/Steam"),
        snap.join(".steam/steam"),
    ]);
    list
}

fn has_game(library: &Path) -> bool {
    let apps = library.join("steamapps");
    apps.join(format!("appmanifest_{}.acf", paths::APP_ID_STR))
        .is_file()
        || apps.join("compatdata").join(paths::APP_ID_STR).is_dir()
        || apps.join("common").join(paths::GAME_DIR).is_dir()
}

fn score(root: &Path) -> u8 {
    let mut points = 1;
    if has_game(root) {
        points += 4;
    }
    if root.join("config/config.vdf").is_file() {
        points += 2;
    }
    points
}

fn parse_libraries(text: &str) -> Vec<PathBuf> {
    let Ok(entries) = vdf::parse(text) else {
        return Vec::new();
    };
    let Some(top) = entries
        .into_iter()
        .find(|entry| entry.key.text.eq_ignore_ascii_case("libraryfolders"))
    else {
        return Vec::new();
    };
    let Some(children) = top.children else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for child in children {
        let raw = match (child.children, child.value) {
            (Some(inner), _) => inner
                .into_iter()
                .find(|entry| entry.key.text == "path")
                .and_then(|entry| entry.value)
                .map(|value| value.text),
            (None, Some(value)) if child.key.text.bytes().all(|byte| byte.is_ascii_digit()) => {
                Some(value.text)
            }
            _ => None,
        };
        if let Some(raw) = raw {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                out.push(path);
            }
        }
    }
    out
}

fn compute() -> Layout {
    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in root_candidates() {
        let usable = dir.join("steamapps").is_dir()
            || dir.join("compatibilitytools.d").is_dir()
            || dir.join("config/config.vdf").is_file();
        if !usable {
            continue;
        }
        let real = fs::canonicalize(&dir).unwrap_or(dir);
        if !roots.contains(&real) {
            roots.push(real);
        }
    }
    let mut best: Option<(u8, PathBuf)> = None;
    for dir in &roots {
        if !dir.join("steamapps").is_dir() {
            continue;
        }
        let points = score(dir);
        if best.as_ref().is_none_or(|(top, _)| points > *top) {
            best = Some((points, dir.clone()));
        }
    }
    let found = best.is_some();
    let root = best
        .map(|(_, dir)| dir)
        .unwrap_or_else(|| data_home().join("Steam"));

    let mut sources: Vec<PathBuf> = vec![root.clone()];
    for dir in &roots {
        if !sources.contains(dir) {
            sources.push(dir.clone());
        }
    }
    let mut libraries: Vec<PathBuf> = vec![root.clone()];
    for source in &sources {
        if source.join("steamapps").is_dir() && !libraries.contains(source) {
            libraries.push(source.clone());
        }
        let Ok(text) = fs::read_to_string(source.join("steamapps/libraryfolders.vdf")) else {
            continue;
        };
        for path in parse_libraries(&text) {
            let real = fs::canonicalize(&path).unwrap_or(path);
            if real.join("steamapps").is_dir() && !libraries.contains(&real) {
                libraries.push(real);
            }
        }
    }
    let game = libraries
        .iter()
        .find(|library| has_game(library))
        .cloned()
        .unwrap_or_else(|| root.clone());
    Layout {
        root,
        roots,
        libraries,
        game,
        found,
    }
}

pub fn layout() -> Arc<Layout> {
    let mut guard = LAYOUT.lock().unwrap_or_else(|poison| poison.into_inner());
    Arc::clone(guard.get_or_insert_with(|| Arc::new(compute())))
}

pub fn refresh() -> Arc<Layout> {
    let fresh = Arc::new(compute());
    let mut guard = LAYOUT.lock().unwrap_or_else(|poison| poison.into_inner());
    *guard = Some(Arc::clone(&fresh));
    fresh
}

pub fn root() -> PathBuf {
    layout().root.clone()
}

pub fn game_library() -> PathBuf {
    layout().game.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_libraryfolders_format() {
        let text = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games/SteamLibrary\"\n\t}\n}\n";
        let list = parse_libraries(text);
        assert_eq!(
            list,
            vec![
                PathBuf::from("/home/u/.local/share/Steam"),
                PathBuf::from("/mnt/games/SteamLibrary")
            ]
        );
    }

    #[test]
    fn parses_old_libraryfolders_format() {
        let text = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"1\"\n\t\"1\"\t\t\"/mnt/old/Steam\"\n}\n";
        assert_eq!(parse_libraries(text), vec![PathBuf::from("/mnt/old/Steam")]);
    }

    #[test]
    fn ignores_relative_and_broken_input() {
        assert!(parse_libraries("garbage {").is_empty());
        let text =
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"relative/dir\"\n\t}\n}\n";
        assert!(parse_libraries(text).is_empty());
    }
}
