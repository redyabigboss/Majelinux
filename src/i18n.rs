use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_language(code: &str) {
    LANG.store(if code == "ru" { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn language() -> &'static str {
    if LANG.load(Ordering::Relaxed) == 1 {
        "ru"
    } else {
        "en"
    }
}

static TABLE: &[(&str, &str, &str)] = &[
    (
        "err.cancelled",
        "Installation cancelled.",
        "Установка отменена.",
    ),
    (
        "err.font_rename",
        "Failed to prepare the emoji font: {err}",
        "Не удалось подготовить emoji-шрифт: {err}",
    ),
    (
        "gui.log_file",
        "[gui] Log file: {path}",
        "[gui] Файл лога: {path}",
    ),
    (
        "gui.waiting_packages",
        "[gui] Waiting for packages: {pkgs}",
        "[gui] Жду установки пакетов: {pkgs}",
    ),
    (
        "gui.packages_ok",
        "[gui] Packages found, continuing",
        "[gui] Пакеты найдены, продолжаю",
    ),
    (
        "gui.install_done",
        "[gui] Installation finished",
        "[gui] Установка завершена",
    ),
    ("nav.install", "Installation", "Установка"),
    ("btn.open_logs", "Open logs folder", "Открыть папку логов"),
    ("btn.install", "Install", "Установить"),
    ("btn.copy", "Copy", "Копировать"),
    ("status.cmd_copied", "Command copied", "Команда скопирована"),
    (
        "install.desc",
        "Installs the components the launcher needs. Steps that are already done are skipped.",
        "Ставит компоненты, нужные лаунчеру. Уже выполненные шаги пропускаются.",
    ),
    ("install.status.idle", "Not installed", "Не установлено"),
    (
        "install.status.done",
        "Everything is installed",
        "Всё установлено",
    ),
    ("install.status.running", "Installing…", "Установка…"),
    (
        "install.status.waiting",
        "Waiting for packages…",
        "Ожидание пакетов…",
    ),
    (
        "install.status.failed",
        "Installation failed",
        "Ошибка установки",
    ),
    (
        "install.packages.title",
        "Packages required",
        "Нужны пакеты",
    ),
    (
        "install.packages.desc",
        "Package manager: {pm}. Run the command in a terminal, installation will continue automatically afterwards.",
        "Пакетный менеджер: {pm}. Выполни команду в терминале, после этого установка продолжится автоматически.",
    ),
    (
        "install.packages.unknown",
        "Package manager not detected. Install these packages manually, installation will continue automatically afterwards.",
        "Пакетный менеджер не определён. Установи эти пакеты вручную, после этого установка продолжится автоматически.",
    ),
    (
        "err.cmd_not_found",
        "Command not found: {cmd}",
        "Не найдена команда: {cmd}",
    ),
    (
        "err.exit_code",
        "{name} exited with code {code}",
        "{name} завершился с кодом {code}",
    ),
    (
        "err.proton_missing",
        "Proton not found: {name}. Install it in Steam.",
        "Не найден Proton: {name}. Установи его в Steam.",
    ),
    (
        "err.download",
        "Failed to download {url}: {err}",
        "Не удалось скачать {url}: {err}",
    ),
    (
        "err.powershell",
        "powershell did not appear in winetricks.log, installation failed.",
        "powershell не появился в winetricks.log, установка не удалась.",
    ),
    (
        "err.launcher_after_install",
        "Launcher not found after installation.",
        "Лаунчер не найден после установки.",
    ),
    (
        "err.steam_handler",
        "Patch was not applied: the prefix registry has no steam:// handler.",
        "Патч не применился: в реестре префикса нет обработчика steam://.",
    ),
    (
        "err.emoji_checksum",
        "Emoji font checksum mismatch.",
        "Контрольная сумма emoji-шрифта не совпала.",
    ),
    (
        "err.emoji_registry",
        "Emoji font was not registered in the prefix registry.",
        "emoji-шрифт не зарегистрировался в реестре префикса.",
    ),
    (
        "err.gta_missing",
        "GTA V not found. Install it via Steam and run it once.",
        "GTA V не найдена. Установи её через Steam и запусти один раз.",
    ),
    (
        "err.no_prefix",
        "Prefix 271590 not found. Run GTA V from Steam once.",
        "Нет префикса 271590. Запусти GTA V из Steam один раз.",
    ),
    (
        "err.launcher_missing",
        "Launcher not found in the prefix.",
        "Лаунчер не найден в префиксе.",
    ),
    (
        "diag.shim",
        "[diag] pid {pid}: {name} | LC_ALL={lc}, nolcall.so {state}",
        "[diag] pid {pid}: {name} | LC_ALL={lc}, nolcall.so {state}",
    ),
    ("diag.loaded", "loaded", "загружен"),
    ("diag.not_loaded", "NOT loaded", "НЕ загружен"),
    ("diag.unset", "unset", "не задан"),
    ("log.downloading", "Downloading {url}", "Скачиваю {url}"),
    (
        "log.prefix_prepare",
        "Preparing the Proton prefix for the selected Proton build…",
        "Подготавливаю префикс Proton для выбранной сборки Proton…",
    ),
    (
        "log.wait_install",
        "Waiting 5 seconds after installation…",
        "Жду 5 секунд после установки…",
    ),
    ("step.proton", "Proton patch", "Патч Proton"),
    (
        "step.powershell",
        "PowerShell in prefix",
        "PowerShell в префиксе",
    ),
    ("step.launcher", "Majestic launcher", "Лаунчер Majestic"),
    ("step.handler", "steam:// handler", "Обработчик steam://"),
    ("step.emoji", "Emoji font", "Шрифт эмодзи"),
    (
        "step.ru_fix",
        "Russian layout fix",
        "Фикс русской раскладки",
    ),
    (
        "gui.step_done",
        "[gui] {title}: already done",
        "[gui] {title}: уже готово",
    ),
    (
        "gui.step_run",
        "[gui] {title}: running…",
        "[gui] {title}: выполняю…",
    ),
    (
        "gui.step_warn",
        "[gui] Warning ({title}): {err}",
        "[gui] Предупреждение ({title}): {err}",
    ),
    (
        "gui.drive_s_fail",
        "[gui] Failed to create drive S: {err}",
        "[gui] Не удалось создать диск S: {err}",
    ),
    (
        "gui.launching",
        "[gui] Starting the launcher",
        "[gui] Запускаю лаунчер",
    ),
    (
        "gui.finished",
        "[gui] Finished, code {code}",
        "[gui] Завершено, код {code}",
    ),
    ("gui.error", "[gui] Error: {err}", "[gui] Ошибка: {err}"),
    (
        "pp.err.no_ntdll_src",
        "ntdll.so not found in the selected Proton.",
        "ntdll.so не найден в выбранном Proton.",
    ),
    (
        "pp.err.no_ntdll_dst",
        "ntdll.so not found in the Proton copy.",
        "ntdll.so не найден в копии Proton.",
    ),
    (
        "pp.err.pattern",
        "Pattern found {count} times in ntdll.so (exactly 1 required): this Proton build differs, nothing was changed.",
        "Паттерн в ntdll.so найден {count} раз (нужен ровно 1): эта сборка Proton отличается, ничего не изменено.",
    ),
    (
        "pp.err.copy",
        "Failed to copy Proton: {err}",
        "Не удалось скопировать Proton: {err}",
    ),
    (
        "pp.err.verify",
        "Post-write patch verification failed.",
        "Проверка после записи патча не прошла.",
    ),
    (
        "pp.log.copy",
        "Copying {name} → {dst} (this may take a while)",
        "Копирую {name} → {dst} (это может занять время)",
    ),
    (
        "pp.log.already",
        "ntdll.so already patched",
        "ntdll.so уже пропатчен",
    ),
    ("pp.log.patched", "ntdll.so patched", "ntdll.so пропатчен"),
    (
        "pp.log.backup",
        "Original ntdll.so saved to {path}",
        "Оригинальный ntdll.so сохранён в {path}",
    ),
    (
        "pp.log.modern",
        "{name}: Wine already restores RCX on return, the ntdll patch is not needed — using this Proton as is.",
        "{name}: Wine уже сам восстанавливает RCX при возврате, патч ntdll не нужен — Proton используется как есть.",
    ),
    (
        "pp.log.as_is",
        "{name}: this build has no RCX sequence to patch — using it as is.",
        "{name}: в этой сборке нет последовательности RCX для патча — используется как есть.",
    ),
    (
        "pp.log.readonly",
        "{name}: Proton directory is read-only, skipping the ntdll patch.",
        "{name}: каталог Proton только для чтения, патч ntdll пропущен.",
    ),
    ("nav.general", "General", "Главная"),
    ("nav.settings", "Settings", "Настройки"),
    ("nav.additional", "Additionally", "Дополнительно"),
    ("nav.about", "About", "О программе"),
    ("about.version", "Version {version}", "Версия {version}"),
    (
        "about.tagline",
        "Native Rust launcher for Majestic RP on Linux",
        "Нативный Rust-лаунчер Majestic RP на Linux",
    ),
    ("about.project", "Project", "Проект"),
    ("about.github", "Open on GitHub", "Открыть на GitHub"),
    ("about.license", "MIT License", "Лицензия MIT"),
    (
        "about.credits",
        "Proton is provided by Valve and its contributors. Grand Theft Auto V and Majestic RP belong to their respective owners.",
        "Proton предоставляется Valve и его авторами. Grand Theft Auto V и Majestic RP принадлежат их правообладателям.",
    ),
    ("status.running", "Running", "Работает"),
    ("status.stopping", "Stopping…", "Останавливаю…"),
    ("status.log_copied", "Log copied", "Лог скопирован"),
    (
        "status.launcher_running",
        "Close the Majestic launcher before installing. The installer rewrites the same Proton prefix that the launcher is using.",
        "Закрой лаунчер Majestic перед установкой. Установщик изменяет тот же префикс Proton, который использует лаунчер.",
    ),
    ("btn.play", "Play", "Играть"),
    (
        "general.not_installed",
        "First launch: Play installs everything needed by itself.",
        "Первый запуск: «Играть» сам установит всё необходимое.",
    ),
    (
        "general.install_failed",
        "Installation failed. Details are in the Installation section.",
        "Установка не удалась. Подробности в разделе «Установка».",
    ),
    (
        "gui.steam_closing",
        "Closing Steam to change its settings…",
        "Закрываю Steam, чтобы изменить его настройки…",
    ),
    ("gui.steam_opening", "Starting Steam…", "Запускаю Steam…"),
    (
        "gui.steam_starting",
        "Starting Steam again…",
        "Запускаю Steam снова…",
    ),
    (
        "err.steam_close",
        "Steam did not close. Close it and press Play again.",
        "Steam не закрылся. Закрой его и нажми «Играть» ещё раз.",
    ),
    ("btn.stop", "Stop", "Остановить"),
    ("btn.copy_log", "Copy log", "Копировать лог"),
    ("settings.group", "Preferences", "Параметры"),
    ("add.group", "Options", "Опции"),
    ("settings.language.title", "Language", "Язык"),
    (
        "settings.language.desc",
        "Interface language: EN or RU",
        "Язык интерфейса: EN или RU",
    ),
    ("settings.args.title", "Arguments", "Аргументы"),
    (
        "settings.args.desc",
        "Passed to the launcher on start",
        "Передаются лаунчеру при запуске",
    ),
    ("step.compat", "Proton for GTA V", "Proton для GTA V"),
    (
        "err.steam_running",
        "Close Steam and run the installation again: the Proton for GTA V cannot be changed while Steam is running.",
        "Закрой Steam и запусти установку снова: пока Steam работает, Proton для GTA V изменить нельзя.",
    ),
    (
        "err.steam_config",
        "Failed to edit the Steam config: {err}",
        "Не удалось изменить конфиг Steam: {err}",
    ),
    (
        "gui.compat_set",
        "[gui] Proton for GTA V set to {name}",
        "[gui] Proton для GTA V: {name}",
    ),
    (
        "gui.compat_deferred",
        "[gui] Steam is running, the Proton mapping will be written after Steam is closed",
        "[gui] Steam запущен, привязка Proton запишется после закрытия Steam",
    ),
    (
        "gui.launch_failed",
        "The launcher exited right after start (code {code}). Check the log.",
        "Лаунчер завершился сразу после запуска (код {code}). Смотри лог.",
    ),
    ("status.starting", "Starting…", "Запуск…"),
    (
        "log.update_found",
        "Launcher update found: {name}",
        "Найдено обновление лаунчера: {name}",
    ),
    (
        "log.update_exit",
        "Update installer finished with code {code}",
        "Установщик обновления завершился с кодом {code}",
    ),
    (
        "log.update_started",
        "Launcher started after the update",
        "Лаунчер запустился после обновления",
    ),
    (
        "log.update_not_started",
        "Launcher did not start after the update, starting it manually",
        "Лаунчер не запустился после обновления, запускаю вручную",
    ),
    ("about.update.title", "Updates", "Обновления"),
    ("update.check", "Check for updates", "Проверить обновления"),
    (
        "update.current",
        "Installed version {version}",
        "Установлена версия {version}",
    ),
    (
        "update.checking",
        "Checking for updates…",
        "Проверяю обновления…",
    ),
    (
        "update.latest",
        "You have the latest version ({version})",
        "У тебя последняя версия ({version})",
    ),
    (
        "update.available",
        "Version {version} is available",
        "Доступна версия {version}",
    ),
    (
        "update.apply",
        "Update to {version}",
        "Обновить до {version}",
    ),
    (
        "update.downloading",
        "Downloading and verifying the update…",
        "Скачиваю и проверяю обновление…",
    ),
    (
        "update.restarting",
        "Updated. Restarting…",
        "Обновлено. Перезапускаю…",
    ),
    (
        "update.failed",
        "Update failed: {err}",
        "Не удалось обновить: {err}",
    ),
    (
        "update.check_failed",
        "Could not check for updates: {err}",
        "Не удалось проверить обновления: {err}",
    ),
    (
        "update.err.parse",
        "Unexpected answer from GitHub",
        "Неожиданный ответ от GitHub",
    ),
    (
        "update.err.no_sum",
        "The release has no checksum for this build",
        "В релизе нет контрольной суммы для этой сборки",
    ),
    (
        "update.err.checksum",
        "The downloaded file does not match its checksum",
        "Скачанный файл не совпал с контрольной суммой",
    ),
    (
        "update.err.invalid",
        "The downloaded file is not a working Majelinux build",
        "Скачанный файл не является рабочей сборкой Majelinux",
    ),
    (
        "update.err.run",
        "The downloaded build does not start: {err}",
        "Скачанная сборка не запускается: {err}",
    ),
    (
        "update.err.target",
        "Cannot find the file to replace",
        "Не найден файл, который нужно заменить",
    ),
    (
        "update.err.package",
        "Majelinux is installed by a package manager. Update it with the package manager.",
        "Majelinux установлен менеджером пакетов. Обнови его через менеджер пакетов.",
    ),
    (
        "update.err.disabled",
        "Self-update is disabled (MAJELINUX_NO_SELF_UPDATE)",
        "Самообновление отключено (MAJELINUX_NO_SELF_UPDATE)",
    ),
    (
        "update.err.replace",
        "Could not replace the program: {err}",
        "Не удалось заменить программу: {err}",
    ),
    ("settings.proton.title", "Proton", "Proton"),
    (
        "settings.proton.desc",
        "Every Proton found on this PC is listed: Steam libraries, compatibilitytools.d, Flatpak and Snap Steam, system folders. You can also add your own folder.",
        "Показаны все Proton, найденные на этом ПК: библиотеки Steam, compatibilitytools.d, Steam из Flatpak и Snap, системные папки. Можно добавить и свою папку.",
    ),
    ("settings.proton.rescan", "Scan again", "Найти заново"),
    ("settings.proton.pick", "Choose folder…", "Выбрать папку…"),
    (
        "settings.proton.clear",
        "Reset own paths",
        "Сбросить свои пути",
    ),
    (
        "settings.proton.pick_title",
        "Select a Proton folder or a folder with Proton builds",
        "Выбери папку Proton или папку со сборками Proton",
    ),
    ("settings.proton.pick_accept", "Select", "Выбрать"),
    ("settings.proton.pick_cancel", "Cancel", "Отмена"),
    (
        "settings.proton.custom_empty",
        "No Proton found in the selected folder.",
        "В выбранной папке Proton не найден.",
    ),
    (
        "settings.proton.scanning",
        "Looking for Proton…",
        "Ищу Proton…",
    ),
    (
        "settings.proton.no_steam",
        "Steam not found. Install Steam (native or Flatpak) and start it once.",
        "Steam не найден. Установи Steam (обычный или Flatpak) и запусти его хотя бы раз.",
    ),
    (
        "settings.proton.none",
        "No Proton found. Install any Proton in Steam (Library, Tools filter) or add your own folder.",
        "Proton не найден. Установи любой Proton в Steam (Библиотека, фильтр «Инструменты») или добавь свою папку.",
    ),
    (
        "settings.proton.denied",
        "No access to the Steam folder. Check its permissions.",
        "Нет доступа к папке Steam. Проверь права доступа.",
    ),
    (
        "settings.proton.stock",
        "Proton Experimental",
        "Proton Experimental",
    ),
    ("settings.proton.state.ready", "Ready", "Готов"),
    (
        "settings.proton.state.missing",
        "Not installed",
        "Не установлен",
    ),
    (
        "settings.proton.state.broken",
        "Damaged or unreadable",
        "Повреждён или недоступен",
    ),
    ("add.debug.title", "Debug mode", "Режим отладки"),
    (
        "add.debug.desc",
        "After launch, opens a window with live logs and closes the program. The window stays until you close it.",
        "После запуска открывает окно с логами в реальном времени, а программа закрывается. Окно остаётся, пока ты его не закроешь.",
    ),
    ("debug.title", "Majelinux — Debug", "Majelinux — Отладка"),
    ("debug.stop_prefix", "Stop prefix", "Остановить префикс"),
    ("debug.back", "Back to launcher", "Вернуться в лаунчер"),
    ("debug.finished", "Finished", "Завершено"),
    (
        "debug.confirm_close",
        "The launcher is still running. Stop the prefix and close the window?",
        "Лаунчер ещё работает. Остановить префикс и закрыть окно?",
    ),
    (
        "app.already_running",
        "Majelinux is already running.",
        "Majelinux уже запущен.",
    ),
    (
        "app.no_display",
        "No graphical session found.",
        "Не найдена графическая сессия.",
    ),
];

fn index() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static INDEX: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    INDEX.get_or_init(|| {
        TABLE
            .iter()
            .map(|(key, en, ru)| (*key, (*en, *ru)))
            .collect()
    })
}

pub fn t(key: &str) -> String {
    match index().get(key) {
        Some((en, ru)) => {
            if LANG.load(Ordering::Relaxed) == 1 {
                (*ru).to_string()
            } else {
                (*en).to_string()
            }
        }
        None => key.to_string(),
    }
}

pub fn tf(key: &str, vars: &[(&str, &str)]) -> String {
    let mut text = t(key);
    for (name, value) in vars {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_translated() {
        for (key, en, ru) in TABLE {
            assert!(!key.is_empty());
            assert!(!en.is_empty(), "missing EN for {key}");
            assert!(!ru.is_empty(), "missing RU for {key}");
        }
        let mut keys: Vec<_> = TABLE.iter().map(|row| row.0).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(count, keys.len(), "duplicate i18n keys");
    }

    #[test]
    fn placeholders_are_substituted() {
        set_language("ru");
        assert_eq!(
            tf("err.cmd_not_found", &[("cmd", "gcc")]),
            "Не найдена команда: gcc"
        );
        set_language("en");
        assert_eq!(
            tf("err.cmd_not_found", &[("cmd", "gcc")]),
            "Command not found: gcc"
        );
    }
}
