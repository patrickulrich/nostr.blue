// Music Source Filter Tabs Component
// Allows filtering between All, Wavlake, Nostr, and Following sources

use dioxus::prelude::*;
use crate::stores::nostr_music::MusicFeedFilter;
use crate::stores::auth_store;

#[derive(Props, Clone, PartialEq)]
pub struct MusicSourceTabsProps {
    pub selected: MusicFeedFilter,
    pub on_change: EventHandler<MusicFeedFilter>,
}

/// Source filter tabs for music discovery
#[component]
pub fn MusicSourceTabs(props: MusicSourceTabsProps) -> Element {
    // Use reactive signal so tabs update on login/logout
    let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;

    let tabs = if is_authenticated {
        vec![
            (MusicFeedFilter::All, "All"),
            (MusicFeedFilter::Wavlake, "Wavlake"),
            (MusicFeedFilter::Nostr, "Nostr"),
            (MusicFeedFilter::Following, "Following"),
        ]
    } else {
        vec![
            (MusicFeedFilter::All, "All"),
            (MusicFeedFilter::Wavlake, "Wavlake"),
            (MusicFeedFilter::Nostr, "Nostr"),
        ]
    };

    rsx! {
        div {
            class: "flex items-center gap-2 overflow-x-auto pb-2 scrollbar-hide",

            for (filter, label) in tabs {
                button {
                    key: "{label}",
                    class: if props.selected == filter {
                        "px-4 py-2 rounded-full text-sm font-medium bg-primary text-primary-foreground transition whitespace-nowrap"
                    } else {
                        "px-4 py-2 rounded-full text-sm font-medium bg-muted hover:bg-muted/80 text-muted-foreground transition whitespace-nowrap"
                    },
                    onclick: {
                        let filter = filter.clone();
                        move |_| props.on_change.call(filter.clone())
                    },
                    "{label}"
                }
            }
        }
    }
}

/// Discovery section tabs (Trending, New, Playlists, Following)
#[derive(Clone, Debug, PartialEq, Default)]
pub enum DiscoveryTab {
    #[default]
    Trending,
    New,
    Playlists,
    Following,
}

#[derive(Props, Clone, PartialEq)]
pub struct DiscoveryTabsProps {
    pub selected: DiscoveryTab,
    pub on_change: EventHandler<DiscoveryTab>,
}

#[component]
pub fn DiscoveryTabs(props: DiscoveryTabsProps) -> Element {
    // Use reactive signal so tabs update on login/logout
    let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;

    let tabs = if is_authenticated {
        vec![
            (DiscoveryTab::Trending, "Trending"),
            (DiscoveryTab::New, "New"),
            (DiscoveryTab::Playlists, "Playlists"),
            (DiscoveryTab::Following, "Following"),
        ]
    } else {
        vec![
            (DiscoveryTab::Trending, "Trending"),
            (DiscoveryTab::New, "New"),
            (DiscoveryTab::Playlists, "Playlists"),
        ]
    };

    rsx! {
        div {
            class: "flex items-center gap-1 border-b border-border",

            for (tab, label) in tabs {
                button {
                    key: "{label}",
                    class: if props.selected == tab {
                        "px-4 py-3 text-sm font-medium text-foreground border-b-2 border-primary transition"
                    } else {
                        "px-4 py-3 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                    },
                    onclick: {
                        let tab = tab.clone();
                        move |_| props.on_change.call(tab.clone())
                    },
                    "{label}"
                }
            }
        }
    }
}
