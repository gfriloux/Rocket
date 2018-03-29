//! Rocket's logging infrastructure.

use std::str::FromStr;
use std::fmt;

use log;
use yansi::Paint;

struct RocketLogger(LoggingLevel);

/// Defines the different levels for log messages.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum LoggingLevel {
    /// Only shows errors, warnings, and launch information.
    Critical,
    /// Shows everything except debug and trace information.
    Normal,
    /// Shows everything.
    Debug,
}

impl LoggingLevel {
    #[inline(always)]
    fn max_log_level(&self) -> log::Level {
        match *self {
            LoggingLevel::Critical => log::Level::Warn,
            LoggingLevel::Normal => log::Level::Info,
            LoggingLevel::Debug => log::Level::Trace,
        }
    }
}

impl FromStr for LoggingLevel {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let level = match s {
            "critical" => LoggingLevel::Critical,
            "normal" => LoggingLevel::Normal,
            "debug" => LoggingLevel::Debug,
            _ => return Err("a log level (debug, normal, critical)")
        };

        Ok(level)
    }
}

impl fmt::Display for LoggingLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let string = match *self {
            LoggingLevel::Critical => "critical",
            LoggingLevel::Normal => "normal",
            LoggingLevel::Debug => "debug",
        };

        write!(f, "{}", string)
    }
}

#[doc(hidden)] #[macro_export]
macro_rules! log_ { ($name:ident: $($args:tt)*) => { $name!(target: "_", $($args)*) }; }
#[doc(hidden)] #[macro_export]
macro_rules! launch_info { ($($args:tt)*) => { info!(target: "launch", $($args)*) } }
#[doc(hidden)] #[macro_export]
macro_rules! launch_info_ { ($($args:tt)*) => { info!(target: "launch_", $($args)*) } }
#[doc(hidden)] #[macro_export]
macro_rules! error_ { ($($args:expr),+) => { log_!(error: $($args),+); }; }
#[doc(hidden)] #[macro_export]
macro_rules! info_ { ($($args:expr),+) => { log_!(info: $($args),+); }; }
#[doc(hidden)] #[macro_export]
macro_rules! trace_ { ($($args:expr),+) => { log_!(trace: $($args),+); }; }
#[doc(hidden)] #[macro_export]
macro_rules! debug_ { ($($args:expr),+) => { log_!(debug: $($args),+); }; }
#[doc(hidden)] #[macro_export]
macro_rules! warn_ { ($($args:expr),+) => { log_!(warn: $($args),+); }; }

impl log::Log for RocketLogger {
    #[inline(always)]
    fn enabled(&self, record: &log::Metadata) -> bool {
        record.target().starts_with("launch") || record.level() <= self.0.max_log_level()
    }

    fn log(&self, record: &log::Record) {
        // Print nothing if this level isn't enabled and this isn't launch info.
        if !self.enabled(record.metadata()) {
            return;
        }

        // Don't print Hyper or Rustls messages unless debug is enabled.
        let configged_level = self.0;
        let from_hyper = record.module_path().map_or(false, |m| m.starts_with("hyper::"));
        let from_rustls = record.module_path().map_or(false, |m| m.starts_with("rustls::"));
        if configged_level != LoggingLevel::Debug && (from_hyper || from_rustls) {
            return;
        }

        // In Rocket, we abuse targets with suffix "_" to indicate indentation.
        if record.target().ends_with('_') {
            if configged_level != LoggingLevel::Critical || record.target().starts_with("launch") {
                print!("    {} ", Paint::white("=>"));
            }
        }

        match record.level() {
            log::Level::Info => println!("{}", Paint::blue(record.args())),
            log::Level::Trace => println!("{}", Paint::purple(record.args())),
            log::Level::Error => {
                println!("{} {}",
                         Paint::red("Error:").bold(),
                         Paint::red(record.args()))
            }
            log::Level::Warn => {
                println!("{} {}",
                         Paint::yellow("Warning:").bold(),
                         Paint::yellow(record.args()))
            }
            log::Level::Debug => {
                print!("\n{} ", Paint::blue("-->").bold());
                if let Some(file) = record.file() {
                    print!("{}", Paint::blue(file));
                }

                if let Some(line) = record.line() {
                    println!(":{}", Paint::blue(line));
                }

                println!("{}", record.args());
            }
        }
    }

    fn flush(&self) {
        // NOOP: We don't buffer any records.
    }
}

pub(crate) fn try_init(level: LoggingLevel, verbose: bool) {
    if !::isatty::stdout_isatty() {
        Paint::disable();
    } else if cfg!(windows) {
        Paint::enable_windows_ascii();
    }

    push_max_level(level);
    if let Err(e) = log::set_boxed_logger(Box::new(RocketLogger(level))) {
        if verbose {
            eprintln!("Logger failed to initialize: {}", e);
        }
    }
}

use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};

static PUSHED: AtomicBool = AtomicBool::new(false);
static LAST_LOG_FILTER: AtomicUsize = AtomicUsize::new(0);

fn filter_to_usize(filter: log::LevelFilter) -> usize {
    match filter {
        log::LevelFilter::Off => 0,
        log::LevelFilter::Error => 1,
        log::LevelFilter::Warn => 2,
        log::LevelFilter::Info => 3,
        log::LevelFilter::Debug => 4,
        log::LevelFilter::Trace => 5,
    }
}

fn usize_to_filter(num: usize) -> log::LevelFilter {
    match num {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => unreachable!("max num is 5 in filter_to_usize")
    }
}

pub(crate) fn push_max_level(level: LoggingLevel) {
    LAST_LOG_FILTER.store(filter_to_usize(log::max_level()), Ordering::Release);
    PUSHED.store(true, Ordering::Release);
    log::set_max_level(level.max_log_level().to_level_filter());
}

pub(crate) fn pop_max_level() {
    if PUSHED.load(Ordering::Acquire) {
        log::set_max_level(usize_to_filter(LAST_LOG_FILTER.load(Ordering::Acquire)));
    }
}

#[doc(hidden)]
pub fn init(level: LoggingLevel) {
    try_init(level, true)
}
