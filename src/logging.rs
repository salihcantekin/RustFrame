//! Centralized logging infrastructure for RustFrame
//!
//! This module provides:
//! - Structured logging with tracing
//! - Configurable log levels (Off, Error, Warn, Info, Debug, Trace)
//! - Automatic daily log rotation
//! - Zero-cost when disabled (compile-time optimization)
//! - Cross-platform log file locations

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

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
    // If logging is completely off, install a no-op subscriber
    if log_level == LogLevel::Off {
        tracing_subscriber::registry().init();
        return Ok(());
    }

    let level: Option<Level> = log_level.into();
    let filter = if let Some(lvl) = level {
        // Create filter with the specified level
        EnvFilter::new(format!("rustframe={}", lvl.as_str())).add_directive(
            format!("rustframe_capture={}", lvl.as_str())
                .parse()
                .unwrap(),
        )
    } else {
        // Should not happen (Off is handled above), but default to ERROR
        EnvFilter::new("rustframe=error")
    };

    let registry = tracing_subscriber::registry().with(filter);

    if log_to_file {
        // File logging enabled
        let logs_dir = get_logs_dir()?;

        // Create a daily rolling file appender with date in filename
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("rustframe")
            .filename_suffix("log")
            .build(logs_dir)
            .context("Failed to create rolling file appender")?;

        // Create non-blocking writer for async file I/O
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        // File layer with structured format
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false) // No ANSI colors in file
            .with_target(true) // Include module path
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_line_number(true)
            .with_span_events(FmtSpan::CLOSE);

        // Console layer - enabled in all modes for debugging
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true) // Colors in console
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_line_number(false)
            .with_span_events(FmtSpan::NONE);

        // Combine layers (both file and console)
        let registry = registry.with(file_layer).with(console_layer);

        registry.init();

        // Store the guard to prevent file handle from being dropped
        // We leak it because logging is a singleton that lives for the app lifetime
        std::mem::forget(_guard);
    } else {
        // Console-only logging (development mode)
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_line_number(false)
            .with_span_events(FmtSpan::NONE);

        registry.with(console_layer).init();
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
