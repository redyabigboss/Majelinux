use std::fs;
use std::path::Path;

use crate::core::paths;
use crate::core::{Error, Result};
use crate::i18n::tf;

const COMPAT_PATH: [&str; 5] = [
    "InstallConfigStore",
    "Software",
    "Valve",
    "Steam",
    "CompatToolMapping",
];
const PRIORITY: &str = "250";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokKind {
    Str,
    Open,
    Close,
}

#[derive(Clone, Debug)]
pub struct Tok {
    pub kind: TokKind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub key: Tok,
    pub value: Option<Tok>,
    pub children: Option<Vec<Entry>>,
    pub close: Option<Tok>,
}

fn fail<T>(detail: impl Into<String>) -> Result<T> {
    Err(Error(tf("err.steam_config", &[("err", &detail.into())])))
}

fn unescape_into(text: &str, index: usize, out: &mut String) -> usize {
    if index >= text.len() {
        return index;
    }
    let ch = text[index..].chars().next().unwrap();
    match ch {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        '\\' => out.push('\\'),
        '"' => out.push('"'),
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    index + ch.len_utf8()
}

pub fn tokenize(text: &str) -> Result<Vec<Tok>> {
    let bytes = text.as_bytes();
    let mut toks = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte.is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if byte == b'/' && pos + 1 < bytes.len() && bytes[pos + 1] == b'/' {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        if byte == b'"' {
            let start = pos;
            pos += 1;
            let mut value = String::new();
            loop {
                if pos >= bytes.len() {
                    return fail(format!("unterminated string at offset {start}"));
                }
                match bytes[pos] {
                    b'\\' => pos = unescape_into(text, pos + 1, &mut value),
                    b'"' => {
                        pos += 1;
                        break;
                    }
                    _ => {
                        let ch = text[pos..].chars().next().unwrap();
                        value.push(ch);
                        pos += ch.len_utf8();
                    }
                }
            }
            toks.push(Tok {
                kind: TokKind::Str,
                text: value,
                start,
                end: pos,
            });
            continue;
        }
        if byte == b'{' || byte == b'}' {
            toks.push(Tok {
                kind: if byte == b'{' {
                    TokKind::Open
                } else {
                    TokKind::Close
                },
                text: (byte as char).to_string(),
                start: pos,
                end: pos + 1,
            });
            pos += 1;
            continue;
        }
        return fail(format!("unexpected character at offset {pos}"));
    }
    Ok(toks)
}

fn parse_entries(
    tokens: &mut std::vec::IntoIter<Tok>,
    top: bool,
) -> Result<(Vec<Entry>, Option<Tok>)> {
    let mut items = Vec::new();
    loop {
        let Some(tok) = tokens.next() else {
            return if top {
                Ok((items, None))
            } else {
                fail("unexpected end of file")
            };
        };
        match tok.kind {
            TokKind::Close => {
                if top {
                    return fail("unbalanced closing brace");
                }
                return Ok((items, Some(tok)));
            }
            TokKind::Open => return fail("unexpected opening brace"),
            TokKind::Str => {
                let Some(next) = tokens.next() else {
                    return fail("unexpected end of file");
                };
                match next.kind {
                    TokKind::Open => {
                        let (children, close) = parse_entries(tokens, false)?;
                        items.push(Entry {
                            key: tok,
                            value: None,
                            children: Some(children),
                            close,
                        });
                    }
                    TokKind::Str => items.push(Entry {
                        key: tok,
                        value: Some(next),
                        children: None,
                        close: None,
                    }),
                    TokKind::Close => return fail("unexpected closing brace"),
                }
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Entry>> {
    let tokens = tokenize(text)?;
    let mut tokens = tokens.into_iter();
    let (items, _) = parse_entries(&mut tokens, true)?;
    Ok(items)
}

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

fn chain(items: &[Entry]) -> Vec<&Entry> {
    let mut chain = Vec::new();
    let mut current = items;
    for key in COMPAT_PATH {
        let Some(node) = child(current, key) else {
            break;
        };
        chain.push(node);
        current = node.children.as_deref().unwrap_or(&[]);
    }
    chain
}

fn read(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .map_err(|error| Error(tf("err.steam_config", &[("err", &error.to_string())])))?;
    String::from_utf8(bytes).map_err(|_| {
        Error(tf(
            "err.steam_config",
            &[("err", "config.vdf is not valid UTF-8")],
        ))
    })
}

pub fn compat_tool_at(path: &Path, appid: u32) -> Result<Option<String>> {
    let text = read(path)?;
    let entries = parse(&text)?;
    let chain = chain(&entries);
    if chain.len() < COMPAT_PATH.len() {
        return Ok(None);
    }
    let Some(app) = child(
        chain[4].children.as_deref().unwrap_or(&[]),
        &appid.to_string(),
    ) else {
        return Ok(None);
    };
    Ok(value(app.children.as_deref().unwrap_or(&[]), "name").map(|tok| tok.text.clone()))
}

pub fn compat_tool(appid: u32) -> Result<Option<String>> {
    compat_tool_at(&paths::config_vdf(), appid)
}

fn line_start(text: &str, close: usize) -> Result<(usize, String)> {
    let start = text[..close.min(text.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let indent = &text[start..close.min(text.len())];
    if !indent.trim().is_empty() {
        return fail("unexpected layout near a closing brace");
    }
    Ok((start, indent.to_string()))
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn app_text(appid: u32, name: &str, indent: &str) -> String {
    format!(
        "{indent}\"{appid}\"\n\
         {indent}{{\n\
         {indent}\t\"name\"\t\t\"{name}\"\n\
         {indent}\t\"config\"\t\t\"\"\n\
         {indent}\t\"priority\"\t\t\"{PRIORITY}\"\n\
         {indent}}}\n",
        name = escape(name)
    )
}

fn apply_edits(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut result = text.to_string();
    for (start, end, new) in edits {
        result.replace_range(start..end, &new);
    }
    result
}

pub fn set_compat_tool_at(
    config: &Path,
    backup: &Path,
    tmp: &Path,
    appid: u32,
    name: &str,
) -> Result<()> {
    let text = read(config)?;
    let entries = parse(&text)?;
    let chain = chain(&entries);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    if chain.len() == COMPAT_PATH.len() {
        let mapping = chain[4];
        let mapping_children = mapping.children.as_deref().unwrap_or(&[]);
        let app_key = appid.to_string();
        if let Some(app) = child(mapping_children, &app_key) {
            let app_children = app.children.as_deref().unwrap_or(&[]);
            if let Some(current) = value(app_children, "name") {
                edits.push((current.start, current.end, format!("\"{}\"", escape(name))));
            } else {
                let close = app
                    .close
                    .as_ref()
                    .ok_or_else(|| Error("missing app closing brace".into()))?;
                let (start, indent) = line_start(&text, close.start)?;
                edits.push((
                    start,
                    start,
                    format!("{indent}\t\"name\"\t\t\"{}\"\n", escape(name)),
                ));
            }
        } else {
            let close = mapping
                .close
                .as_ref()
                .ok_or_else(|| Error("missing mapping closing brace".into()))?;
            let (start, indent) = line_start(&text, close.start)?;
            let block = app_text(appid, name, &(indent + "\t"));
            edits.push((start, start, block));
        }
    } else {
        if chain.len() < COMPAT_PATH.len() - 1 {
            return fail("Steam section not found in config.vdf");
        }
        let steam = chain[chain.len() - 1];
        let close = steam
            .close
            .as_ref()
            .ok_or_else(|| Error("missing Steam closing brace".into()))?;
        let (start, indent) = line_start(&text, close.start)?;
        let inner = indent + "\t";
        let block = format!(
            "{inner}\"CompatToolMapping\"\n{inner}{{\n{}{inner}}}\n",
            app_text(appid, name, &(inner.clone() + "\t"))
        );
        edits.push((start, start, block));
    }

    let new_text = apply_edits(&text, edits);
    parse(&new_text)?;

    if !backup.exists() {
        fs::copy(config, backup)?;
    }
    let mode = fs::metadata(config).ok().map(|meta| meta.permissions());
    let write_result = (|| -> Result<()> {
        fs::write(tmp, new_text.as_bytes())?;
        if let Some(mode) = mode {
            fs::set_permissions(tmp, mode)?;
        }
        fs::rename(tmp, config)?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(tmp);
        return Err(error);
    }
    if compat_tool_at(config, appid)?.as_deref() != Some(name) {
        let _ = fs::copy(backup, config);
        return fail("verification failed, config restored from backup");
    }
    Ok(())
}

pub fn set_compat_tool(appid: u32, name: &str) -> Result<()> {
    set_compat_tool_at(
        &paths::config_vdf(),
        &paths::backup_vdf(),
        &paths::config_vdf().with_file_name("config.vdf.Majelinux.tmp"),
        appid,
        name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(mapping: bool, app: bool) -> String {
        let mut text = String::from(
            "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n",
        );
        if mapping {
            text.push_str("\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n");
            if app {
                text.push_str(
                    "\t\t\t\t\t\"271590\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"proton_experimental\"\n\t\t\t\t\t\t\"config\"\t\t\"\"\n\t\t\t\t\t\t\"priority\"\t\t\"250\"\n\t\t\t\t\t}\n",
                );
            }
            text.push_str("\t\t\t\t}\n");
        }
        text.push_str("\t\t\t}\n\t\t}\n\t}\n}\n");
        text
    }

    struct Temp {
        dir: std::path::PathBuf,
        config: std::path::PathBuf,
        backup: std::path::PathBuf,
        tmp: std::path::PathBuf,
    }

    impl Temp {
        fn new(contents: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "Majelinux-vdf-{}-{}",
                std::process::id(),
                crate::core::util::unique_suffix()
            ));
            fs::create_dir_all(&dir).unwrap();
            let config = dir.join("config.vdf");
            fs::write(&config, contents).unwrap();
            Self {
                backup: dir.join("config.vdf.Majelinux.bak"),
                tmp: dir.join("config.vdf.Majelinux.tmp"),
                config,
                dir,
            }
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn reads_existing_mapping() {
        let temp = Temp::new(&sample(true, true));
        assert_eq!(
            compat_tool_at(&temp.config, 271590).unwrap().as_deref(),
            Some("proton_experimental")
        );
    }

    #[test]
    fn replaces_name_in_place() {
        let temp = Temp::new(&sample(true, true));
        set_compat_tool_at(
            &temp.config,
            &temp.backup,
            &temp.tmp,
            271590,
            "Proton-Exp-Majelinux",
        )
        .unwrap();
        assert_eq!(
            compat_tool_at(&temp.config, 271590).unwrap().as_deref(),
            Some("Proton-Exp-Majelinux")
        );
        let text = fs::read_to_string(&temp.config).unwrap();
        assert!(text.starts_with("\"InstallConfigStore\""));
        assert!(text.contains("\t\t\t\t\t\t\"name\"\t\t\"Proton-Exp-Majelinux\""));
    }

    #[test]
    fn inserts_app_block() {
        let temp = Temp::new(&sample(true, false));
        set_compat_tool_at(
            &temp.config,
            &temp.backup,
            &temp.tmp,
            271590,
            "Proton-Exp-Majelinux",
        )
        .unwrap();
        assert_eq!(
            compat_tool_at(&temp.config, 271590).unwrap().as_deref(),
            Some("Proton-Exp-Majelinux")
        );
        let text = fs::read_to_string(&temp.config).unwrap();
        assert!(text.contains("\"priority\"\t\t\"250\""));
    }

    #[test]
    fn creates_compat_mapping() {
        let temp = Temp::new(&sample(false, false));
        set_compat_tool_at(
            &temp.config,
            &temp.backup,
            &temp.tmp,
            271590,
            "Proton-Exp-Majelinux",
        )
        .unwrap();
        assert_eq!(
            compat_tool_at(&temp.config, 271590).unwrap().as_deref(),
            Some("Proton-Exp-Majelinux")
        );
        assert!(
            fs::read_to_string(&temp.config)
                .unwrap()
                .contains("CompatToolMapping")
        );
    }

    #[test]
    fn validates_after_edit() {
        let temp = Temp::new(&sample(true, false));
        set_compat_tool_at(
            &temp.config,
            &temp.backup,
            &temp.tmp,
            271590,
            "Proton-Exp-Majelinux",
        )
        .unwrap();
        parse(&fs::read_to_string(&temp.config).unwrap()).unwrap();
    }

    #[test]
    fn rejects_broken_file() {
        let temp = Temp::new("\"a\"\n{\n\"b\" \"c\"\n");
        assert!(compat_tool_at(&temp.config, 271590).is_err());
    }

    #[test]
    fn keeps_comments_and_other_keys() {
        let temp = Temp::new(
            "\"InstallConfigStore\"\n{\n\t// keep me\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"Other\" \"x\"\n\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n",
        );
        set_compat_tool_at(&temp.config, &temp.backup, &temp.tmp, 271590, "A").unwrap();
        let text = fs::read_to_string(&temp.config).unwrap();
        assert!(text.contains("// keep me"));
        assert!(text.contains("\"Other\" \"x\""));
    }
}
