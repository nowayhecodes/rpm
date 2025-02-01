use log::{LevelFilter, Record};
use env_logger::{Builder, fmt::{Color, Style, StyledValue}};
use std::io::Write;

pub struct LoggingConfig {
    pub level: LevelFilter,
    pub show_timestamps: bool,
    pub show_module_path: bool,
    pub color: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            show_timestamps: true,
            show_module_path: false,
            color: true,
        }
    }
}

pub fn setup_logging(config: LoggingConfig) {
    let mut builder = Builder::new();

    builder.format(move |buf, record| {
        let mut style = buf.style();

        let level = colored_level(&mut style, record.level());

        let mut output = String::new();

        if config.show_timestamps {
            output.push_str(&format!("[{}] ", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        }

        output.push_str(&format!("{}: ", level));

        if config.show_module_path {
            if let Some(module_path) = record.module_path() {
                output.push_str(&format!("[{}] ", module_path));
            }
        }

        output.push_str(&format!("{}", record.args()));
        writeln!(buf, "{}", output)
    });

    builder.filter_level(config.level);
    builder.init();
}

fn colored_level(style: &mut Style, level: log::Level) -> StyledValue<&'static str> {
    match level {
        log::Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        log::Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        log::Level::Info => style.set_color(Color::Green).value("INFO"),
        log::Level::Warn => style.set_color(Color::Yellow).value("WARN"),
        log::Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
} 