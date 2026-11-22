use std::fs;
use std::path::Path;

use crate::core::vdf::{self, Entry, Tok};
use crate::core::{Error, Result, paths};

pub const ENV: &str = "PROTON_ENABLE_WAYLAND=0";

fn child<'a>(items: &'a [Entry], key: &str) -> Option<&'a Entry> {
    items
        .iter()
        .find(|entry| entry.children.is_some() && entry.key.text == key)
}

fn value<'a>(items: &'a [Entry], key: &str) -> Option<&'a Tok> {
    items
        .iter()
        .find(|entry| entry.value.is_some() && entry.key.text == key)
        .and_then(|entry| entry.value.as_ref())
}

fn nested<'a>(items: &'a [Entry], keys: &[&str]) -> Option<&'a Entry> {
    let mut current = items;
    let mut found = None;
    for key in keys {
        let node = child(current, key)?;
        current = node.children.as_deref().unwrap_or(&[]);
        found = Some(node);
    }
    found
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn line_start(text: &str, close: usize) -> Result<(usize, String)> {
    let start = text[..close.min(text.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let indent = &text[start..close.min(text.len())];
    if !indent.trim().is_empty() {
        return Err(Error("unexpected layout near a closing brace".into()));
    }
    Ok((start, indent.to_string()))
}

fn present(value: &str) -> bool {
    value.split_whitespace().any(|part| part == ENV)
}

fn merged(value: &str) -> String {
    let current = value.trim();
    if current.is_empty() {
        ENV.to_string()
    } else {
        format!("{ENV} {current}")
    }
}

fn write(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let entries = vdf::parse(&text)?;
    let Some(apps) = nested(
        &entries,
        &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"],
    ) else {
        return Ok(());
    };
    let Some(app) = child(apps.children.as_deref().unwrap_or(&[]), paths::APP_ID_STR) else {
        return Ok(());
    };
    let children = app.children.as_deref().unwrap_or(&[]);
    let existing = value(children, "LaunchOptions");
    if existing.is_some_and(|token| present(&token.text)) {
        return Ok(());
    }
    let new_value = merged(existing.map(|token| token.text.as_str()).unwrap_or(""));
    let (start, end, replacement) = match existing {
        Some(token) => (
            token.start,
            token.end,
            format!("\"{}\"", escape(&new_value)),
        ),
        None => {
            let close = app
                .close
                .as_ref()
                .ok_or_else(|| Error("missing app closing brace".into()))?;
            let (start, indent) = line_start(&text, close.start)?;
            (
                start,
                start,
                format!(
                    "{indent}\t\"LaunchOptions\"\t\t\"{}\"\n",
                    escape(&new_value)
                ),
            )
        }
    };
    let mut updated = text.clone();
    updated.replace_range(start..end, &replacement);
    vdf::parse(&updated)?;
    let backup = path.with_file_name("localconfig.vdf.Majelinux.bak");
    if !backup.exists() {
        fs::copy(path, &backup)?;
    }
    let mode = fs::metadata(path).ok().map(|meta| meta.permissions());
    let tmp = path.with_file_name("localconfig.vdf.Majelinux.tmp");
    let result = (|| -> Result<()> {
        fs::write(&tmp, updated.as_bytes())?;
        if let Some(mode) = mode {
            fs::set_permissions(&tmp, mode)?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    let check = fs::read_to_string(path)?;
    let entries = vdf::parse(&check)?;
    let verified = nested(
        &entries,
        &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"],
    )
    .and_then(|apps| child(apps.children.as_deref().unwrap_or(&[]), paths::APP_ID_STR))
    .and_then(|app| value(app.children.as_deref().unwrap_or(&[]), "LaunchOptions"))
    .is_some_and(|token| present(&token.text));
    if !verified {
        let _ = fs::copy(&backup, path);
        return Err(Error("launch options verification failed".into()));
    }
    Ok(())
}

pub fn ensure() -> Result<()> {
    let dir = paths::steam().join("userdata");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let config = entry.path().join("config/localconfig.vdf");
        if config.is_file() {
            write(&config)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_missing_launch_options() {
        let dir = std::env::temp_dir().join(format!("Majelinux-opts-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("localconfig.vdf");
        let text = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"271590\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"Playtime\"\t\t\"1\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";
        fs::write(&path, text).unwrap();
        write(&path).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("\"LaunchOptions\"\t\t\"PROTON_ENABLE_WAYLAND=0\""));
        let entries = vdf::parse(&updated).unwrap();
        assert!(
            nested(
                &entries,
                &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
            )
            .is_some()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_existing_options() {
        let dir = std::env::temp_dir().join(format!("Majelinux-opts2-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("localconfig.vdf");
        let text = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"271590\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"gamemoderun %command%\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";
        fs::write(&path, text).unwrap();
        write(&path).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("\"PROTON_ENABLE_WAYLAND=0 gamemoderun %command%\""));
        let _ = fs::remove_dir_all(&dir);
    }
}
