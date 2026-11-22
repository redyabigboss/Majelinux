use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn shlex_split(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            Some('"') => {
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    match chars.peek().copied() {
                        Some(next @ ('"' | '\\' | '$' | '`')) => {
                            current.push(next);
                            chars.next();
                        }
                        _ => current.push('\\'),
                    }
                } else {
                    current.push(ch);
                }
            }
            Some(_) => unreachable!(),
            None => match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        started = true;
                    }
                }
                '\'' | '"' => {
                    quote = Some(ch);
                    started = true;
                }
                ch if ch.is_whitespace() => {
                    if started {
                        out.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                _ => {
                    current.push(ch);
                    started = true;
                }
            },
        }
    }
    if started {
        out.push(current);
    }
    out
}

pub fn shell_join(program: &Path, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(quote_shell(program.to_string_lossy().as_ref()));
    for arg in args {
        parts.push(quote_shell(arg));
    }
    parts.join(" ")
}

fn quote_shell(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/:=+@".contains(&b))
    {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || bytes[i] == b';' || bytes[i] == b'?')
            {
                i += 1;
            }
            if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub fn escape_pango(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
pub fn unique_suffix() -> String {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| std::process::id().to_string())
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if let Ok(meta) = fs::metadata(&candidate)
            && meta.is_file()
            && meta.permissions().mode() & 0o111 != 0
        {
            return Some(candidate);
        }
    }
    None
}

pub fn mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_shell_words() {
        assert_eq!(shlex_split("-disable-gpu"), vec!["-disable-gpu"]);
        assert_eq!(
            shlex_split(r#"-a "b c" 'd e' f\ g"#),
            vec!["-a", "b c", "d e", "f g"]
        );
        assert_eq!(shlex_split("  "), Vec::<String>::new());
    }

    #[test]
    fn strips_ansi() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m text"), "red text");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn escapes_pango() {
        assert_eq!(escape_pango("a<b&c>"), "a&lt;b&amp;c&gt;");
    }
}
