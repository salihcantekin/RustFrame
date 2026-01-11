//! Centralized logging infrastructure for RustFrame
//!
//! This module provides:
//! - Structured logging with tracing
//! - Configurable log levels (Off, Error, Warn, Info, Debug, Trace)
//! - Automatic daily log rotation
//! - Zero-cost when disabled (compile-time optimization)
//! - Cross-platform log file locations

use anyhow::{Context, Result};
use lazy_static::lazy_static;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Registry;

lazy_static! {
    // Global handle for reloading log level dynamically
    static ref LOG_RELOAD_HANDLE: Mutex<Option<Handle<EnvFilter, Registry>>> = Mutex::new(None);
}

/// Log level configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(LogLevel::Off),
            "error" => Ok(LogLevel::Error),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(format!("Invalid log level: {}", s)),
        }
    }
}

impl ToString for LogLevel {
    fn to_string(&self) -> String {
        match self {
            LogLevel::Off => "Off".to_string(),
            LogLevel::Error => "Error".to_string(),
            LogLevel::Warn => "Warn".to_string(),
            LogLevel::Info => "Info".to_string(),
            LogLevel::Debug => "Debug".to_string(),
            LogLevel::Trace => "Trace".to_string(),
        }
    }
}

impl From<LogLevel> for Option<Level> {
    fn from(log_level: LogLevel) -> Self {
        match log_level {
            LogLevel::Off => None,
            LogLevel::Error => Some(Level::ERROR),
            LogLevel::Warn => Some(Level::WARN),
            LogLevel::Info => Some(Level::INFO),
            LogLevel::Debug => Some(Level::DEBUG),
            LogLevel::Trace => Some(Level::TRACE),
        }
    }
}

/// Get the platform-specific logs directory
pub fn get_logs_dir() -> Result<PathBuf> {
    let logs_dir = if cfg!(target_os = "macos") {
        // macOS: ~/Library/Logs/RustFrame
        dirs::home_dir()
            .context("Failed to get home directory")?
            .join("Library")
            .join("Logs")
            .join("RustFrame")
    } else if cfg!(target_os = "windows") {
        // Windows: %LOCALAPPDATA%\RustFrame\logs
        dirs::data_local_dir()
            .context("Failed to get local data directory")?
            .join("RustFrame")
            .join("logs")
    } else {
        // Linux: ~/.local/share/RustFrame/logs
        dirs::data_local_dir()
            .context("Failed to get local data directory")?
            .join("RustFrame")
            .join("logs")
    };

    // Create directory if it doesn't exist
    if !logs_dir.exists() {
        fs::create_dir_all(&logs_dir)
            .with_context(|| format!("Failed to create logs directory: {:?}", logs_dir))?;
    }

    Ok(logs_dir)
}

/// Initialize the logging system
///
/// # Arguments
/// * `log_level` - The minimum log level to record
/// * `log_to_file` - Whether to write logs to file
///
/// # Returns
/// * `Ok(())` if logging was initialized successfully
/// * `Err(anyhow::Error)` if initialization failed
pub fn init_logging(log_level: LogLevel, log_to_file: bool) -> Result<()> {
    let level_filter = if log_level == LogLevel::Off {
        EnvFilter::new("off")
    } else {
        let level: Option<Level> = log_level.into();
        if let Some(lvl) = level {
            EnvFilter::new(format!("rustframe={}", lvl.as_str())).add_directive(
                format!("rustframe_capture={}", lvl.as_str())
                    .parse()
                    .unwrap(),
            )
        } else {
            EnvFilter::new("rustframe=error")
        }
    };

    // Check if logging is already initialized
    let mut handle_guard = LOG_RELOAD_HANDLE.lock().unwrap();
    if let Some(handle) = handle_guard.as_ref() {
        // Logging already initialized, just reload the filter
        handle.reload(level_filter).context("Failed to reload log filter")?;
        return Ok(());
    }

    // First time initialization
    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(level_filter);
    *handle_guard = Some(reload_handle);

    // Standard stdout/stderr layer
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    // File layer (optional)
    // To keep the subscriber type consistent, we always construct the optional layer
    // but only enable it if needed. However, since init() is only called once,
    // we must decide on file logging at startup.
    // If we want to support toggling file logging, we'd need another reload layer 
    // or an Option layer. For simplicity, we stick to the startup choice for file logging structure,
    // but we can at least avoid the panic.
    
    if log_to_file {
        let logs_dir = get_logs_dir()?;
        let appender = RollingFileAppender::new(Rotation::DAILY, &logs_dir, "rustframe.log");
        
        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(appender)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(false)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(file_layer)
            .init();
            
        // Note: The appender usage here uses a worker guard which is returned by non_blocking
        // but RollingFileAppender::new directly returns a writer that blocks (or manages itself).
        // The original code used non_blocking which returns a guard.
        // If we want non-blocking, we need to handle the guard.
        // But for now, let's look at how the original code did it.
        // It didn't assign the guard to anything, meaning it dropped immediately 
        // if using tracing_appender::non_blocking.
        // BUT the original code used `tracing_appender::rolling::RollingFileAppender` directly?
        // Let's check the readout again.
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }

    Ok(())
}


/// Clean up old log files
///
/// # Arguments
/// * `logs_dir` - Directory containing log files
/// * `keep_days` - Number of days to keep (files older than this will be deleted)
///
/// # Returns
/// * Number of files deleted
pub fn cleanup_old_logs(logs_dir: &Path, keep_days: u32) -> Result<usize> {
    let now = std::time::SystemTime::now();
    let keep_duration = std::time::Duration::from_secs(keep_days as u64 * 24 * 60 * 60);

    let mut deleted_count = 0;

    for entry in fs::read_dir(logs_dir)
        .with_context(|| format!("Failed to read logs directory: {:?}", logs_dir))?
    {
        let entry = entry?;
        let path = entry.path();

        // Only process .log files
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }

        // Get file metadata
        let metadata = entry.metadata()?;
        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = now.duration_since(modified) {
                if age > keep_duration {
                    // File is older than keep_days, delete it
                    if fs::remove_file(&path).is_ok() {
                        deleted_count += 1;
                        tracing::debug!(file = ?path, age_days = age.as_secs() / 86400, "Deleted old log file");
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Auto-cleanup old logs on startup (runs in background)
pub fn auto_cleanup_old_logs(keep_days: u32) {
    std::thread::spawn(move || {
        if let Ok(logs_dir) = get_logs_dir() {
            match cleanup_old_logs(&logs_dir, keep_days) {
                Ok(count) if count > 0 => {
                    tracing::info!(deleted_count = count, "Cleaned up old log files");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to cleanup old log files");
                }
                _ => {}
            }
        }
    });
}
