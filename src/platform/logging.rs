//! Filtered WASM console logger (replaces `wasm_logger`).
//!
//! Byte-identical console formatting to `wasm_logger` 0.2 (level badge +
//! `file:line` + styled args), plus a demotion table for known-benign
//! nostr-sdk diagnostics that otherwise flood the console at ERROR level.
//! SDK records reach the `log` facade through tracing's log-always interop
//! (no tracing subscriber is installed app-side), with `target()` set to
//! the callsite module path — which is what the table matches on.
//!
//! | Target | Message | Action |
//! |---|---|---|
//! | `nostr_sdk::relay::inner` | `subscription not found` | drop — relays pushing events for subs we already CLOSEd |
//! | `nostr_sdk::relay::inner` | `invalid message format` | drop — SDK 0.45 parser gap (NIP-11 `LIMITS`) |
//! | `nostr_sdk::client::api::fetch_events` | `Failed to handle streamed event` + `relay not connected` | drop — connection churn mid-stream |
//! | `nostr_sdk::client::api::fetch_events` | same + relay-limit errors (`too many subscriptions`, `kind not allowed`) | **warn** — real relay-side signals, reduced volume |
//! | `nostr_sdk::client::gossip::updater` | `relay banned` / `relay not connected` | drop — repeated once per gossip cycle |
//!
//! Everything else (all app logs included) passes through unchanged.
//! Default max level `Info`; override at runtime via the localStorage key
//! `nb_log_level` (`trace`/`debug`/`info`/`warn`/`error`/`off`) for dev
//! digs — see the DRY plan's log-cleanup workstream.
use log::{Level, LevelFilter, Log, Metadata, Record};
use wasm_bindgen::JsValue;
use web_sys::console;

/// localStorage key overriding the max log level (dev aid).
const LEVEL_OVERRIDE_KEY: &str = "nb_log_level";

/// Install the filtered logger as the global `log` implementation.
pub fn init() {
    let max_level = configured_level();
    let logger = FilteredLogger { max_level };
    match log::set_boxed_logger(Box::new(logger)) {
        Ok(()) => log::set_max_level(max_level),
        Err(e) => console::error_1(&JsValue::from(e.to_string())),
    }
}

fn configured_level() -> LevelFilter {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LEVEL_OVERRIDE_KEY).ok().flatten())
        .and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
            "trace" => Some(LevelFilter::Trace),
            "debug" => Some(LevelFilter::Debug),
            "info" => Some(LevelFilter::Info),
            "warn" => Some(LevelFilter::Warn),
            "error" => Some(LevelFilter::Error),
            "off" => Some(LevelFilter::Off),
            _ => None,
        })
        .unwrap_or(LevelFilter::Info)
}

struct FilteredLogger {
    max_level: LevelFilter,
}

/// Demotion table: the level a record should print at, or `None` to drop.
/// Only nostr-sdk targets are inspected (app logs pass through untouched —
/// and without paying the args-formatting allocation).
fn effective_level(record: &Record<'_>) -> Option<Level> {
    if !record.target().starts_with("nostr_sdk") {
        return Some(record.level());
    }
    let msg = record.args().to_string();
    if record.target().starts_with("nostr_sdk::relay::inner")
        && (msg.contains("subscription not found") || msg.contains("invalid message format"))
    {
        return None;
    }
    if record.target().starts_with("nostr_sdk::client::api::fetch_events")
        && msg.contains("Failed to handle streamed event")
    {
        if msg.contains("relay not connected") {
            return None;
        }
        // `too many subscriptions` / `kind not allowed` and other relay-side
        // rejections are real routing signals — keep them, at reduced volume.
        return Some(Level::Warn);
    }
    if record.target().starts_with("nostr_sdk::client::gossip::updater")
        && (msg.contains("relay banned") || msg.contains("relay not connected"))
    {
        return None;
    }
    Some(record.level())
}

impl Log for FilteredLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Some(level) = effective_level(record) else {
            return;
        };
        if level > self.max_level {
            return;
        }
        let style = Style::instance();
        let s = format!(
            "%c{}%c {}:{}%c {}",
            level,
            record.file().unwrap_or_else(|| record.target()),
            record
                .line()
                .map_or_else(|| "[Unknown]".to_string(), |line| line.to_string()),
            record.args(),
        );
        let s = JsValue::from_str(&s);
        let tgt_style = JsValue::from_str(&style.tgt);
        let args_style = JsValue::from_str(&style.args);
        match level {
            Level::Trace => console::debug_4(
                &s,
                &JsValue::from_str(&style.lvl_trace),
                &tgt_style,
                &args_style,
            ),
            Level::Debug => {
                console::log_4(&s, &JsValue::from_str(&style.lvl_debug), &tgt_style, &args_style)
            }
            Level::Info => {
                console::info_4(&s, &JsValue::from_str(&style.lvl_info), &tgt_style, &args_style)
            }
            Level::Warn => {
                console::warn_4(&s, &JsValue::from_str(&style.lvl_warn), &tgt_style, &args_style)
            }
            Level::Error => console::error_4(
                &s,
                &JsValue::from_str(&style.lvl_error),
                &tgt_style,
                &args_style,
            ),
        }
    }

    fn flush(&self) {}
}

/// Console CSS styles — identical to `wasm_logger` so output looks the same.
struct Style {
    lvl_trace: String,
    lvl_debug: String,
    lvl_info: String,
    lvl_warn: String,
    lvl_error: String,
    tgt: String,
    args: String,
}

impl Style {
    fn new() -> Self {
        let base = String::from("color: white; padding: 0 3px; background:");
        Self {
            lvl_trace: format!("{base} gray;"),
            lvl_debug: format!("{base} blue;"),
            lvl_info: format!("{base} green;"),
            lvl_warn: format!("{base} orange;"),
            lvl_error: format!("{base} darkred;"),
            tgt: String::from("font-weight: bold; color: inherit"),
            args: String::from("background: inherit; color: inherit"),
        }
    }

    fn instance() -> &'static Style {
        static STYLE: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
        STYLE.get_or_init(Style::new)
    }
}
