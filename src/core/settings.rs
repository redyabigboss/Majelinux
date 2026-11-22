use std::fs;
use std::path::Path;

use crate::core::Result;
use crate::core::paths::DEFAULT_ARGS;

#[derive(Clone, Debug)]
pub struct Settings {
    pub extra_args: String,
    pub proton: String,
    pub custom: Vec<String>,
    pub language: String,
    pub debug: bool,
    pub trim_mode: String,
    pub trim_delay: u64,
    pub protontricks_args: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            extra_args: DEFAULT_ARGS.to_string(),
            proton: String::new(),
            custom: Vec::new(),
            language: "en".to_string(),
            debug: false,
            trim_mode: "suspend".to_string(),
            trim_delay: 60,
            protontricks_args: String::new(),
        }
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut settings = Settings::default();
        let Ok(text) = fs::read_to_string(path) else {
            return settings;
        };
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            if !section.is_empty() && section != "General" {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = unquote(value);
            match key {
                "extra_args" => settings.extra_args = value,
                "proton" => settings.proton = value,
                "custom_proton" => {
                    if !value.is_empty() && !settings.custom.contains(&value) {
                        settings.custom.push(value);
                    }
                }
                "language" => {
                    settings.language = if value == "ru" { "ru" } else { "en" }.to_string()
                }
                "debug" => settings.debug = value == "true" || value == "1",
                "launcher_trim" => settings.trim_mode = value,
                "protontricks_args" => settings.protontricks_args = value,
                "launcher_trim_delay" => {
                    if let Ok(seconds) = value.parse::<u64>() {
                        settings.trim_delay = seconds;
                    }
                }
                _ => {}
            }
        }
        settings
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = format!(
            "[General]\nextra_args={}\nproton={}\nlanguage={}\ndebug={}\n",
            self.extra_args,
            self.proton,
            self.language,
            if self.debug { "true" } else { "false" }
        );
        text.push_str(&format!(
            "launcher_trim={}\nlauncher_trim_delay={}\nprotontricks_args={}\n",
            self.trim_mode, self.trim_delay, self.protontricks_args
        ));
        for path in &self.custom {
            text.push_str("custom_proton=");
            text.push_str(path);
            text.push('\n');
        }
        fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_qsettings_ini() {
        let dir = std::env::temp_dir().join(format!("majelinux-ini-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("config.ini");
        fs::write(
            &path,
            "[General]\nextra_args=-disable-gpu -foo\ndebug=true\nlanguage=ru\nproton=path:/tmp/x\ncustom_proton=/opt/p\n",
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.extra_args, "-disable-gpu -foo");
        assert_eq!(settings.language, "ru");
        assert!(settings.debug);
        assert_eq!(settings.proton, "path:/tmp/x");
        assert_eq!(settings.custom, vec!["/opt/p".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}
