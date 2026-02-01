//! Error handling utilities for consistent error logging across the codebase.
/// Log a fetch/async operation error with context.
///
/// Provides consistent error logging format for fetch operations, making it
/// easier to track down issues in the logs. Uses `log::warn` level.
///
/// # Arguments
/// * `context` - A description of what operation failed (e.g., "profile metadata", "citation data")
/// * `error` - The error that occurred (implements Display)
///
/// # Examples
/// ```
/// match fetch_profile(&pubkey).await {
///     Ok(profile) => // handle success
///     Err(e) => log_fetch_error("profile metadata", e),
/// }
/// ```
pub fn log_fetch_error(context: &str, error: impl std::fmt::Display) {
    log::warn!("[fetch] Failed to fetch {}: {}", context, error);
}
/// Log a parse/decode error with context.
///
/// Use this for parsing failures (JSON, timestamps, etc.) rather than fetch errors.
///
/// # Arguments
/// * `context` - A description of what was being parsed (e.g., "JSON response", "timestamp")
/// * `error` - The error that occurred
#[allow(dead_code)]
pub fn log_parse_error(context: &str, error: impl std::fmt::Display) {
    log::warn!("[parse] Failed to parse {}: {}", context, error);
}
/// Log an error and return a user-friendly error message.
///
/// Useful when you need to both log the error and display something to the user.
///
/// # Arguments
/// * `context` - What operation failed
/// * `error` - The technical error
///
/// # Returns
/// A user-friendly error string like "Failed to load profile"
#[allow(dead_code)]
pub fn log_and_format_error(context: &str, error: impl std::fmt::Display) -> String {
    log::warn!("[error] Failed to {}: {}", context, error);
    format!("Failed to {}", context)
}
