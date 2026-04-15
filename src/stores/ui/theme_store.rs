use crate::platform::storage;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use serde::{Deserialize, Serialize};
#[cfg(feature = "native")]
use std::sync::Once;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}
impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        }
    }
}
/// Global theme state
pub static THEME: GlobalSignal<Theme> = Signal::global(Theme::default);
const STORAGE_KEY: &str = "nostr_theme";

#[cfg(feature = "native")]
static SYSTEM_THEME_IS_DARK: GlobalSignal<bool> = Signal::global(|| false);
#[cfg(feature = "native")]
static NATIVE_THEME_LISTENER_STARTED: Once = Once::new();

/// Initialize theme from localStorage or system preference
pub fn init_theme() {
    if let Ok(theme_str) = storage::get::<String>(STORAGE_KEY) {
        let theme = Theme::from_str(&theme_str);
        *THEME.write() = theme;
        log::info!("Loaded theme from storage: {:?}", theme);
    } else {
        *THEME.write() = Theme::System;
        log::info!("Using system theme preference");
    }

    #[cfg(feature = "native")]
    start_native_theme_listener();

    apply_theme();
}
/// Set theme UI state only (internal use, no Nostr sync)
pub fn set_theme_internal(theme: Theme) {
    if *THEME.read() == theme {
        return;
    }
    *THEME.write() = theme;
    if let Err(e) = storage::set(STORAGE_KEY, &theme.as_str()) {
        log::warn!("Failed to persist theme to storage: {}", e);
    }
    log::info!("Theme changed to: {:?}", theme);
    apply_theme();
}
/// Set theme and persist to localStorage and Nostr (NIP-78)
pub fn set_theme(theme: Theme) {
    set_theme_internal(theme);
    #[cfg(feature = "web")]
    {
        use crate::stores::settings_store;
        dioxus::prelude::spawn(async move {
            settings_store::update_theme(theme).await;
        });
    }
}
/// Apply theme to document
pub fn apply_theme() {
    #[cfg(feature = "web")]
    {
        use web_sys::window;
        if let Some(win) = window() {
            if let Some(document) = win.document() {
                if let Some(root) = document.document_element() {
                    let class = if is_dark_mode() { "dark" } else { "" };
                    root.set_attribute("class", class).ok();
                }
            }
        }
    }
    #[cfg(feature = "native")]
    {
        let is_dark = is_dark_mode();
        let class = if is_dark { "dark" } else { "" };
        let color_scheme = if is_dark { "dark" } else { "light" };
        let js = format!(
            "document.documentElement.setAttribute('class', '{}'); document.documentElement.style.colorScheme = '{}';",
            class,
            color_scheme
        );
        dioxus::prelude::spawn(async move {
            if let Err(e) = dioxus::prelude::document::eval(&js).await {
                log::warn!("Failed to apply theme on native: {:?}", e);
            }
        });
    }
}
/// Get current theme
#[allow(dead_code)]
pub fn get_theme() -> Theme {
    *THEME.read()
}
/// Toggle between light and dark themes
#[allow(dead_code)]
pub fn toggle_theme() {
    let current = *THEME.read();
    let new_theme = match current {
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::Light,
        Theme::System => Theme::Dark,
    };
    set_theme(new_theme);
}
/// Check if dark mode is active
#[allow(dead_code)]
pub fn is_dark_mode() -> bool {
    match *THEME.read() {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => {
            #[cfg(feature = "web")]
            {
                use web_sys::window;
                if let Some(window) = window() {
                    let media_query = "(prefers-color-scheme: dark)";
                    if let Ok(Some(match_media)) = window.match_media(media_query) {
                        return match_media.matches();
                    }
                }
                false
            }
            #[cfg(feature = "native")]
            {
                *SYSTEM_THEME_IS_DARK.read()
            }
        }
    }
}

#[cfg(feature = "native")]
fn start_native_theme_listener() {
    NATIVE_THEME_LISTENER_STARTED.call_once(|| {
        dioxus::prelude::spawn(async move {
            let mut eval = dioxus::prelude::document::eval(
                r#"
                const query = "(prefers-color-scheme: dark)";
                const media = typeof window.matchMedia === "function"
                    ? window.matchMedia(query)
                    : null;

                dioxus.send(Boolean(media && media.matches));

                if (media) {
                    const sendUpdate = (event) => dioxus.send(Boolean(event.matches));
                    if (typeof media.addEventListener === "function") {
                        media.addEventListener("change", sendUpdate);
                    } else if (typeof media.addListener === "function") {
                        media.addListener(sendUpdate);
                    }
                }

                await new Promise(() => {});
                "#,
            );

            loop {
                match eval.recv::<bool>().await {
                    Ok(is_dark) => {
                        let previous = *SYSTEM_THEME_IS_DARK.read();
                        if previous != is_dark {
                            log::info!("Native system theme changed: dark={}", is_dark);
                        }
                        *SYSTEM_THEME_IS_DARK.write() = is_dark;
                        apply_theme();
                    }
                    Err(e) => {
                        log::warn!("Native theme listener stopped: {:?}", e);
                        break;
                    }
                }
            }
        });
    });
}
