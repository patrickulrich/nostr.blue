//! Filtered WASM console logger (replaces `wasm_logger`) + tracing→log bridge.
//!
//! **WASM:** byte-identical console formatting to `wasm_logger` 0.2 (level
//! badge + `file:line` + styled args), plus a demotion table for known-benign
//! nostr-sdk diagnostics that otherwise flood the console at ERROR level.
//! SDK records reach the `log` facade with `target()` set to the callsite
//! module path — which is what the table matches on.
//!
//! **Load-bearing invariant (why `init_tracing_bridge` exists):** the dioxus
//! `logger` cargo feature must stay off in OUR manifest, but
//! `dioxus-primitives` re-enables it transitively via its default features.
//! Under that feature, `dioxus::launch` (dioxus-0.7.9 launch.rs:299) calls
//! `dioxus_logger::initialize_default()`, which on wasm32 installs
//! `tracing_wasm::WASMLayer` as the global tracing subscriber. Once
//! `dispatcher::has_been_set()` is true, tracing's own log bridge
//! (`if_log_enabled!` in tracing-0.1.44 macros.rs) stops firing and every
//! SDK diagnostic prints via tracing-wasm's own console path — unfiltered,
//! invisible to the demotion table. (This is exactly what happened on the
//! first deployment of this logger: the table silently matched nothing.)
//! `init_tracing_bridge` pre-installs a forwarding subscriber, which both
//! makes dioxus-logger's auto-init a no-op (`has_been_set()` already true)
//! and routes every tracing event into the `log` facade ourselves.
//!
//! | Target | Message | Action |
//! |---|---|---|
//! | `nostr_sdk::relay::inner` | `subscription not found` | drop — relays pushing events for subs we already CLOSEd |
//! | `nostr_sdk::relay::inner` | `invalid message format` | drop — SDK 0.45 parser gap (NIP-11 `LIMITS`) |
//! | `nostr_sdk::relay::inner` | `relay URL does not match` | **warn** — SDK declining mismatched NIP-42 challenges (proxy/path relays) |
//! | `nostr_sdk::client::api::fetch_events` | `Failed to handle streamed event` + `relay not connected` | drop — connection churn mid-stream |
//! | `nostr_sdk::client::api::fetch_events` | same + relay-limit errors (`too many subscriptions`, `kind not allowed`) | **warn** — real relay-side signals, reduced volume |
//! | `nostr_sdk::client::gossip::updater` | `relay banned` / `relay not connected` | drop — repeated once per gossip cycle |
//!
//! Everything else (all app logs included) passes through unchanged.
//! Default max level `Info`; override at runtime via the localStorage key
//! `nb_log_level` (`trace`/`debug`/`info`/`warn`/`error`/`off`) for dev
//! digs — see the DRY plan's log-cleanup workstream.
#[cfg(target_arch = "wasm32")]
use log::{Level, LevelFilter, Log, Metadata, Record};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use web_sys::console;

/// Install the tracing→log forwarding subscriber. MUST run before
/// `dioxus::launch` on every target (see module docs). Idempotent and
/// race-safe: if some other subscriber was already installed we keep it.
pub fn init_tracing_bridge() {
    use tracing_subscriber::layer::SubscriberExt;
    let reg = tracing_subscriber::registry().with(TracingToLog);
    if tracing::subscriber::set_global_default(reg).is_err() {
        // Someone installed a subscriber first (tests, embedding app). Leave
        // theirs active; ours only forwards anyway.
        log::debug!("tracing->log bridge: subscriber already set, skipping");
    }
}

/// Minimal [`tracing_subscriber::Layer`] that forwards events to the `log`
/// crate with the callsite module path as target — mirroring what tracing's
/// own (subscriber-gated) log bridge would have done. Spans are no-ops.
struct TracingToLog;

impl<S> tracing_subscriber::Layer<S> for TracingToLog
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        let Some(level) = log_level(meta.level()) else {
            return;
        };
        // Cheap gate honoring the log facade's max level and logger before
        // paying for field formatting.
        if !log::log_enabled!(target: meta.target(), level) {
            return;
        }
        let mut fields = FieldCollector::default();
        event.record(&mut fields);
        let message = format!("{}{}", fields.message, fields.rest);
        // Build the record with the *callsite's* file/line/module (mirroring
        // tracing's own bridge) so SDK diagnostics keep their informative
        // origins instead of this forwarding line. The build + log call must
        // stay in one expression statement: `format_args!` internals are
        // temporaries that die at the end of a statement.
        let log_meta = log::Metadata::builder()
            .level(level)
            .target(meta.target())
            .build();
        log::logger().log(
            &log::Record::builder()
                .metadata(log_meta)
                .file(meta.file())
                .line(meta.line())
                .module_path(meta.module_path())
                .args(format_args!("{message}"))
                .build(),
        );
    }
}

fn log_level(level: &tracing::Level) -> Option<log::Level> {
    match *level {
        tracing::Level::ERROR => Some(log::Level::Error),
        tracing::Level::WARN => Some(log::Level::Warn),
        tracing::Level::INFO => Some(log::Level::Info),
        tracing::Level::DEBUG => Some(log::Level::Debug),
        tracing::Level::TRACE => Some(log::Level::Trace),
    }
}

/// Renders an event's fields as `message field=value …` (same shape tracing's
/// own log bridge produces, so the demotion table's substring checks match).
#[derive(Default)]
struct FieldCollector {
    message: String,
    rest: String,
}

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.rest.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

/// localStorage key overriding the max log level (dev aid).
#[cfg(target_arch = "wasm32")]
const LEVEL_OVERRIDE_KEY: &str = "nb_log_level";

/// Install the filtered WASM console logger (plus the tracing bridge).
/// On non-wasm32 targets (host `cargo check` with the web feature, tests)
/// only the tracing→log bridge is installed; the log implementation there
/// is env_logger, set up by the caller.
pub fn init() {
    #[cfg(target_arch = "wasm32")]
    {
        let max_level = configured_level();
        let logger = FilteredLogger { max_level };
        match log::set_boxed_logger(Box::new(logger)) {
            Ok(()) => log::set_max_level(max_level),
            Err(e) => console::error_1(&JsValue::from(e.to_string())),
        }
    }
    // After the log implementation so forwarded events can never race the
    // facade being empty (log's default max level is Off until set).
    init_tracing_bridge();
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
struct FilteredLogger {
    max_level: LevelFilter,
}

/// Demotion table: the level a record should print at, or `None` to drop.
/// Only nostr-sdk targets are inspected (app logs pass through untouched —
/// and without paying the args-formatting allocation).
#[cfg(target_arch = "wasm32")]
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
    // NIP-42 AUTH challenges whose relay URL doesn't match the connection
    // URL (proxy/path relays like filter.nostr.wine/npub…, orly.dev): the
    // SDK declines these by design; not actionable client-side.
    if record.target().starts_with("nostr_sdk::relay::inner")
        && msg.contains("relay URL does not match")
    {
        return Some(Level::Warn);
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

#[cfg(target_arch = "wasm32")]
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
#[cfg(target_arch = "wasm32")]
struct Style {
    lvl_trace: String,
    lvl_debug: String,
    lvl_info: String,
    lvl_warn: String,
    lvl_error: String,
    tgt: String,
    args: String,
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the load-bearing contract of `init_tracing_bridge`: nostr-sdk
    /// `tracing::` events must arrive at the `log` facade with the callsite
    /// module path as `target()` (that's what the demotion table matches on)
    /// and the message field rendered into the args. Without our forwarding
    /// subscriber, records only reach the console via dioxus-logger's
    /// tracing-wasm layer — unfiltered.
    #[test]
    fn sdk_tracing_events_forward_to_log_facade_with_module_target() {
        use std::sync::Mutex;
        use std::time::Duration;

        static CAPTURED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
        struct Probe;
        impl log::Log for Probe {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, r: &log::Record<'_>) {
                if r.target().starts_with("nostr_sdk") {
                    CAPTURED
                        .lock()
                        .unwrap()
                        .push((r.target().to_string(), r.args().to_string()));
                }
            }
            fn flush(&self) {}
        }

        init_tracing_bridge();
        let _ = log::set_boxed_logger(Box::new(Probe)); // sole installer in this test binary
        log::set_max_level(log::LevelFilter::Info);

        // Connecting to a dead localhost port emits `Connection failed.` from
        // nostr_sdk::relay::inner — the same callsite class the demotion
        // table targets. Localhost only; no external network needed.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let client = nostr_sdk::client::Client::default();
            client.add_relay("ws://127.0.0.1:1").await.unwrap();
            client.connect().await;
            for _ in 0..20 {
                if !CAPTURED.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            client.shutdown().await;
        });

        let captured = CAPTURED.lock().unwrap();
        assert!(
            captured.iter().any(|(t, a)| t.starts_with("nostr_sdk::relay::inner")
                && a.contains("Connection failed")),
            "expected a nostr_sdk::relay::inner 'Connection failed' record at the log facade, got: {captured:?}"
        );
    }
}
