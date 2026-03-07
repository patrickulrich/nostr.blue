//! Badges Home Page
//!
//! Discovery/search page for NIP-58 badges.
//! Shows user's badges, pending awards, and all badges.
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip58::{self, BadgeAward, BadgeDefinition};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Tab selection for the Badges page
#[derive(Clone, Copy, PartialEq, Debug)]
enum BadgesTab {
    MyBadges,
    Pending,
    All,
    Created,
}
impl BadgesTab {
    fn label(&self) -> &'static str {
        match self {
            BadgesTab::MyBadges => "My Badges",
            BadgesTab::Pending => "Pending",
            BadgesTab::All => "All Badges",
            BadgesTab::Created => "Created",
        }
    }
}
/// Badges home page
#[component]
pub fn BadgesHome() -> Element {
    let mut active_tab = use_signal(|| BadgesTab::MyBadges);
    let mut search_query = use_signal(String::new);
    let mut my_badges = use_signal(Vec::<BadgeDefinition>::new);
    let mut pending_awards = use_signal(Vec::<(BadgeAward, Option<BadgeDefinition>)>::new);
    let mut all_badges = use_signal(Vec::<BadgeDefinition>::new);
    let mut created_badges = use_signal(Vec::<BadgeDefinition>::new);
    let mut loading = use_signal(|| true);
    let mut has_more = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let is_authenticated = auth_store::is_authenticated();
    let user_pubkey = auth_store::get_pubkey();
    use_effect(use_reactive(
        (
            &*active_tab.read(),
            &*nostr_client::CLIENT_INITIALIZED.read(),
        ),
        move |(tab, client_initialized)| {
            if !client_initialized {
                return;
            }
            loading.set(true);
            error.set(None);
            let pubkey = user_pubkey.clone();
            spawn(async move {
                match tab {
                    BadgesTab::MyBadges => {
                        if let Some(pk) = &pubkey {
                            match nip58::fetch_profile_badges(pk).await {
                                Ok(entries) => {
                                    let mut badges = Vec::new();
                                    for entry in entries {
                                        if let Ok(badge) = nip58::fetch_badge_by_coordinate(
                                            &entry.definition_coordinate,
                                        )
                                        .await
                                        {
                                            badges.push(badge);
                                        }
                                    }
                                    my_badges.set(badges);
                                }
                                Err(e) => {
                                    log::warn!("Failed to fetch my badges: {}", e);
                                    my_badges.set(Vec::new());
                                }
                            }
                        } else {
                            my_badges.set(Vec::new());
                        }
                    }
                    BadgesTab::Pending => {
                        if let Some(pk) = &pubkey {
                            match nip58::fetch_pending_badge_awards(pk).await {
                                Ok(awards) => {
                                    let accepted: Vec<String> = nip58::fetch_profile_badges(pk)
                                        .await
                                        .unwrap_or_default()
                                        .iter()
                                        .map(|e| e.definition_coordinate.clone())
                                        .collect();
                                    let mut pending = Vec::new();
                                    for award in awards {
                                        if !accepted.contains(&award.definition_coordinate) {
                                            let badge = nip58::fetch_badge_by_coordinate(
                                                &award.definition_coordinate,
                                            )
                                            .await
                                            .ok();
                                            pending.push((award, badge));
                                        }
                                    }
                                    pending_awards.set(pending);
                                }
                                Err(e) => {
                                    log::warn!("Failed to fetch pending awards: {}", e);
                                    pending_awards.set(Vec::new());
                                }
                            }
                        } else {
                            pending_awards.set(Vec::new());
                        }
                    }
                    BadgesTab::All => match nip58::fetch_recent_badges(50).await {
                        Ok(badges) => {
                            all_badges.set(badges);
                            has_more.set(true);
                        }
                        Err(e) => {
                            log::warn!("Failed to fetch all badges: {}", e);
                            all_badges.set(Vec::new());
                        }
                    },
                    BadgesTab::Created => {
                        if let Some(pk) = &pubkey {
                            match nip58::fetch_badges_by_issuer(pk).await {
                                Ok(badges) => {
                                    created_badges.set(badges);
                                }
                                Err(e) => {
                                    log::warn!("Failed to fetch created badges: {}", e);
                                    created_badges.set(Vec::new());
                                }
                            }
                        } else {
                            created_badges.set(Vec::new());
                        }
                    }
                }
                loading.set(false);
            });
        },
    ));
    let filter_badges = |badges: &[BadgeDefinition], query: &str| -> Vec<BadgeDefinition> {
        if query.is_empty() {
            return badges.to_vec();
        }
        let q = query.to_lowercase();
        badges
            .iter()
            .filter(|b| {
                b.name
                    .as_ref()
                    .map(|n| n.to_lowercase().contains(&q))
                    .unwrap_or(false)
                    || b.id.to_lowercase().contains(&q)
                    || b.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    };
    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            ClientInitializing {}
        };
    }
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    h1 { class: "text-xl font-bold", "Badges" }
                    if is_authenticated {
                        Link {
                            to: Route::BadgeNew {},
                            class: "flex items-center gap-2 px-3 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M12 4v16m8-8H4",
                                }
                            }
                            "Create"
                        }
                    }
                }
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-4 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Search badges...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
                div { class: "flex gap-2 px-4 pb-3 overflow-x-auto",
                    if is_authenticated {
                        TabButton {
                            label: BadgesTab::MyBadges.label(),
                            active: *active_tab.read() == BadgesTab::MyBadges,
                            onclick: move |_| active_tab.set(BadgesTab::MyBadges),
                        }
                        TabButton {
                            label: BadgesTab::Pending.label(),
                            active: *active_tab.read() == BadgesTab::Pending,
                            onclick: move |_| active_tab.set(BadgesTab::Pending),
                            badge_count: pending_awards.read().len(),
                        }
                    }
                    TabButton {
                        label: BadgesTab::All.label(),
                        active: *active_tab.read() == BadgesTab::All,
                        onclick: move |_| active_tab.set(BadgesTab::All),
                    }
                    if is_authenticated {
                        TabButton {
                            label: BadgesTab::Created.label(),
                            active: *active_tab.read() == BadgesTab::Created,
                            onclick: move |_| active_tab.set(BadgesTab::Created),
                        }
                    }
                }
            }
            div { class: "p-4",
                if *loading.read() {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4",
                        for _ in 0..8 {
                            BadgeCardSkeleton {}
                        }
                    }
                } else {
                    match *active_tab.read() {
                        BadgesTab::MyBadges => {
                            let filtered = filter_badges(&my_badges.read(), &search_query.read());
                            if filtered.is_empty() {
                                let msg = if search_query.read().is_empty() {
                                    "You haven't accepted any badges yet".to_string()
                                } else {
                                    "No badges match your search".to_string()
                                };
                                rsx! {
                                    EmptyState { message: msg }
                                }
                            } else {
                                rsx! {
                                    BadgeGrid { badges: filtered }
                                }
                            }
                        }
                        BadgesTab::Pending => {
                            let awards = pending_awards.read();
                            if awards.is_empty() {
                                rsx! {
                                    EmptyState { message: "No pending badge awards".to_string() }
                                }
                            } else {
                                rsx! {
                                    PendingAwardsList { awards: awards.clone() }
                                }
                            }
                        }
                        BadgesTab::All => {
                            let filtered = filter_badges(&all_badges.read(), &search_query.read());
                            if filtered.is_empty() {
                                let msg = if search_query.read().is_empty() {
                                    "No badges found".to_string()
                                } else {
                                    "No badges match your search".to_string()
                                };
                                rsx! {
                                    EmptyState { message: msg }
                                }
                            } else {
                                rsx! {
                                    BadgeGrid { badges: filtered }
                                }
                            }
                        }
                        BadgesTab::Created => {
                            let filtered = filter_badges(&created_badges.read(), &search_query.read());
                            if filtered.is_empty() {
                                let msg = if search_query.read().is_empty() {
                                    "You haven't created any badges".to_string()
                                } else {
                                    "No badges match your search".to_string()
                                };
                                rsx! {
                                    EmptyState { message: msg }
                                }
                            } else {
                                rsx! {
                                    BadgeGrid { badges: filtered }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Tab button component
#[component]
fn TabButton(
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
    badge_count: Option<usize>,
) -> Element {
    rsx! {
        button {
            class: if active { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium text-sm whitespace-nowrap flex items-center gap-2" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium text-sm whitespace-nowrap flex items-center gap-2" },
            onclick,
            "{label}"
            if let Some(count) = badge_count {
                if count > 0 {
                    span { class: "px-1.5 py-0.5 text-xs rounded-full bg-destructive text-destructive-foreground",
                        "{count}"
                    }
                }
            }
        }
    }
}
/// Badge grid component
#[component]
fn BadgeGrid(badges: Vec<BadgeDefinition>) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4",
            for badge in badges {
                BadgeCard { badge }
            }
        }
    }
}
/// Badge card component
#[component]
fn BadgeCard(badge: BadgeDefinition) -> Element {
    let naddr = badge.naddr.clone();
    rsx! {
        Link {
            to: Route::BadgeDetail { naddr },
            class: "group block bg-card border border-border rounded-xl overflow-hidden hover:border-primary/50 transition",
            div { class: "aspect-square bg-muted flex items-center justify-center p-4",
                if let Some(image) = badge.get_thumbnail() {
                    img {
                        src: "{image}",
                        alt: "{badge.get_display_name()}",
                        class: "w-full h-full object-contain group-hover:scale-105 transition",
                    }
                } else {
                    div { class: "w-16 h-16 rounded-full bg-primary/20 flex items-center justify-center",
                        span { class: "text-2xl font-bold text-primary",
                            "{badge.id.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
            }
            div { class: "p-3",
                h3 { class: "font-medium text-sm truncate", "{badge.get_display_name()}" }
                if let Some(desc) = &badge.description {
                    p { class: "text-xs text-muted-foreground truncate mt-1", "{desc}" }
                }
            }
        }
    }
}
/// Badge card skeleton for loading state
#[component]
fn BadgeCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-xl overflow-hidden animate-pulse",
            div { class: "aspect-square bg-muted" }
            div { class: "p-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-full" }
            }
        }
    }
}
/// Pending awards list component
#[component]
fn PendingAwardsList(awards: Vec<(BadgeAward, Option<BadgeDefinition>)>) -> Element {
    rsx! {
        div { class: "space-y-3",
            for (award , badge_opt) in awards {
                PendingAwardCard { award, badge: badge_opt }
            }
        }
    }
}
/// Pending award card component
#[component]
fn PendingAwardCard(award: BadgeAward, badge: Option<BadgeDefinition>) -> Element {
    let mut accepting = use_signal(|| false);
    let badge_name = badge
        .as_ref()
        .and_then(|b| b.name.clone())
        .unwrap_or_else(|| "Unknown Badge".to_string());
    let naddr = badge.as_ref().map(|b| b.naddr.clone());
    let award_clone = award.clone();
    let _badge_clone = badge.clone();
    let issuer_short = truncate_pubkey(&award.issuer);
    rsx! {
        div { class: "flex items-center gap-4 p-4 bg-card border border-border rounded-xl",
            div { class: "w-16 h-16 rounded-lg overflow-hidden bg-muted shrink-0",
                if let Some(thumb) = badge.as_ref().and_then(|b| b.get_thumbnail()) {
                    img {
                        src: "{thumb}",
                        alt: "{badge_name}",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center",
                        span { class: "text-xl font-bold text-primary",
                            "{badge_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                h3 { class: "font-medium truncate", "{badge_name}" }
                p { class: "text-sm text-muted-foreground truncate", "Awarded by {issuer_short}" }
            }
            div { class: "flex gap-2 shrink-0",
                if let Some(addr) = naddr {
                    Link {
                        to: Route::BadgeDetail { naddr: addr },
                        class: "px-3 py-1.5 text-sm rounded-lg border border-border hover:bg-accent transition",
                        "View"
                    }
                }
                button {
                    class: "px-3 py-1.5 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                    disabled: *accepting.read(),
                    onclick: move |_| {
                        accepting.set(true);
                        let coord = award_clone.definition_coordinate.clone();
                        let event_id = award_clone.event_id.clone();
                        spawn(async move {
                            match nip58::accept_badge(&coord, &event_id).await {
                                Ok(_) => {
                                    log::info!("Badge accepted!");
                                }
                                Err(e) => {
                                    log::error!("Failed to accept badge: {}", e);
                                }
                            }
                            accepting.set(false);
                        });
                    },
                    if *accepting.read() {
                        "Accepting..."
                    } else {
                        "Accept"
                    }
                }
            }
        }
    }
}
/// Empty state component
#[component]
fn EmptyState(message: String) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-16 text-center",
            div { class: "text-6xl mb-4", "🏅" }
            p { class: "text-muted-foreground", "{message}" }
        }
    }
}
