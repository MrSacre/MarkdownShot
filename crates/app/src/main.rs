#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;

mod app;
mod cli;
mod config;
mod obsidian;
mod icon;
mod overlay;
mod platform;
mod recorder;
mod scrolling;
mod text;
mod toast;
mod update;
mod ui;
mod worker;

use std::io::Write;
use std::sync::Mutex;

struct Logger {
    file: Mutex<Option<std::fs::File>>,
    stderr: bool,
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::max_level()
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) || !r.target().starts_with("markdownshot") {
            return;
        }
        let line = format!("{} [{}] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), r.level(), r.args());
        if self.stderr {
            eprint!("{line}");
        }
        if let Ok(mut f) = self.file.lock() {
            if let Some(f) = f.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
    fn flush(&self) {}
}

fn init_logging(stderr: bool) {
    let file = config::Config::path().parent().and_then(|d| {
        let _ = std::fs::create_dir_all(d);
        let p = d.join("markdownshot.log");
        if std::fs::metadata(&p).map(|m| m.len() > 2_000_000).unwrap_or(false) {
            let _ = std::fs::remove_file(&p);
        }
        std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
    });
    let logger = Box::leak(Box::new(Logger { file: Mutex::new(file), stderr }));
    let _ = log::set_logger(logger);
    let level =
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    log::set_max_level(level);
}

fn main() {
    i18n::set(&config::Config::peek_language());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut open_image = None;
    if let [cmd, path] = args.as_slice()
        && matches!(cmd.as_str(), "open" | "snipe")
    {
        let snipe = cmd == "snipe";
        open_image = Some((std::path::PathBuf::from(path), snipe));
    } else if let Some(cmd) = args.first() {
        platform::attach_console();
        match cmd.as_str() {
            "--version" | "-V" => {
                println!("markdownshot {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" | "help" => {
                println!("{}", cli::help());
                return;
            }
            _ => {
                init_logging(true);
                if let Err(e) = cli::run(cmd, &args[1..]) {
                    eprintln!("{} {e:#}", t!("Erreur :", "Error:"));
                    std::process::exit(1);
                }
                return;
            }
        }
    }

    init_logging(false);
    log::info!("starting MarkdownShot {}", env!("CARGO_PKG_VERSION"));
    if !platform::single_instance() {
        platform::message_box(
            "MarkdownShot",
            t!(
                "MarkdownShot est déjà lancé : son icône se trouve près de l'horloge.",
                "MarkdownShot is already running: its icon is next to the clock."
            ),
        );
        return;
    }
    let (cfg, first_run) = match config::Config::load() {
        Ok(c) => {
            i18n::set(&c.0.interface.language);
            c
        }
        Err(e) => {
            platform::message_box(
                t!("MarkdownShot - configuration invalide", "MarkdownShot - invalid configuration"),
                &tf!("{e:#}\n\nLa configuration par défaut est utilisée.", "{e:#}\n\nThe default configuration is used."),
            );
            (config::Config::default(), false)
        }
    };
    let event_loop = match winit::event_loop::EventLoop::<app::UserEvent>::with_user_event().build() {
        Ok(e) => e,
        Err(e) => {
            platform::message_box("MarkdownShot", &tf!("Impossible de démarrer : {e}", "Cannot start: {e}"));
            return;
        }
    };
    let mut app = app::App::new(event_loop.create_proxy(), cfg, first_run);
    app.open_on_start = open_image;
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
}
