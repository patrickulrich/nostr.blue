use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use crate::platform::storage;
use serde::{Deserialize, Serialize};
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
                    let theme = *THEME.read();
                    match theme {
                        Theme::Light => {
                            root.set_attribute("class", "").ok();
                        }
                        Theme::Dark => {
                            root.set_attribute("class", "dark").ok();
                        }
                        Theme::System => {
                            let media_query = "(prefers-color-scheme: dark)";
                            if let Ok(Some(match_media)) = win.match_media(media_query) {
                                if match_media.matches() {
                                    root.set_attribute("class", "dark").ok();
                                } else {
                                    root.set_attribute("class", "").ok();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    #[cfg(feature = "native")]
    {
        let theme = *THEME.read();
        let class = match theme {
            Theme::Light => "",
            Theme::Dark => "dark",
            Theme::System => "",
        };
        let js = format!("document.documentElement.setAttribute('class', '{}')", class);
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
            }
            false
        }
    }
}
