use env_logger::Builder;
use log::LevelFilter;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let mut output = String::new();

        if config.show_timestamps {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or_default();
            output.push_str(&format!("[{}] ", timestamp));
        }

        output.push_str(&format!("{}: ", record.level()));

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
