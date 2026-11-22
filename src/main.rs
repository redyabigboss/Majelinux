mod core;
mod ffi;
mod i18n;
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!(
            "{} {}",
            crate::core::paths::APP_NAME,
            crate::core::paths::APP_VERSION
        );
        return;
    }
    if let Some(code) = crate::core::trim::from_args(&args) {
        std::process::exit(code);
    }
    if let Some(pid) = crate::core::update::wait_pid_arg(&args) {
        crate::core::update::wait_for_exit(pid);
    }
    if let Err(error) = ui::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
