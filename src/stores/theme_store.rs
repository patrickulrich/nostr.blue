use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
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
    // Try to load from localStorage
    if let Ok(theme_str) = LocalStorage::get::<String>(STORAGE_KEY) {
        let theme = Theme::from_str(&theme_str);
        *THEME.write() = theme;
        log::info!("Loaded theme from storage: {:?}", theme);
    } else {
        // Use system preference
        *THEME.write() = Theme::System;
        log::info!("Using system theme preference");
    }

    apply_theme();
}

/// Set theme UI state only (internal use, no Nostr sync)
pub fn set_theme_internal(theme: Theme) {
    // Check if theme is already set to avoid redundant updates
    if *THEME.read() == theme {
        return;
    }

    *THEME.write() = theme;
    LocalStorage::set(STORAGE_KEY, theme.as_str()).ok();
    log::info!("Theme changed to: {:?}", theme);
    apply_theme();
}

/// Set theme and persist to localStorage and Nostr (NIP-78)
pub fn set_theme(theme: Theme) {
    set_theme_internal(theme);

    // Sync to Nostr in background
    #[cfg(target_arch = "wasm32")]
    {
        use crate::stores::settings_store;
        dioxus::prelude::spawn(async move {
            settings_store::update_theme(theme).await;
        });
    }
}

/// Apply theme to document
pub fn apply_theme() {
    #[cfg(target_arch = "wasm32")]
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
                            // Check system preference
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
        Theme::System => Theme::Dark, // Default to dark when toggling from system
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
            // Check system preference
            #[cfg(target_arch = "wasm32")]
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
