mod css;
mod widgets;

use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::settings::Settings;
use crate::core::update as selfupdate;
use crate::core::util::escape_pango;
use crate::core::{Error, Result, install, launch, log, paths, process, proton};
use crate::ffi::*;
use crate::i18n::{self, t, tf};
use widgets::*;

const GOOGLE_SANS: &[u8] = include_bytes!("../../assets/fonts/GoogleSans.ttf");
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");
const STEP_COUNT: usize = 7;

static WAKE_PENDING: AtomicBool = AtomicBool::new(false);
static WAKE_TARGET: AtomicUsize = AtomicUsize::new(0);

fn request_wake() {
    let target = WAKE_TARGET.load(Ordering::SeqCst);
    if target == 0 {
        return;
    }
    if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
        unsafe { g_idle_add(Some(wake_cb), target as W) };
    }
}

unsafe extern "C" fn wake_cb(data: W) -> c_int {
    WAKE_PENDING.store(false, Ordering::SeqCst);
    let app = unsafe { &*(data as *const App) };
    app.update();
    0
}

struct SharedState {
    rev: u64,
    install_running: bool,
    install_states: [&'static str; STEP_COUNT],
    status_key: &'static str,
    packages: Vec<String>,
    command: String,
    manager: String,
    install_log: Option<PathBuf>,
    install_result: Option<bool>,
    install_stopping: bool,
    launch_running: bool,
    launch_log: Option<PathBuf>,
    launch_result: Option<std::result::Result<Option<i32>, String>>,
    debug_running: bool,
    debug_log: Option<PathBuf>,
    debug_stopping: bool,
    debug_finished: Option<i32>,
}

impl Clone for SharedState {
    fn clone(&self) -> Self {
        Self {
            rev: self.rev,
            install_running: self.install_running,
            install_states: self.install_states,
            status_key: self.status_key,
            packages: self.packages.clone(),
            command: self.command.clone(),
            manager: self.manager.clone(),
            install_log: self.install_log.clone(),
            install_result: self.install_result,
            install_stopping: self.install_stopping,
            launch_running: self.launch_running,
            launch_log: self.launch_log.clone(),
            launch_result: self.launch_result.clone(),
            debug_running: self.debug_running,
            debug_log: self.debug_log.clone(),
            debug_stopping: self.debug_stopping,
            debug_finished: self.debug_finished,
        }
    }
}

enum UpdateEvent {
    Checked(
        std::result::Result<Option<selfupdate::Release>, String>,
        bool,
    ),
    Applied(std::result::Result<PathBuf, String>),
}

#[derive(Clone)]
enum UpdateView {
    Idle,
    Checking,
    Latest,
    Available(String),
    Downloading,
    Restarting,
    Failed(String),
}

struct Shared {
    inner: Mutex<Arc<SharedState>>,
    update: Mutex<Option<UpdateEvent>>,
    listing: Mutex<Option<proton::Listing>>,
    status: Mutex<Option<(u64, install::InstallationStatus)>>,
}

impl Shared {
    fn new(status_key: &'static str) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Arc::new(SharedState {
                rev: 1,
                install_running: false,
                install_states: ["pending"; STEP_COUNT],
                status_key,
                packages: Vec::new(),
                command: String::new(),
                manager: String::new(),
                install_log: None,
                install_result: None,
                install_stopping: false,
                launch_running: false,
                launch_log: None,
                launch_result: None,
                debug_running: false,
                debug_log: None,
                debug_stopping: false,
                debug_finished: None,
            })),
            update: Mutex::new(None),
            listing: Mutex::new(None),
            status: Mutex::new(None),
        })
    }

    fn update<F: FnOnce(&mut SharedState)>(&self, change: F) {
        if let Ok(mut guard) = self.inner.lock() {
            let state = Arc::make_mut(&mut *guard);
            change(state);
            state.rev = state.rev.wrapping_add(1);
        }
        request_wake();
    }

    fn put_update(&self, event: UpdateEvent) {
        if let Ok(mut slot) = self.update.lock() {
            *slot = Some(event);
        }
        request_wake();
    }

    fn take_update(&self) -> Option<UpdateEvent> {
        self.update.lock().ok().and_then(|mut slot| slot.take())
    }

    fn put_listing(&self, listing: proton::Listing) {
        if let Ok(mut slot) = self.listing.lock() {
            *slot = Some(listing);
        }
        request_wake();
    }

    fn take_listing(&self) -> Option<proton::Listing> {
        self.listing.lock().ok().and_then(|mut slot| slot.take())
    }

    fn take_install_status(&self) -> Option<(u64, install::InstallationStatus)> {
        self.status.lock().ok().and_then(|mut slot| slot.take())
    }

    fn put_install_status(&self, generation: u64, status: install::InstallationStatus) {
        if let Ok(mut slot) = self.status.lock() {
            *slot = Some((generation, status));
        }
        request_wake();
    }

    fn snapshot(&self) -> Arc<SharedState> {
        match self.inner.lock() {
            Ok(state) => Arc::clone(&*state),
            Err(poison) => Arc::clone(&*poison.into_inner()),
        }
    }

    fn rev(&self) -> u64 {
        self.inner.lock().map(|state| state.rev).unwrap_or(0)
    }

    fn install_running(&self) -> bool {
        self.inner
            .lock()
            .map(|state| state.install_running)
            .unwrap_or(true)
    }
}

struct GeneralWidgets {
    page: W,
    play: W,
    play_label: W,
    logs: W,
    note: W,
    sub: W,
}

struct InstallWidgets {
    page: W,
    title: W,
    status: W,
    desc: W,
    step_labels: Vec<W>,
    step_icons: Vec<W>,
    panel: W,
    panel_title: W,
    panel_desc: W,
    cmd: W,
    cmd_copy: W,
    log_scroll: W,
    log_view: W,
    log_buffer: W,
    copy_log: W,
    logs: W,
}

struct SettingsWidgets {
    page: W,
    title: W,
    group_title: W,
    lang_title: W,
    lang_desc: W,
    lang_en: W,
    lang_ru: W,
    proton_title: W,
    proton_desc: W,
    proton_list: W,
    proton_rescan: W,
    proton_pick: W,
    proton_clear: W,
    proton_hint: W,
    args_title: W,
    args_desc: W,
    entry: W,
    reset: W,
}

struct AddWidgets {
    page: W,
    title: W,
    group_title: W,
    debug_title: W,
    debug_desc: W,
    debug_view: *const SwitchView,
}

struct AboutWidgets {
    page: W,
    title: W,
    tagline: W,
    version: W,
    project_title: W,
    update_title: W,
    update_status: W,
    update_button: W,
    repo: W,
    repo_label: W,
    license: W,
    credits: W,
}

struct DebugWin {
    window: W,
    view: W,
    buffer: W,
    status: W,
    stop: W,
    settings: Settings,
    log_path: RefCell<Option<PathBuf>>,
    offset: Cell<u64>,
    mirror: RefCell<String>,
    lines: Cell<usize>,
}

struct ProtonRow {
    button: W,
    icon: W,
    option: Box<OptionData>,
}

struct OptionData {
    app: *const App,
    id: String,
}

struct SwitchView {
    area: W,
    app: Cell<*const App>,
    on: Cell<bool>,
    pos: Cell<f64>,
    target: Cell<f64>,
    timer: Cell<u32>,
}

const PI: f64 = std::f64::consts::PI;

fn mix(from: (f64, f64, f64), to: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    (
        from.0 + (to.0 - from.0) * t,
        from.1 + (to.1 - from.1) * t,
        from.2 + (to.2 - from.2) * t,
    )
}

unsafe fn rounded_rect(cr: W, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    unsafe {
        cairo_new_path(cr);
        cairo_arc(cr, x + radius, y + radius, radius, PI, 1.5 * PI);
        cairo_arc(
            cr,
            x + width - radius,
            y + radius,
            radius,
            1.5 * PI,
            2.0 * PI,
        );
        cairo_arc(
            cr,
            x + width - radius,
            y + height - radius,
            radius,
            0.0,
            0.5 * PI,
        );
        cairo_arc(cr, x + radius, y + height - radius, radius, 0.5 * PI, PI);
        cairo_close_path(cr);
    }
}

unsafe extern "C" fn draw_switch(widget: W, cr: W, data: W) -> c_int {
    let view = unsafe { &*(data as *const SwitchView) };
    let pos = view.pos.get().clamp(0.0, 1.0);
    let width = unsafe { gtk_widget_get_allocated_width(widget) } as f64;
    let height = unsafe { gtk_widget_get_allocated_height(widget) } as f64;
    if width <= 0.0 || height <= 0.0 {
        return 0;
    }
    let track = mix((0.227, 0.169, 0.180), (1.0, 0.698, 0.733), pos);
    let border = mix((0.329, 0.259, 0.271), (1.0, 0.769, 0.796), pos);
    unsafe {
        rounded_rect(cr, 0.5, 0.5, width - 1.0, height - 1.0, height / 2.0 - 0.5);
        cairo_set_source_rgba(cr, track.0, track.1, track.2, 1.0);
        cairo_fill_preserve(cr);
        cairo_set_line_width(cr, 1.0);
        cairo_set_source_rgba(cr, border.0, border.1, border.2, 1.0);
        cairo_stroke(cr);

        let margin = 3.0;
        let radius = (height - 2.0 * margin) / 2.0;
        let x = margin + radius + pos * (width - 2.0 * margin - 2.0 * radius);
        let y = height / 2.0;

        cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 0.16);
        cairo_arc(cr, x, y + 0.7, radius, 0.0, 2.0 * PI);
        cairo_fill(cr);

        let knob = mix((0.788, 0.706, 0.718), (1.0, 0.965, 0.969), pos);
        cairo_set_source_rgba(cr, knob.0, knob.1, knob.2, 1.0);
        cairo_arc(cr, x, y, radius, 0.0, 2.0 * PI);
        cairo_fill(cr);
    }
    0
}

unsafe extern "C" fn switch_animate(data: W) -> c_int {
    let view = unsafe { &*(data as *const SwitchView) };
    let target = view.target.get();
    let current = view.pos.get();
    let diff = target - current;
    if diff.abs() < 0.015 {
        view.pos.set(target);
    } else {
        view.pos.set(current + diff * 0.30);
    }
    unsafe { gtk_widget_queue_draw(view.area) };
    if view.pos.get() == target {
        view.timer.set(0);
        0
    } else {
        1
    }
}

unsafe extern "C" fn switch_click(_widget: W, _event: W, data: W) -> c_int {
    let view = unsafe { &*(data as *const SwitchView) };
    let on = !view.on.get();
    view.on.set(on);
    view.target.set(if on { 1.0 } else { 0.0 });
    let app = view.app.get();
    if !app.is_null() {
        let app = unsafe { &*app };
        app.settings.borrow_mut().debug = on;
        app.save_settings();
    }
    if view.timer.get() == 0 {
        let tag = unsafe { g_timeout_add(16, Some(switch_animate), data) };
        view.timer.set(tag);
    }
    1
}

struct App {
    window: W,
    pages: Vec<W>,
    nav: Vec<W>,
    general: GeneralWidgets,
    installer: InstallWidgets,
    settings_page: SettingsWidgets,
    add: AddWidgets,
    about: AboutWidgets,
    settings: RefCell<Settings>,
    shared: Arc<Shared>,
    status_scan_running: Arc<AtomicBool>,
    status_scan_dirty: Arc<AtomicBool>,
    status_scan_generation: Arc<AtomicU64>,
    last_rev: Cell<u64>,
    install_active: Cell<bool>,
    debug_active: Cell<bool>,
    debug_stopping: Cell<bool>,
    debug_finished: Cell<bool>,
    debug_leaving: Cell<bool>,
    install_log: RefCell<Option<PathBuf>>,
    debug_log: RefCell<Option<PathBuf>>,
    installed: Cell<bool>,
    proton_display: RefCell<String>,
    last_fail: Cell<Option<i32>>,
    install_seen: Cell<bool>,
    launch_seen: Cell<bool>,
    proton_rows: RefCell<Vec<ProtonRow>>,
    scanning: Cell<bool>,
    listing: RefCell<Option<proton::Listing>>,
    step_states: RefCell<[&'static str; STEP_COUNT]>,
    step_shown: RefCell<[&'static str; STEP_COUNT]>,
    step_titles: RefCell<Vec<String>>,
    log_path: RefCell<Option<PathBuf>>,
    log_offset: Cell<u64>,
    log_mirror: RefCell<String>,
    log_lines: Cell<usize>,
    notice: RefCell<Option<(Instant, &'static str)>>,
    update_view: RefCell<UpdateView>,
    update_release: RefCell<Option<selfupdate::Release>>,
    update_busy: Cell<bool>,
    play_after_install: Cell<bool>,
    install_failed: Cell<bool>,
    packages_prompted: Cell<bool>,
    debug: RefCell<Option<Box<DebugWin>>>,
    _lock: File,
}

#[repr(C)]
struct NavData {
    app: *const App,
    index: u32,
}

fn app_ptr(app: &App) -> W {
    (app as *const App as *mut c_void) as W
}

fn leak_nav(app: &App, index: usize) -> W {
    let data = Box::new(NavData {
        app: app as *const App,
        index: index as u32,
    });
    Box::leak(data) as *mut NavData as W
}

pub fn run() -> Result<()> {
    paths::migrate_state();
    fs::create_dir_all(paths::state())?;
    fs::create_dir_all(paths::logs_dir())?;
    let mut settings = Settings::load(&paths::config_ini());

    proton::set_custom(&settings.custom);
    let normalized = proton::normalize_id(&settings.proton);
    if normalized != settings.proton {
        settings.proton = normalized;
        let _ = settings.save(&paths::config_ini());
    }
    i18n::set_language(&settings.language);
    log::cleanup(&paths::logs_dir(), &[paths::legacy_log()]);

    unsafe {
        g_set_prgname(c(paths::APP_NAME).as_ptr());
        gdk_set_program_class(c(paths::APP_NAME).as_ptr());
        g_set_application_name(c(paths::APP_NAME).as_ptr());
        if gtk_init_check(std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            return Err(Error(t("app.no_display")));
        }
        g_object_set(
            gtk_settings_get_default(),
            c("gtk-application-prefer-dark-theme").as_ptr(),
            1 as c_int,
            std::ptr::null::<std::ffi::c_char>(),
        );
        let provider = gtk_css_provider_new();
        let sheet = css::build();
        gtk_css_provider_load_from_data(provider, c(&sheet).as_ptr(), -1, std::ptr::null_mut());
        gtk_style_context_add_provider_for_screen(gdk_screen_get_default(), provider, 600);
        g_object_unref(provider);
    }

    let lock = match acquire_lock() {
        Some(lock) => lock,
        None => {
            unsafe {
                widgets::message(std::ptr::null_mut(), 1, &t("app.already_running"));
            }
            return Ok(());
        }
    };

    register_font();
    let app = Box::leak(Box::new(build_app(settings, lock)));
    app.connect();
    if selfupdate::disabled() {
        app.render_update();
    } else {
        app.start_update_check(false);
    }
    unsafe {
        gtk_widget_show_all(app.window);
        app.select_page(0);
        gtk_window_present(app.window);
        let data = app_ptr(app);
        WAKE_TARGET.store(data as usize, Ordering::SeqCst);
        log::set_hook(request_wake);
        g_unix_signal_add(2, Some(signal_quit), data);
        g_unix_signal_add(15, Some(signal_quit), data);
        g_idle_add(Some(initial_cb), data);
        gtk_main();
    }
    Ok(())
}

fn acquire_lock() -> Option<File> {
    let path = paths::state().join("app.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .ok()?;

    let result = unsafe { flock(file.as_raw_fd(), 2 | 4) };
    if result == 0 { Some(file) } else { None }
}

fn register_font() {
    let dir = paths::state().join("ui");
    let _ = fs::create_dir_all(&dir);
    let font = dir.join("GoogleSans.ttf");
    let matches = fs::metadata(&font)
        .map(|meta| meta.len() == GOOGLE_SANS.len() as u64)
        .unwrap_or(false);
    if !matches {
        let tmp = dir.join("GoogleSans.ttf.tmp");
        if fs::write(&tmp, GOOGLE_SANS).is_ok() {
            let _ = fs::rename(&tmp, &font);
        }
    }
    let name = c(&font.to_string_lossy());
    let added = unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), name.as_ptr()) };
    if added == 0 {
        eprintln!("Google Sans unavailable, falling back to the system font");
    }
}

fn build_app(settings: Settings, lock: File) -> App {
    unsafe {
        let window = gtk_window_new(0);
        gtk_window_set_title(window, c(paths::APP_NAME).as_ptr());
        gtk_window_set_default_size(window, 980, 680);
        gtk_window_set_position(window, 1);
        gtk_widget_set_size_request(window, 860, 600);
        if let Some(pixbuf) = png_pixbuf(ICON_PNG) {
            gtk_window_set_icon(window, pixbuf);
            gtk_window_set_default_icon(pixbuf);
            g_object_unref(pixbuf);
        }

        let shell = box_new(false, 0);
        gtk_container_add(window, shell);

        let rail = box_new(true, 16);
        style(rail, "navigation-rail");
        gtk_widget_set_size_request(rail, 128, -1);
        pack(shell, rail, false);

        let brand_box = box_new(true, 4);
        let logo = png_image(ICON_PNG, 46);
        gtk_widget_set_halign(logo, 3);
        pack(brand_box, logo, false);
        let brand = centered_label(paths::APP_NAME, "brand");
        pack(brand_box, brand, false);
        pack(rail, brand_box, false);

        let nav_box = box_new(true, 6);
        pack(rail, nav_box, false);
        let names = [
            "nav.general",
            "nav.install",
            "nav.settings",
            "nav.additional",
            "nav.about",
        ];
        let icons = ["play", "download", "tune", "plus", "info"];
        let mut nav = Vec::with_capacity(names.len());
        for (index, key) in names.iter().enumerate() {
            let button = button_label(&t(key));
            style(button, "navigation-item");
            button_icon(button, symbol(icons[index], "#ffcdd3", 24));
            pack(nav_box, button, false);
            nav.push(button);
        }
        pack(rail, box_new(true, 0), true);

        let content = box_new(false, 0);
        pack(shell, content, true);

        let general = build_general();
        let installer = build_install();
        let settings_page = build_settings(&settings);
        let add = build_add(&settings);
        let about = build_about();
        let pages = vec![
            general.page,
            installer.page,
            settings_page.page,
            add.page,
            about.page,
        ];
        for page in &pages {
            pack(content, *page, true);
        }

        let shared = Shared::new("install.status.idle");
        App {
            window,
            pages,
            nav,
            general,
            installer,
            settings_page,
            add,
            about,
            settings: RefCell::new(settings),
            shared,
            status_scan_running: Arc::new(AtomicBool::new(false)),
            status_scan_dirty: Arc::new(AtomicBool::new(false)),
            status_scan_generation: Arc::new(AtomicU64::new(0)),
            last_rev: Cell::new(0),
            install_active: Cell::new(false),
            debug_active: Cell::new(false),
            debug_stopping: Cell::new(false),
            debug_finished: Cell::new(false),
            debug_leaving: Cell::new(false),
            install_log: RefCell::new(None),
            debug_log: RefCell::new(None),
            installed: Cell::new(false),
            proton_display: RefCell::new(String::new()),
            last_fail: Cell::new(None),
            install_seen: Cell::new(true),
            launch_seen: Cell::new(true),
            proton_rows: RefCell::new(Vec::new()),
            scanning: Cell::new(false),
            listing: RefCell::new(None),
            step_states: RefCell::new(["pending"; STEP_COUNT]),
            step_shown: RefCell::new([""; STEP_COUNT]),
            step_titles: RefCell::new(Vec::new()),
            log_path: RefCell::new(None),
            log_offset: Cell::new(0),
            log_mirror: RefCell::new(String::new()),
            log_lines: Cell::new(0),
            notice: RefCell::new(None),
            update_view: RefCell::new(UpdateView::Idle),
            update_release: RefCell::new(None),
            update_busy: Cell::new(false),
            play_after_install: Cell::new(false),
            install_failed: Cell::new(false),
            packages_prompted: Cell::new(false),
            debug: RefCell::new(None),
            _lock: lock,
        }
    }
}

fn build_general() -> GeneralWidgets {
    unsafe {
        let page = box_new(true, 16);
        style(page, "page");

        let hero = box_new(true, 8);
        style(hero, "hero");
        let eyebrow = label("MAJESTIC RP", "eyebrow");
        let title = label(paths::APP_NAME, "hero-title");
        let sub = label("", "body");
        let note = label("", "warning");
        gtk_widget_set_no_show_all(note, 1);
        gtk_widget_set_visible(note, 0);
        let actions = box_new(false, 12);

        let play = button_new();
        style(play, "primary");
        style(play, "play-button");
        let play_box = box_new(false, 10);
        pack(play_box, symbol("play", "#561d28", 26), false);
        let play_label = label(&t("btn.play"), "");
        pack(play_box, play_label, false);
        gtk_container_add(play, play_box);
        let logs = button_label(&t("btn.open_logs"));
        style(logs, "text-button");
        pack(actions, play, false);
        pack(actions, logs, false);

        pack(hero, eyebrow, false);
        pack(hero, title, false);
        pack(hero, sub, false);
        pack(hero, note, false);
        pack(hero, actions, false);
        pack(page, hero, false);
        pack(page, box_new(true, 0), true);

        GeneralWidgets {
            page,
            play,
            play_label,
            logs,
            note,
            sub,
        }
    }
}

fn build_install() -> InstallWidgets {
    unsafe {
        let page = box_new(true, 10);
        style(page, "page");

        let head = box_new(false, 12);
        let title = label(&t("nav.install"), "page-title");
        let status = label(&t("install.status.idle"), "status-chip");
        gtk_label_set_xalign(status, 0.5);
        pack(head, title, true);
        pack(head, status, false);
        pack(page, head, false);

        let desc = label(&t("install.desc"), "caption");
        pack(page, desc, false);

        let steps = box_new(true, 8);
        style(steps, "section-card");
        let mut step_labels = Vec::with_capacity(STEP_COUNT);
        let mut step_icons = Vec::with_capacity(STEP_COUNT);
        for _ in 0..STEP_COUNT {
            let row = box_new(false, 10);
            let icon = symbol("dot", "#8f7c7e", 18);
            gtk_widget_set_valign(icon, 3);
            gtk_widget_set_size_request(icon, 20, 20);
            let step = label("", "body");
            pack(row, icon, false);
            pack(row, step, true);
            pack(steps, row, false);
            step_labels.push(step);
            step_icons.push(icon);
        }
        pack(page, steps, false);

        let panel = box_new(true, 6);
        style(panel, "section-card");
        gtk_widget_set_no_show_all(panel, 1);
        gtk_widget_set_visible(panel, 0);
        let panel_title = label(&t("install.packages.title"), "section-title");
        let panel_desc = label("", "caption");
        let cmd_row = box_new(false, 8);
        let cmd = gtk_entry_new();
        style(cmd, "mono");
        gtk_editable_set_editable(cmd, 0);
        let cmd_copy = button_label(&t("btn.copy"));
        style(cmd_copy, "text-button");
        pack(cmd_row, cmd, true);
        pack(cmd_row, cmd_copy, false);
        pack(panel, panel_title, false);
        pack(panel, panel_desc, false);
        pack(panel, cmd_row, false);
        pack(page, panel, false);

        let scroll = gtk_scrolled_window_new(std::ptr::null_mut(), std::ptr::null_mut());
        gtk_scrolled_window_set_policy(scroll, 1, 1);
        gtk_scrolled_window_set_min_content_height(scroll, 150);
        style(scroll, "log-surface");
        gtk_widget_set_no_show_all(scroll, 1);
        let log_view = gtk_text_view_new();
        gtk_text_view_set_editable(log_view, 0);
        gtk_text_view_set_cursor_visible(log_view, 0);
        gtk_text_view_set_monospace(log_view, 1);
        gtk_text_view_set_wrap_mode(log_view, 3);
        log_margins(log_view);
        gtk_container_add(scroll, log_view);
        gtk_widget_show(log_view);
        let log_buffer = gtk_text_view_get_buffer(log_view);
        pack(page, scroll, true);

        let bar = box_new(false, 8);
        let copy_log = button_label(&t("btn.copy_log"));
        style(copy_log, "text-button");
        let logs = button_label(&t("btn.open_logs"));
        style(logs, "text-button");
        pack(bar, copy_log, false);
        pack(bar, logs, false);
        pack(bar, box_new(false, 0), true);
        pack(page, bar, false);

        InstallWidgets {
            page,
            title,
            status,
            desc,
            step_labels,
            step_icons,
            panel,
            panel_title,
            panel_desc,
            cmd,
            cmd_copy,
            log_scroll: scroll,
            log_view,
            log_buffer,
            copy_log,
            logs,
        }
    }
}

fn preference_row(body: W, title: &str, desc: &str, control: W) -> (W, W) {
    unsafe {
        let row = box_new(false, 14);
        style(row, "preference-row");
        let texts = box_new(true, 2);
        let title = label(title, "preference-title");
        let desc = label(desc, "caption");
        pack(texts, title, false);
        pack(texts, desc, false);
        pack(row, texts, true);
        pack(row, control, false);
        pack(body, row, false);
        (title, desc)
    }
}

fn settings_group(parent: W, title: &str) -> (W, W) {
    unsafe {
        let group = box_new(true, 6);
        style(group, "settings-group");
        let group_title = label(title, "group-title");
        pack(group, group_title, false);
        let body = box_new(true, 4);
        style(body, "settings-surface");
        pack(group, body, false);
        pack(parent, group, false);
        (body, group_title)
    }
}

fn build_settings(settings: &Settings) -> SettingsWidgets {
    unsafe {
        let page = box_new(true, 10);
        style(page, "page");
        let title = label(&t("nav.settings"), "page-title");
        pack(page, title, false);

        let scroll = gtk_scrolled_window_new(std::ptr::null_mut(), std::ptr::null_mut());
        gtk_scrolled_window_set_policy(scroll, 2, 1);
        let inner = box_new(true, 0);
        gtk_container_add(scroll, inner);
        pack(page, scroll, true);

        let (body, group_title) = settings_group(inner, &t("settings.group"));

        let lang_control = box_new(false, 4);
        style(lang_control, "segmented");
        let lang_en = button_label("EN");
        let lang_ru = button_label("RU");
        for button in [lang_en, lang_ru] {
            style(button, "segment");
            pack(lang_control, button, false);
        }
        let (lang_title, lang_desc) = preference_row(
            body,
            &t("settings.language.title"),
            &t("settings.language.desc"),
            lang_control,
        );

        let proton_title = label(&t("settings.proton.title"), "preference-title");
        let proton_desc = label(&t("settings.proton.desc"), "caption");
        for widget in [proton_title, proton_desc] {
            gtk_widget_set_margin_start(widget, 12);
            gtk_widget_set_margin_end(widget, 12);
        }
        gtk_widget_set_margin_top(proton_title, 10);
        pack(body, proton_title, false);
        pack(body, proton_desc, false);
        let proton_list = box_new(true, 4);
        pack(body, proton_list, false);
        let proton_hint = label("", "warning");
        gtk_widget_set_margin_start(proton_hint, 12);
        gtk_widget_set_margin_end(proton_hint, 12);
        gtk_widget_set_no_show_all(proton_hint, 1);
        gtk_widget_set_visible(proton_hint, 0);
        pack(body, proton_hint, false);
        let proton_actions = box_new(false, 8);
        gtk_widget_set_halign(proton_actions, 1);
        let proton_pick = button_label(&t("settings.proton.pick"));
        let proton_rescan = button_label(&t("settings.proton.rescan"));
        let proton_clear = button_label(&t("settings.proton.clear"));
        for button in [proton_pick, proton_rescan, proton_clear] {
            style(button, "text-button");
            pack(proton_actions, button, false);
        }
        gtk_widget_set_sensitive(proton_clear, (!settings.custom.is_empty()) as c_int);
        pack(body, proton_actions, false);

        let args_control = box_new(false, 6);
        let entry = gtk_entry_new();
        gtk_entry_set_text(entry, c(&settings.extra_args).as_ptr());
        gtk_widget_set_size_request(entry, 280, -1);
        let reset = button_label("↺");
        style(reset, "icon-button");
        gtk_widget_set_sensitive(reset, (settings.extra_args != paths::DEFAULT_ARGS) as c_int);
        pack(args_control, entry, true);
        pack(args_control, reset, false);
        let (args_title, args_desc) = preference_row(
            body,
            &t("settings.args.title"),
            &t("settings.args.desc"),
            args_control,
        );

        SettingsWidgets {
            page,
            title,
            group_title,
            lang_title,
            lang_desc,
            lang_en,
            lang_ru,
            proton_title,
            proton_desc,
            proton_list,
            proton_rescan,
            proton_pick,
            proton_clear,
            proton_hint,
            args_title,
            args_desc,
            entry,
            reset,
        }
    }
}

fn build_add(settings: &Settings) -> AddWidgets {
    unsafe {
        let page = box_new(true, 10);
        style(page, "page");
        let title = label(&t("nav.additional"), "page-title");
        pack(page, title, false);

        let (body, group_title) = settings_group(page, &t("add.group"));
        let debug_switch = gtk_drawing_area_new();
        gtk_widget_set_size_request(debug_switch, 44, 24);
        gtk_widget_set_halign(debug_switch, 3);
        gtk_widget_set_valign(debug_switch, 3);
        gtk_widget_add_events(debug_switch, 256);
        let view = Box::leak(Box::new(SwitchView {
            area: debug_switch,
            app: Cell::new(std::ptr::null()),
            on: Cell::new(settings.debug),
            pos: Cell::new(if settings.debug { 1.0 } else { 0.0 }),
            target: Cell::new(if settings.debug { 1.0 } else { 0.0 }),
            timer: Cell::new(0),
        }));
        let view_ptr = view as *const SwitchView as W;
        connect(
            debug_switch,
            "draw",
            draw_switch as *const std::ffi::c_void,
            view_ptr,
        );
        connect(
            debug_switch,
            "button-press-event",
            switch_click as *const std::ffi::c_void,
            view_ptr,
        );
        let (debug_title, debug_desc) = preference_row(
            body,
            &t("add.debug.title"),
            &t("add.debug.desc"),
            debug_switch,
        );

        let filler = box_new(true, 0);
        pack(page, filler, true);

        AddWidgets {
            page,
            title,
            group_title,
            debug_title,
            debug_desc,
            debug_view: view_ptr as *const SwitchView,
        }
    }
}

fn build_about() -> AboutWidgets {
    unsafe {
        let page = box_new(true, 14);
        style(page, "page");

        let title = label(&t("nav.about"), "page-title");

        let hero = box_new(false, 18);
        style(hero, "about-hero");
        let logo = png_image(ICON_PNG, 72);
        gtk_widget_set_valign(logo, 3);
        let hero_text = box_new(true, 4);
        let name = label(paths::APP_NAME, "hero-title");
        let tagline = label(&t("about.tagline"), "body");
        pack(hero_text, name, false);
        pack(hero_text, tagline, false);
        pack(hero, logo, false);
        pack(hero, hero_text, true);

        let meta = box_new(false, 12);
        let version = label(
            &tf("about.version", &[("version", paths::APP_VERSION)]),
            "version-chip",
        );
        gtk_label_set_xalign(version, 0.5);
        gtk_widget_set_valign(version, 3);
        pack(meta, version, false);

        let updates = box_new(true, 8);
        style(updates, "section-card");
        let update_title = label(&t("about.update.title"), "section-title");
        let update_status = label("", "caption");
        let update_button = button_label(&t("update.check"));
        style(update_button, "primary");
        gtk_widget_set_halign(update_button, 1);
        pack(updates, update_title, false);
        pack(updates, update_status, false);
        pack(updates, update_button, false);

        let project = box_new(true, 8);
        style(project, "section-card");
        let project_title = label(&t("about.project"), "section-title");
        let repo = button_new();
        style(repo, "primary");
        gtk_widget_set_halign(repo, 1);
        let repo_box = box_new(false, 8);
        pack(repo_box, symbol("open", "#561d28", 18), false);
        let repo_label = label(&t("about.github"), "");
        pack(repo_box, repo_label, false);
        gtk_container_add(repo, repo_box);
        let repo_url = label(paths::REPO_URL, "repo-url");
        gtk_label_set_selectable(repo_url, 1);
        gtk_label_set_line_wrap(repo_url, 0);
        pack(project, project_title, false);
        pack(project, repo, false);
        pack(project, repo_url, false);

        let credits_card = box_new(true, 6);
        style(credits_card, "section-card");
        let license = label(&t("about.license"), "section-title");
        let credits = label(&t("about.credits"), "caption");
        pack(credits_card, license, false);
        pack(credits_card, credits, false);

        pack(page, title, false);
        pack(page, hero, false);
        pack(page, meta, false);
        pack(page, updates, false);
        pack(page, project, false);
        pack(page, credits_card, false);

        AboutWidgets {
            page,
            title,
            tagline,
            version,
            update_title,
            update_status,
            update_button,
            project_title,
            repo,
            repo_label,
            license,
            credits,
        }
    }
}

unsafe fn set_step_icon(widget: W, state: &str) {
    let (name, color) = match state {
        "done" => ("check", "#ffb2bb"),
        "running" => ("play", "#ffb2bb"),
        "warn" => ("warn", "#f2c98a"),
        "error" => ("cross", "#ffb4ab"),
        _ => ("dot", "#8f7c7e"),
    };
    unsafe {
        let pixbuf = symbol_pixbuf(name, color, 18);
        if !pixbuf.is_null() {
            gtk_image_set_from_pixbuf(widget, pixbuf);
            g_object_unref(pixbuf);
        }
    }
}

impl App {
    fn connect(&'static self) {
        unsafe {
            let data = app_ptr(self);
            for (index, button) in self.nav.iter().enumerate() {
                connect(
                    *button,
                    "clicked",
                    nav_click as *const std::ffi::c_void,
                    leak_nav(self, index),
                );
            }
            connect(
                self.general.play,
                "clicked",
                play_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.general.logs,
                "clicked",
                logs_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.installer.copy_log,
                "clicked",
                copy_log_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.installer.logs,
                "clicked",
                logs_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.installer.cmd_copy,
                "clicked",
                copy_cmd_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.lang_en,
                "clicked",
                lang_en_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.lang_ru,
                "clicked",
                lang_ru_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.proton_rescan,
                "clicked",
                proton_rescan_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.proton_pick,
                "clicked",
                proton_pick_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.proton_clear,
                "clicked",
                proton_clear_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.about.update_button,
                "clicked",
                update_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.about.repo,
                "clicked",
                repo_click as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.reset,
                "clicked",
                args_reset as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.entry,
                "changed",
                args_changed as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.entry,
                "activate",
                args_save as *const std::ffi::c_void,
                data,
            );
            connect(
                self.settings_page.entry,
                "focus-out-event",
                args_focus_out as *const std::ffi::c_void,
                data,
            );
            let view = &*self.add.debug_view;
            view.app.set(self as *const App);
            connect(
                self.window,
                "delete-event",
                window_delete as *const std::ffi::c_void,
                data,
            );
            connect(
                self.window,
                "destroy",
                quit_click as *const std::ffi::c_void,
                data,
            );
        }
    }

    fn initial_state(&'static self) {
        self.start_scan();
        self.render_language_buttons();
        let snapshot = self.shared.snapshot();
        self.last_rev.set(snapshot.rev);
        self.install_active.set(snapshot.install_running);
        self.debug_active.set(snapshot.debug_running);
        self.debug_stopping.set(snapshot.debug_stopping);
        self.debug_finished.set(snapshot.debug_finished.is_some());
        *self.install_log.borrow_mut() = snapshot.install_log.clone();
        *self.debug_log.borrow_mut() = snapshot.debug_log.clone();
        self.render_install(&snapshot);
        self.render_play(&snapshot);
    }

    fn select_page(&self, page: usize) {
        unsafe {
            for (index, widget) in self.pages.iter().enumerate() {
                if index == page {
                    gtk_widget_show(*widget);
                } else {
                    gtk_widget_hide(*widget);
                }
            }
            for (index, button) in self.nav.iter().enumerate() {
                if index == page {
                    add_class(*button, "nav-selected");
                    gtk_widget_grab_focus(*button);
                } else {
                    remove_class(*button, "nav-selected");
                }
            }
        }
    }

    fn refresh_installed(&self) {
        self.request_install_status();
    }

    fn refresh_install_steps(&self) {
        self.request_install_status();
    }

    fn request_install_status(&self) {
        if self.scanning.get() || self.shared.install_running() {
            return;
        }
        let generation = self
            .status_scan_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        if self.status_scan_running.swap(true, Ordering::AcqRel) {
            self.status_scan_dirty.store(true, Ordering::Release);
            return;
        }
        self.status_scan_dirty.store(false, Ordering::Release);

        let settings = self.settings.borrow().clone();
        let shared = Arc::clone(&self.shared);
        let running = Arc::clone(&self.status_scan_running);
        let latest_generation = Arc::clone(&self.status_scan_generation);
        thread::spawn(move || {
            let status = install::installation_status(&settings);
            running.store(false, Ordering::Release);
            if latest_generation.load(Ordering::Acquire) == generation {
                shared.put_install_status(generation, status);
            }
        });
    }

    fn apply_install_status(&self, status: install::InstallationStatus) {
        let install::InstallationStatus {
            steps: done,
            installed,
            proton_display,
        } = status;
        self.installed.set(installed);
        *self.proton_display.borrow_mut() = proton_display;
        let titles: Vec<String> = install::STEP_KEYS.iter().map(|key| t(key)).collect();
        let states = {
            let old_titles = self.step_titles.borrow();
            let old_states = self.step_states.borrow();
            let mut states: [&'static str; STEP_COUNT] = ["pending"; STEP_COUNT];
            for index in 0..STEP_COUNT {
                if done.get(index).copied().unwrap_or(false) {
                    states[index] = "done";
                } else if old_titles.get(index) == Some(&titles[index])
                    && old_states
                        .get(index)
                        .is_some_and(|state| *state == "warn" || *state == "error")
                {
                    states[index] = old_states[index];
                }
            }
            states
        };
        *self.step_titles.borrow_mut() = titles;
        *self.step_states.borrow_mut() = states;
        self.step_shown
            .borrow_mut()
            .iter_mut()
            .for_each(|shown| *shown = "");

        let snapshot = self.shared.snapshot();
        if !snapshot.install_running {
            let next = if installed {
                "install.status.done"
            } else if snapshot.status_key == "install.status.done" {
                "install.status.idle"
            } else {
                snapshot.status_key
            };
            if next != snapshot.status_key {
                self.shared.update(|state| state.status_key = next);
            }
        }
        self.refresh_general();
        self.render_steps(&states);
        let snapshot = self.shared.snapshot();
        self.render_install(&snapshot);
        self.render_play(&snapshot);
    }

    fn refresh_general(&self) {
        unsafe {
            set_text(self.general.sub, self.proton_display.borrow().as_str());
            let installed = self.installed.get();
            if !installed {
                let key = if self.install_failed.get() {
                    "general.install_failed"
                } else {
                    "general.not_installed"
                };
                set_text(self.general.note, &t(key));
                gtk_widget_set_visible(self.general.note, 1);
            } else if let Some(code) = self.last_fail.get() {
                set_text(
                    self.general.note,
                    &tf("gui.launch_failed", &[("code", &code.to_string())]),
                );
                gtk_widget_set_visible(self.general.note, 1);
            } else {
                gtk_widget_set_visible(self.general.note, 0);
            }
        }
    }

    fn render_steps(&self, states: &[&'static str]) {
        let mut shown = self.step_shown.borrow_mut();
        unsafe {
            for (index, widget) in self.installer.step_labels.iter().enumerate() {
                let state = states.get(index).copied().unwrap_or("pending");
                if shown[index] == state {
                    continue;
                }
                shown[index] = state;
                let title = escape_pango(&t(install::STEP_KEYS[index]));
                let title_color = match state {
                    "done" | "running" => "#f0dedf",
                    "warn" => "#f2c98a",
                    "error" => "#ffb4ab",
                    _ => "#bfa9ab",
                };
                set_markup(
                    *widget,
                    &format!("<span foreground=\"{title_color}\">{title}</span>"),
                );
                if let Some(icon) = self.installer.step_icons.get(index) {
                    set_step_icon(*icon, state);
                }
            }
        }
    }

    fn render_install(&self, snapshot: &SharedState) {
        unsafe {
            let status = match self.notice.borrow().as_ref() {
                Some((_, key)) => t(key),
                None => t(snapshot.status_key),
            };
            set_text(self.installer.status, &status);

            let running = snapshot.install_running;
            let show_log = running || self.log_lines.get() > 0;
            gtk_widget_set_visible(self.installer.log_scroll, show_log as c_int);

            let states = if running {
                snapshot.install_states
            } else {
                *self.step_states.borrow()
            };
            self.render_steps(&states);

            if snapshot.packages.is_empty() {
                self.packages_prompted.set(false);
                gtk_widget_set_visible(self.installer.panel, 0);
            } else {
                set_text(self.installer.panel_title, &t("install.packages.title"));
                let desc = if !snapshot.manager.is_empty() && !snapshot.command.is_empty() {
                    tf("install.packages.desc", &[("pm", &snapshot.manager)])
                } else {
                    t("install.packages.unknown")
                };
                set_text(self.installer.panel_desc, &desc);
                let command = if snapshot.command.is_empty() {
                    snapshot.packages.join(" ")
                } else {
                    snapshot.command.clone()
                };
                gtk_entry_set_text(self.installer.cmd, c(&command).as_ptr());
                gtk_widget_show_all(self.installer.panel);
                if !self.packages_prompted.replace(true) {
                    self.select_page(1);
                }
            }
        }
    }

    fn render_play(&self, snapshot: &SharedState) {
        unsafe {
            let label = if snapshot.launch_running {
                t("status.starting")
            } else if snapshot.install_running {
                t("install.status.running")
            } else {
                t("btn.play")
            };
            set_text(self.general.play_label, &label);
            let enabled =
                !snapshot.launch_running && !snapshot.debug_running && !snapshot.install_running;
            gtk_widget_set_sensitive(self.general.play, enabled as c_int);
            let idle = (!snapshot.launch_running && !snapshot.install_running) as c_int;
            gtk_widget_set_sensitive(self.settings_page.proton_list, idle);
            let can_scan = idle != 0 && !self.scanning.get();
            gtk_widget_set_sensitive(self.settings_page.proton_rescan, can_scan as c_int);
            gtk_widget_set_sensitive(self.settings_page.proton_pick, can_scan as c_int);
            let has_custom = !self.settings.borrow().custom.is_empty();
            gtk_widget_set_sensitive(
                self.settings_page.proton_clear,
                (can_scan && has_custom) as c_int,
            );
        }
        self.render_update();
    }

    fn render_language_buttons(&self) {
        unsafe {
            let ru = i18n::language() == "ru";
            for (button, active) in [
                (self.settings_page.lang_en, !ru),
                (self.settings_page.lang_ru, ru),
            ] {
                if active {
                    add_class(button, "seg-active");
                } else {
                    remove_class(button, "seg-active");
                }
            }
        }
    }

    fn start_scan(&self) {
        if self.scanning.replace(true) {
            return;
        }
        self.status_scan_generation.fetch_add(1, Ordering::AcqRel);
        self.status_scan_dirty.store(false, Ordering::Release);
        unsafe {
            set_text(
                self.settings_page.proton_hint,
                &t("settings.proton.scanning"),
            );
            gtk_widget_set_visible(self.settings_page.proton_hint, 1);
            gtk_widget_set_sensitive(self.settings_page.proton_rescan, 0);
        }
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            shared.put_listing(proton::listing());
        });
    }

    fn rerender_listing(&self) {
        let stored = self.listing.borrow().clone();
        if let Some(listing) = stored {
            self.render_listing(listing);
        }
    }

    fn render_listing(&self, listing: proton::Listing) {
        unsafe {
            for row in self.proton_rows.borrow_mut().drain(..) {
                gtk_widget_destroy(row.button);
            }
            let current = proton::normalize_id(&self.settings.borrow().proton);
            let interactive = listing.candidates.len() > 1;
            let mut rows = Vec::new();
            for candidate in &listing.candidates {
                let chosen = candidate.id == current || !interactive;
                let name = if candidate.stock {
                    t("settings.proton.stock")
                } else {
                    candidate.name.clone()
                };
                let (state_key, state_class) = match candidate.readiness {
                    proton::Readiness::Ready => ("settings.proton.state.ready", "chip-ready"),
                    proton::Readiness::Broken => ("settings.proton.state.broken", "chip-bad"),
                    proton::Readiness::Missing => ("settings.proton.state.missing", "chip-bad"),
                };

                let root = if interactive {
                    button_new()
                } else {
                    box_new(false, 0)
                };
                style(root, "proton-option");
                if chosen {
                    add_class(root, "proton-selected");
                }
                let row = box_new(false, 14);
                let icon = symbol("dot", "#8f7c7e", 18);
                gtk_widget_set_valign(icon, 3);
                set_step_icon(icon, if chosen { "done" } else { "pending" });
                let texts = box_new(true, 2);
                let name_label = label(&name, "proton-name");
                gtk_label_set_line_wrap(name_label, 0);
                let path_label = label(&display_path(&candidate.path), "proton-path");
                gtk_label_set_line_wrap(path_label, 0);
                gtk_label_set_ellipsize(path_label, 2);
                gtk_label_set_max_width_chars(path_label, 56);
                pack(texts, name_label, false);
                pack(texts, path_label, false);
                let chip = label(&t(state_key), "status-chip");
                add_class(chip, state_class);
                gtk_label_set_xalign(chip, 0.5);
                gtk_widget_set_valign(chip, 3);
                pack(row, icon, false);
                pack(row, texts, true);
                pack(row, chip, false);
                if interactive {
                    gtk_container_add(root, row);
                } else {
                    pack(root, row, true);
                }
                pack(self.settings_page.proton_list, root, false);

                let option = Box::new(OptionData {
                    app: self as *const App,
                    id: candidate.id.clone(),
                });
                if interactive {
                    let data = &*option as *const OptionData as W;
                    connect(
                        root,
                        "clicked",
                        proton_option_click as *const std::ffi::c_void,
                        data,
                    );
                }
                gtk_widget_show_all(root);
                rows.push(ProtonRow {
                    button: root,
                    icon,
                    option,
                });
            }
            *self.proton_rows.borrow_mut() = rows;
            match listing.hint {
                Some(key) => {
                    set_text(self.settings_page.proton_hint, &t(key));
                    gtk_widget_set_visible(self.settings_page.proton_hint, 1);
                }
                None => gtk_widget_set_visible(self.settings_page.proton_hint, 0),
            }
        }
        *self.listing.borrow_mut() = Some(listing);
    }

    fn mark_selected_proton(&self) {
        let current = proton::normalize_id(&self.settings.borrow().proton);
        unsafe {
            for row in self.proton_rows.borrow().iter() {
                if row.option.id == current {
                    add_class(row.button, "proton-selected");
                    set_step_icon(row.icon, "done");
                } else {
                    remove_class(row.button, "proton-selected");
                    set_step_icon(row.icon, "pending");
                }
            }
        }
    }

    fn choose_proton(&self, id: &str) {
        let unchanged = self.settings.borrow().proton == id;
        if !unchanged {
            self.settings.borrow_mut().proton = id.to_string();
            self.save_settings();
        }
        self.mark_selected_proton();
        if !unchanged {
            self.on_proton_changed();
        }
    }

    fn retranslate(&self) {
        unsafe {
            for (index, key) in [
                "nav.general",
                "nav.install",
                "nav.settings",
                "nav.additional",
                "nav.about",
            ]
            .iter()
            .enumerate()
            {
                gtk_button_set_label(self.nav[index], c(&t(key)).as_ptr());
            }
            gtk_button_set_label(self.general.logs, c(&t("btn.open_logs")).as_ptr());
            set_text(self.installer.title, &t("nav.install"));
            set_text(self.installer.desc, &t("install.desc"));
            gtk_button_set_label(self.installer.copy_log, c(&t("btn.copy_log")).as_ptr());
            gtk_button_set_label(self.installer.logs, c(&t("btn.open_logs")).as_ptr());
            gtk_button_set_label(self.installer.cmd_copy, c(&t("btn.copy")).as_ptr());

            set_text(self.settings_page.title, &t("nav.settings"));
            set_text(self.settings_page.group_title, &t("settings.group"));
            set_text(self.settings_page.lang_title, &t("settings.language.title"));
            set_text(self.settings_page.lang_desc, &t("settings.language.desc"));
            set_text(self.settings_page.proton_title, &t("settings.proton.title"));
            set_text(self.settings_page.proton_desc, &t("settings.proton.desc"));
            gtk_button_set_label(
                self.settings_page.proton_rescan,
                c(&t("settings.proton.rescan")).as_ptr(),
            );
            gtk_button_set_label(
                self.settings_page.proton_pick,
                c(&t("settings.proton.pick")).as_ptr(),
            );
            gtk_button_set_label(
                self.settings_page.proton_clear,
                c(&t("settings.proton.clear")).as_ptr(),
            );
            set_text(self.settings_page.args_title, &t("settings.args.title"));
            set_text(self.settings_page.args_desc, &t("settings.args.desc"));
            self.rerender_listing();
            self.render_language_buttons();

            set_text(self.add.title, &t("nav.additional"));
            set_text(self.add.group_title, &t("add.group"));
            set_text(self.add.debug_title, &t("add.debug.title"));
            set_text(self.add.debug_desc, &t("add.debug.desc"));

            set_text(self.about.title, &t("nav.about"));
            set_text(self.about.update_title, &t("about.update.title"));
            self.render_update();
            set_text(self.about.tagline, &t("about.tagline"));
            set_text(
                self.about.version,
                &tf("about.version", &[("version", paths::APP_VERSION)]),
            );
            set_text(self.about.project_title, &t("about.project"));
            set_text(self.about.repo_label, &t("about.github"));
            set_text(self.about.license, &t("about.license"));
            set_text(self.about.credits, &t("about.credits"));

            self.refresh_general();
            self.refresh_install_steps();
            let snapshot = self.shared.snapshot();
            self.render_install(&snapshot);
            self.render_play(&snapshot);
        }
    }

    fn set_notice(&self, key: &'static str) {
        *self.notice.borrow_mut() = Some((Instant::now() + Duration::from_secs(2), key));
        unsafe {
            set_text(self.installer.status, &t(key));
            g_timeout_add(2100, Some(notice_expire), app_ptr(self));
        }
    }

    fn tail_install(&self) {
        let Some(path) = self.install_log.borrow().clone() else {
            return;
        };
        let changed = self.log_path.borrow().as_deref() != Some(path.as_path());
        if changed {
            *self.log_path.borrow_mut() = Some(path.clone());
            self.log_offset.set(0);
            self.log_mirror.borrow_mut().clear();
            self.log_lines.set(0);
        }
        let (text, offset) = log::read_new(&path, self.log_offset.get());
        self.log_offset.set(offset);
        if text.is_empty() {
            if !self.install_active.get() {
                self.install_log.borrow_mut().take();
                self.log_path.borrow_mut().take();
            }
            return;
        }
        let mut mirror = self.log_mirror.borrow_mut();
        let mut lines = self.log_lines.get();
        let trimmed = push_mirror(&mut mirror, &mut lines, &text);
        self.log_lines.set(lines);
        unsafe {
            if changed || trimmed {
                replace_log(self.installer.log_view, self.installer.log_buffer, &mirror);
            } else {
                append_log(self.installer.log_view, self.installer.log_buffer, &text);
            }
            gtk_widget_set_visible(self.installer.log_scroll, 1);
        }
    }

    fn tail_debug(&self) {
        let debug = self.debug.borrow();
        let Some(win) = debug.as_ref() else {
            return;
        };
        if let Some(path) = self.debug_log.borrow().clone() {
            let changed = win.log_path.borrow().as_deref() != Some(path.as_path());
            if changed {
                *win.log_path.borrow_mut() = Some(path.clone());
                win.offset.set(0);
                win.mirror.borrow_mut().clear();
                win.lines.set(0);
            }
            let (text, offset) = log::read_new(&path, win.offset.get());
            win.offset.set(offset);
            if text.is_empty() {
                if !self.debug_active.get() {
                    self.debug_log.borrow_mut().take();
                }
            } else {
                let mut mirror = win.mirror.borrow_mut();
                let mut lines = win.lines.get();
                let trimmed = push_mirror(&mut mirror, &mut lines, &text);
                win.lines.set(lines);
                unsafe {
                    if changed || trimmed {
                        replace_log(win.view, win.buffer, &mirror);
                    } else {
                        append_log(win.view, win.buffer, &text);
                    }
                }
            }
        }
        let running = self.debug_active.get();
        unsafe {
            let status = if self.debug_stopping.get() {
                t("status.stopping")
            } else if running {
                t("status.running")
            } else if self.debug_finished.get() {
                t("debug.finished")
            } else {
                t("status.starting")
            };
            set_text(win.status, &status);
            gtk_widget_set_sensitive(win.stop, running as c_int);
        }
    }

    fn update(&self) {
        if !self.status_scan_running.load(Ordering::Acquire)
            && self.status_scan_dirty.swap(false, Ordering::AcqRel)
        {
            self.request_install_status();
        }
        if let Some((generation, status)) = self.shared.take_install_status()
            && self.status_scan_generation.load(Ordering::Acquire) == generation
        {
            self.apply_install_status(status);
        }
        if let Some(event) = self.shared.take_update() {
            self.handle_update(event);
        }
        if let Some(listing) = self.shared.take_listing() {
            self.scanning.set(false);
            self.render_listing(listing);
            self.refresh_installed();
            let snapshot = self.shared.snapshot();
            self.render_play(&snapshot);
        }
        let rev = self.shared.rev();
        let changed = rev != self.last_rev.get();
        if changed {
            self.last_rev.set(rev);
            let snapshot = self.shared.snapshot();
            self.install_active.set(snapshot.install_running);
            self.debug_active.set(snapshot.debug_running);
            self.debug_stopping.set(snapshot.debug_stopping);
            self.debug_finished.set(snapshot.debug_finished.is_some());
            if snapshot.install_log.is_some() {
                *self.install_log.borrow_mut() = snapshot.install_log.clone();
            }
            if snapshot.debug_log.is_some() {
                *self.debug_log.borrow_mut() = snapshot.debug_log.clone();
            }
            self.render_install(&snapshot);
            self.render_play(&snapshot);
            if let Some(result) = &snapshot.launch_result
                && !self.launch_seen.get()
            {
                self.launch_seen.set(true);
                match result {
                    Ok(None) => {
                        log::shutdown();
                        unsafe { gtk_main_quit() }
                    }
                    Ok(Some(code)) => {
                        self.last_fail.set(Some(*code));
                        self.refresh_general();
                    }
                    Err(_) => {
                        self.last_fail.set(Some(1));
                        self.refresh_general();
                    }
                }
            }
            if snapshot.install_result.is_some() && !self.install_seen.get() {
                self.install_seen.set(true);
                let ok = snapshot.install_result == Some(true);
                let play = self.play_after_install.replace(false);
                if play && ok {
                    self.installed.set(true);
                    self.start_scan();
                    self.begin_launch();
                } else {
                    self.install_failed.set(!ok && play);
                    self.start_scan();
                    self.refresh_general();
                }
            }
        }
        if self.install_active.get() || self.install_log.borrow().is_some() {
            self.tail_install();
        }
        if self.debug.borrow().is_some()
            && (self.debug_active.get() || self.debug_log.borrow().is_some())
        {
            self.tail_debug();
        }
        let expired = matches!(
            *self.notice.borrow(),
            Some((deadline, _)) if Instant::now() >= deadline
        );
        if expired {
            *self.notice.borrow_mut() = None;
            let snapshot = self.shared.snapshot();
            self.render_install(&snapshot);
        }
    }

    fn start_install(&self) {
        let snapshot = self.shared.snapshot();
        if snapshot.install_running {
            return;
        }
        if snapshot.launch_running || process::launcher_running() {
            unsafe { widgets::message(self.window, 1, &t("status.launcher_running")) };
            return;
        }
        self.status_scan_generation.fetch_add(1, Ordering::AcqRel);
        self.status_scan_dirty.store(false, Ordering::Release);
        let settings = self.settings.borrow().clone();
        self.install_failed.set(false);
        self.install_seen.set(false);
        self.log_path.borrow_mut().take();
        self.install_log.borrow_mut().take();
        self.log_offset.set(0);
        self.log_mirror.borrow_mut().clear();
        self.log_lines.set(0);
        unsafe {
            replace_log(self.installer.log_view, self.installer.log_buffer, "");
        }
        self.step_states
            .borrow_mut()
            .iter_mut()
            .for_each(|state| *state = "pending");
        let shared = Arc::clone(&self.shared);
        shared.update(|state| {
            state.install_running = true;
            state.install_states = ["pending"; STEP_COUNT];
            state.status_key = "install.status.running";
            state.packages.clear();
            state.command.clear();
            state.manager.clear();
            state.install_log = None;
            state.install_result = None;
            state.install_stopping = false;
        });
        let snapshot = self.shared.snapshot();
        self.last_rev.set(snapshot.rev);
        self.install_active.set(true);
        self.render_install(&snapshot);

        thread::spawn(move || {
            let path = log::start(&paths::logs_dir(), "install-").ok();
            if let Some(path) = &path {
                shared.update(|state| state.install_log = Some(path.clone()));
                log::line(&tf("gui.log_file", &[("path", &path.to_string_lossy())]));
            }
            let result = install::install(&settings, |event| match event {
                install::Event::Step(index, state) => {
                    shared.update(|shared| {
                        if index < shared.install_states.len() {
                            shared.install_states[index] = state;
                        }
                    });
                }
                install::Event::Packages(packages, command, manager) => {
                    shared.update(|shared| {
                        shared.status_key = if packages.is_empty() {
                            "install.status.running"
                        } else {
                            "install.status.waiting"
                        };
                        shared.packages = packages;
                        shared.command = command;
                        shared.manager = manager;
                    });
                }
                install::Event::Status(key) => {
                    shared.update(|shared| shared.status_key = key);
                }
            });
            let ok = match &result {
                Ok(()) => true,
                Err(error) => {
                    log::line(&tf("gui.error", &[("err", &error.to_string())]));
                    false
                }
            };
            shared.update(|shared| {
                shared.install_running = false;
                shared.install_result = Some(ok);
                shared.packages.clear();
                shared.command.clear();
                shared.manager.clear();
                shared.status_key = if ok {
                    "install.status.done"
                } else if shared.install_stopping {
                    "install.status.idle"
                } else {
                    "install.status.failed"
                };
            });
        });
    }

    fn start_play(&self) {
        let snapshot = self.shared.snapshot();
        if snapshot.launch_running || snapshot.install_running || snapshot.debug_running {
            return;
        }
        if !self.installed.get() {
            self.play_after_install.set(true);
            self.start_install();
            if !self.shared.snapshot().install_running {
                self.play_after_install.set(false);
            }
            return;
        }
        self.begin_launch();
    }

    fn begin_launch(&self) {
        let settings = self.settings.borrow().clone();
        if settings.debug {
            self.open_debug(settings);
            return;
        }
        self.last_fail.set(None);
        self.launch_seen.set(false);
        let shared = Arc::clone(&self.shared);
        shared.update(|state| {
            state.launch_running = true;
            state.launch_log = None;
            state.launch_result = None;
        });
        let snapshot = self.shared.snapshot();
        self.last_rev.set(snapshot.rev);
        self.render_play(&snapshot);
        self.refresh_general();

        thread::spawn(move || {
            let path = log::start(&paths::logs_dir(), "").ok();
            if let Some(path) = &path {
                shared.update(|state| state.launch_log = Some(path.clone()));
                log::line(&tf("gui.log_file", &[("path", &path.to_string_lossy())]));
            }
            log::line(&t("gui.launching"));
            let result = launch::launch(&settings, path.as_deref());
            match &result {
                Ok(Some(code)) => {
                    log::line(&tf("gui.finished", &[("code", &code.to_string())]));
                }
                Ok(None) => {}
                Err(error) => {
                    log::line(&tf("gui.error", &[("err", &error.to_string())]));
                }
            }
            shared.update(|state| {
                state.launch_running = false;
                state.launch_result = Some(result.map_err(|error| error.to_string()));
            });
        });
    }

    fn open_debug(&self, settings: Settings) {
        unsafe {
            let window = gtk_window_new(0);
            gtk_window_set_title(window, c(&t("debug.title")).as_ptr());
            gtk_window_set_default_size(window, 800, 500);
            gtk_window_set_position(window, 1);
            gtk_window_set_transient_for(window, self.window);
            if let Some(pixbuf) = png_pixbuf(ICON_PNG) {
                gtk_window_set_icon(window, pixbuf);
                g_object_unref(pixbuf);
            }
            let root = box_new(true, 10);
            style(root, "page");
            gtk_container_add(window, root);

            let scroll = gtk_scrolled_window_new(std::ptr::null_mut(), std::ptr::null_mut());
            gtk_scrolled_window_set_policy(scroll, 1, 1);
            style(scroll, "log-surface");
            let view = gtk_text_view_new();
            gtk_text_view_set_editable(view, 0);
            gtk_text_view_set_cursor_visible(view, 0);
            gtk_text_view_set_monospace(view, 1);
            gtk_text_view_set_wrap_mode(view, 3);
            log_margins(view);
            gtk_container_add(scroll, view);
            let buffer = gtk_text_view_get_buffer(view);
            pack(root, scroll, true);

            let bar = box_new(false, 8);
            let back = button_label(&t("debug.back"));
            style(back, "text-button");
            let stop = button_label(&t("debug.stop_prefix"));
            style(stop, "destructive");
            let copy = button_label(&t("btn.copy_log"));
            style(copy, "text-button");
            let logs = button_label(&t("btn.open_logs"));
            style(logs, "text-button");
            let status = label(&t("status.starting"), "caption");
            pack(bar, back, false);
            pack(bar, stop, false);
            pack(bar, copy, false);
            pack(bar, logs, false);
            pack(bar, box_new(false, 0), true);
            pack(bar, status, false);
            pack(root, bar, false);

            let data = app_ptr(self);
            connect(
                back,
                "clicked",
                debug_back_click as *const std::ffi::c_void,
                data,
            );
            connect(
                stop,
                "clicked",
                debug_stop_click as *const std::ffi::c_void,
                data,
            );
            connect(
                copy,
                "clicked",
                debug_copy_click as *const std::ffi::c_void,
                data,
            );
            connect(logs, "clicked", logs_click as *const std::ffi::c_void, data);
            connect(
                window,
                "delete-event",
                debug_delete as *const std::ffi::c_void,
                data,
            );
            connect(
                window,
                "destroy",
                debug_destroy as *const std::ffi::c_void,
                data,
            );

            self.shared.update(|state| {
                state.debug_running = true;
                state.debug_log = None;
                state.debug_stopping = false;
                state.debug_finished = None;
            });
            let win = Box::new(DebugWin {
                window,
                view,
                buffer,
                status,
                stop,
                settings: settings.clone(),
                log_path: RefCell::new(None),
                offset: Cell::new(0),
                mirror: RefCell::new(String::new()),
                lines: Cell::new(0),
            });
            *self.debug.borrow_mut() = Some(win);
            gtk_widget_show_all(window);
            gtk_widget_hide(self.window);

            let shared = Arc::clone(&self.shared);
            thread::spawn(move || {
                let path = log::start(&paths::logs_dir(), "").ok();
                if let Some(path) = &path {
                    shared.update(|state| state.debug_log = Some(path.clone()));
                    log::line(&tf("gui.log_file", &[("path", &path.to_string_lossy())]));
                }
                log::line(&t("gui.launching"));
                let result = launch::launch(&settings, None);
                match &result {
                    Ok(Some(code)) => {
                        log::line(&tf("gui.finished", &[("code", &code.to_string())]));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::line(&tf("gui.error", &[("err", &error.to_string())]));
                    }
                }
                let code = result.ok().flatten().unwrap_or(1);
                shared.update(|state| {
                    state.debug_running = false;
                    state.debug_stopping = false;
                    state.debug_finished = Some(code);
                });
            });
        }
    }

    fn stop_debug(&self) {
        let snapshot = self.shared.snapshot();
        if !snapshot.debug_running || snapshot.debug_stopping {
            return;
        }
        let settings = self.debug.borrow().as_ref().map(|win| win.settings.clone());
        self.shared.update(|state| {
            state.debug_stopping = true;
            state.debug_running = true;
        });
        thread::spawn(move || {
            let dir = settings.as_ref().map(install::active_proton_dir);
            process::stop(false, dir.as_deref());
            thread::sleep(Duration::from_secs(3));
            if process::launcher_running() {
                process::stop(true, dir.as_deref());
            }
        });
    }

    fn leave_debug(&self) {
        let Some(window) = self.debug.borrow().as_ref().map(|win| win.window) else {
            return;
        };
        self.debug_leaving.set(true);
        unsafe {
            gtk_widget_destroy(window);
            gtk_widget_show(self.window);
            gtk_window_present(self.window);
        }
    }

    fn save_settings(&self) {
        let settings = self.settings.borrow().clone();
        let _ = settings.save(&paths::config_ini());
    }

    fn set_language(&self, ru: bool) {
        let code = if ru { "ru" } else { "en" };
        i18n::set_language(code);
        {
            let mut settings = self.settings.borrow_mut();
            settings.language = code.to_string();
        }
        self.save_settings();
        self.retranslate();
    }

    fn save_args(&self) {
        let text = unsafe { entry_text(self.settings_page.entry) };
        {
            let mut settings = self.settings.borrow_mut();
            if settings.extra_args != text {
                settings.extra_args = text;
            }
        }
        self.save_settings();
    }

    fn render_update(&self) {
        let view = self.update_view.borrow().clone();
        let apply_label = self
            .update_release
            .borrow()
            .as_ref()
            .map(|release| tf("update.apply", &[("version", release.version.as_str())]));
        let (status, button) = match &view {
            UpdateView::Idle => (
                tf("update.current", &[("version", paths::APP_VERSION)]),
                t("update.check"),
            ),
            UpdateView::Checking => (t("update.checking"), t("update.check")),
            UpdateView::Latest => (
                tf("update.latest", &[("version", paths::APP_VERSION)]),
                t("update.check"),
            ),
            UpdateView::Available(version) => (
                tf("update.available", &[("version", version.as_str())]),
                apply_label.unwrap_or_else(|| t("update.check")),
            ),
            UpdateView::Downloading => (t("update.downloading"), t("update.check")),
            UpdateView::Restarting => (t("update.restarting"), t("update.check")),
            UpdateView::Failed(error) => (
                error.clone(),
                apply_label.unwrap_or_else(|| t("update.check")),
            ),
        };
        let snapshot = self.shared.snapshot();
        let idle = !snapshot.launch_running && !snapshot.install_running && !snapshot.debug_running;
        let enabled = idle && !self.update_busy.get();
        unsafe {
            set_text(self.about.update_status, &status);
            gtk_button_set_label(self.about.update_button, c(&button).as_ptr());
            gtk_widget_set_sensitive(self.about.update_button, enabled as c_int);
        }
    }

    fn start_update_check(&self, manual: bool) {
        if self.update_busy.replace(true) {
            return;
        }
        *self.update_view.borrow_mut() = UpdateView::Checking;
        self.render_update();
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            let result = selfupdate::latest().map_err(|error| error.to_string());
            shared.put_update(UpdateEvent::Checked(result, manual));
        });
    }

    fn start_update_apply(&self, release: selfupdate::Release) {
        if self.update_busy.replace(true) {
            return;
        }
        *self.update_view.borrow_mut() = UpdateView::Downloading;
        self.render_update();
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            let result = selfupdate::stage(&release)
                .and_then(|staged| selfupdate::apply(&staged))
                .map_err(|error| error.to_string());
            shared.put_update(UpdateEvent::Applied(result));
        });
    }

    fn update_clicked(&self) {
        let release = self.update_release.borrow().clone();
        match release {
            Some(release) => self.start_update_apply(release),
            None => self.start_update_check(true),
        }
    }

    fn handle_update(&self, event: UpdateEvent) {
        self.update_busy.set(false);
        match event {
            UpdateEvent::Checked(Ok(Some(release)), _) => {
                *self.update_view.borrow_mut() = UpdateView::Available(release.version.clone());
                *self.update_release.borrow_mut() = Some(release);
            }
            UpdateEvent::Checked(Ok(None), _) => {
                *self.update_release.borrow_mut() = None;
                *self.update_view.borrow_mut() = UpdateView::Latest;
            }
            UpdateEvent::Checked(Err(error), manual) => {
                *self.update_view.borrow_mut() = if manual {
                    UpdateView::Failed(tf("update.check_failed", &[("err", error.as_str())]))
                } else {
                    UpdateView::Idle
                };
            }
            UpdateEvent::Applied(Ok(target)) => {
                *self.update_view.borrow_mut() = UpdateView::Restarting;
                self.render_update();
                match selfupdate::restart(&target) {
                    Ok(()) => {
                        self.save_args();
                        log::shutdown();
                        unsafe {
                            gtk_widget_destroy(self.window);
                            gtk_main_quit();
                        }
                        return;
                    }
                    Err(error) => {
                        *self.update_view.borrow_mut() = UpdateView::Failed(tf(
                            "update.failed",
                            &[("err", error.to_string().as_str())],
                        ));
                    }
                }
            }
            UpdateEvent::Applied(Err(error)) => {
                *self.update_view.borrow_mut() =
                    UpdateView::Failed(tf("update.failed", &[("err", error.as_str())]));
            }
        }
        self.render_update();
    }

    fn pick_proton_folder(&self) {
        let start = paths::home().to_string_lossy().into_owned();
        let picked = unsafe {
            choose_folder(
                self.window,
                &t("settings.proton.pick_title"),
                &t("settings.proton.pick_accept"),
                &t("settings.proton.pick_cancel"),
                &start,
            )
        };
        let Some(raw) = picked else {
            return;
        };
        let path = PathBuf::from(raw);
        let real = fs::canonicalize(&path).unwrap_or(path);
        if proton::count_in(&real) == 0 {
            unsafe {
                set_text(
                    self.settings_page.proton_hint,
                    &t("settings.proton.custom_empty"),
                );
                gtk_widget_set_visible(self.settings_page.proton_hint, 1);
            }
            return;
        }
        let text = real.to_string_lossy().into_owned();
        {
            let mut settings = self.settings.borrow_mut();
            if !settings.custom.contains(&text) {
                settings.custom.push(text.clone());
            }
            proton::set_custom(&settings.custom);
        }
        self.save_settings();
        unsafe { gtk_widget_set_sensitive(self.settings_page.proton_clear, 1) };
        let direct = proton::is_proton_dir(&real);
        self.start_scan();
        if direct {
            let id = proton::normalize_id(&format!("path:{text}"));
            self.choose_proton(&id);
        }
    }

    fn clear_custom_protons(&self) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.custom.is_empty() {
                return;
            }
            settings.custom.clear();
            proton::set_custom(&settings.custom);
        }
        let current = self.settings.borrow().proton.clone();
        let reset = !proton::is_listed(&current);
        if reset {
            self.settings.borrow_mut().proton = String::new();
        }
        self.save_settings();
        unsafe { gtk_widget_set_sensitive(self.settings_page.proton_clear, 0) };
        self.start_scan();
        if reset {
            self.on_proton_changed();
        }
    }

    fn on_proton_changed(&self) {
        self.refresh_installed();
    }
}

fn push_mirror(mirror: &mut String, lines: &mut usize, text: &str) -> bool {
    *lines += text.bytes().filter(|byte| *byte == b'\n').count();
    mirror.push_str(text);
    if *lines <= 5200 {
        return false;
    }
    let mut index = mirror.len();
    let mut count = 0;
    for (position, character) in mirror.char_indices().rev() {
        if character == '\n' {
            count += 1;
            if count == 5000 {
                index = position + 1;
                break;
            }
        }
    }
    mirror.drain(..index);
    *lines = 5000;
    true
}

unsafe extern "C" fn notice_expire(data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    app.update();
    0
}

unsafe extern "C" fn initial_cb(data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    app.initial_state();
    0
}

unsafe extern "C" fn signal_quit(data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    let snapshot = app.shared.snapshot();
    if snapshot.install_running || snapshot.launch_running || snapshot.debug_running {
        process::stop(true, None);
    }
    app.save_args();
    log::shutdown();
    unsafe { gtk_main_quit() };
    0
}

unsafe extern "C" fn nav_click(_: W, data: W) {
    let info = unsafe { &*(data as *const NavData) };
    let app = unsafe { &*info.app };
    app.select_page(info.index as usize);
    match info.index {
        0 => {
            app.refresh_installed();
        }
        1 => {
            app.refresh_install_steps();
        }
        _ => {}
    }
}

unsafe extern "C" fn play_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.start_play();
}

unsafe extern "C" fn logs_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    if let Err(error) = install::open_logs_dir() {
        unsafe { widgets::message(app.window, 1, &error.to_string()) };
    }
}

unsafe extern "C" fn repo_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    let args = [paths::REPO_URL.to_string()];
    if let Err(error) = process::spawn_detached(std::path::Path::new("xdg-open"), &args, &[], None)
    {
        unsafe { widgets::message(app.window, 1, &error.to_string()) };
    }
}

unsafe extern "C" fn copy_log_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    let text = app.log_mirror.borrow().clone();
    unsafe { clipboard(&text) };
    app.set_notice("status.log_copied");
}

unsafe extern "C" fn copy_cmd_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    let text = unsafe { entry_text(app.installer.cmd) };
    unsafe {
        clipboard(&text);
        set_text(app.installer.status, &t("status.cmd_copied"));
    }
}

unsafe extern "C" fn lang_en_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.set_language(false);
}

unsafe extern "C" fn lang_ru_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.set_language(true);
}

unsafe extern "C" fn args_changed(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    let text = unsafe { entry_text(app.settings_page.entry) };
    unsafe {
        gtk_widget_set_sensitive(
            app.settings_page.reset,
            (text != paths::DEFAULT_ARGS) as c_int,
        );
    }
}

unsafe extern "C" fn args_save(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.save_args();
}

unsafe extern "C" fn args_focus_out(_: W, _: W, data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    app.save_args();
    0
}

unsafe extern "C" fn args_reset(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    unsafe {
        gtk_entry_set_text(app.settings_page.entry, c(paths::DEFAULT_ARGS).as_ptr());
        gtk_widget_set_sensitive(app.settings_page.reset, 0);
    }
    app.save_args();
}

unsafe extern "C" fn proton_option_click(_: W, data: W) {
    let option = unsafe { &*(data as *const OptionData) };
    let app = unsafe { &*option.app };
    let id = option.id.clone();
    app.choose_proton(&id);
}

unsafe extern "C" fn update_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.update_clicked();
}

unsafe extern "C" fn proton_pick_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.pick_proton_folder();
}

unsafe extern "C" fn proton_clear_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.clear_custom_protons();
}

unsafe extern "C" fn proton_rescan_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.start_scan();
}

unsafe extern "C" fn window_delete(_: W, _: W, data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    let snapshot = app.shared.snapshot();
    if snapshot.install_running || snapshot.launch_running {
        process::stop(true, None);
    }
    app.save_args();
    0
}

unsafe extern "C" fn quit_click(_: W, _: W) {
    log::shutdown();
    unsafe { gtk_main_quit() };
}

unsafe extern "C" fn debug_back_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.leave_debug();
}

unsafe extern "C" fn debug_stop_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    app.stop_debug();
}

unsafe extern "C" fn debug_copy_click(_: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    let text = app
        .debug
        .borrow()
        .as_ref()
        .map(|win| win.mirror.borrow().clone())
        .unwrap_or_default();
    unsafe { clipboard(&text) };
    if let Some(win) = app.debug.borrow().as_ref() {
        unsafe { set_text(win.status, &t("status.log_copied")) };
    }
}

unsafe extern "C" fn debug_delete(_: W, _: W, data: W) -> c_int {
    let app = unsafe { &*(data as *const App) };
    let snapshot = app.shared.snapshot();
    if snapshot.debug_running {
        let confirmed = unsafe { widgets::confirm(app.window, &t("debug.confirm_close")) };
        if !confirmed {
            return 1;
        }
        process::stop(true, None);
    }
    0
}

unsafe extern "C" fn debug_destroy(_: W, _: W, data: W) {
    let app = unsafe { &*(data as *const App) };
    *app.debug.borrow_mut() = None;
    if app.debug_leaving.get() {
        app.debug_leaving.set(false);
        return;
    }
    log::shutdown();
    unsafe {
        gtk_widget_destroy(app.window);
        gtk_main_quit();
    }
}

fn display_path(path: &std::path::Path) -> String {
    match path.strip_prefix(paths::home()) {
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

unsafe fn png_pixbuf(bytes: &[u8]) -> Option<W> {
    unsafe {
        let loader = gdk_pixbuf_loader_new_with_type(c("png").as_ptr(), std::ptr::null_mut());
        if loader.is_null() {
            return None;
        }
        let mut pixbuf = std::ptr::null_mut();
        if gdk_pixbuf_loader_write(loader, bytes.as_ptr(), bytes.len(), std::ptr::null_mut()) != 0
            && gdk_pixbuf_loader_close(loader, std::ptr::null_mut()) != 0
        {
            let raw = gdk_pixbuf_loader_get_pixbuf(loader);
            if !raw.is_null() {
                g_object_ref(raw);
                pixbuf = raw;
            }
        }
        g_object_unref(loader);
        if pixbuf.is_null() { None } else { Some(pixbuf) }
    }
}
