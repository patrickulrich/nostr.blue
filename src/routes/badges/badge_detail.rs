//! Badge Detail Page
//!
//! Full page view of a badge definition with accept/reject actions.
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::nip58::{self, BadgeAward, BadgeDefinition};
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
/// Badge detail page component
#[component]
pub fn BadgeDetail(naddr: String) -> Element {
    let mut badge = use_signal(|| None::<BadgeDefinition>);
    let mut pending_award = use_signal(|| None::<BadgeAward>);
    let mut is_accepted = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut processing = use_signal(|| false);
    let navigator = use_navigator();
    let is_authenticated = auth_store::is_authenticated();
    let user_pubkey = auth_store::get_pubkey();
    use_effect(use_reactive(&naddr, move |addr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let pubkey = user_pubkey.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);
            match nip58::fetch_badge_definition(&addr).await {
                Ok(b) => {
                    badge.set(Some(b.clone()));
                    if let Some(pk) = &pubkey {
                        if let Ok(entries) = nip58::fetch_profile_badges(pk).await {
                            is_accepted.set(
                                entries
                                    .iter()
                                    .any(|e| e.definition_coordinate == b.coordinate),
                            );
                        }
                        if let Ok(awards) = nip58::fetch_pending_badge_awards(pk).await {
                            pending_award.set(
                                awards
                                    .into_iter()
                                    .find(|a| a.definition_coordinate == b.coordinate),
                            );
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch badge: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));
    let mut issuer_profile = use_signal(|| None::<Metadata>);
    use_effect(move || {
        if let Some(b) = badge.read().clone() {
            let pk = b.pubkey.clone();
            if let Some(profile) = profiles::get_profile(&pk) {
                issuer_profile.set(Some(profile));
            }
        }
    });
    let issuer_name = issuer_profile
        .read()
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()));
    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            ClientInitializing {}
        };
    }
    let badge_data = badge.read().clone();
    let (issuer_display_name, created_timestamp) = if let Some(ref b) = badge_data {
        let name = issuer_name
            .clone()
            .unwrap_or_else(|| truncate_pubkey(&b.pubkey));
        let ts = Timestamp::from(b.created_at);
        (name, ts)
    } else {
        (String::new(), Timestamp::from(0u64))
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center gap-4 px-4 py-3",
                    button {
                        class: "p-2 rounded-full hover:bg-accent transition",
                        onclick: move |_| {
                            navigator.go_back();
                        },
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold", "Badge" }
                }
            }
            if *loading.read() {
                div { class: "p-4 space-y-4",
                    div { class: "flex justify-center",
                        div { class: "w-48 h-48 rounded-xl bg-muted animate-pulse" }
                    }
                    div { class: "h-8 bg-muted rounded w-1/2 mx-auto animate-pulse" }
                    div { class: "h-4 bg-muted rounded w-3/4 mx-auto animate-pulse" }
                }
            } else if let Some(e) = error.read().as_ref() {
                div { class: "flex flex-col items-center justify-center py-16 text-center",
                    div { class: "text-6xl mb-4", "⚠️" }
                    p { class: "text-muted-foreground", "Failed to load badge" }
                    p { class: "text-sm text-destructive mt-2", "{e}" }
                }
            } else if let Some(b) = badge_data.clone() {
                div { class: "p-4 space-y-6",
                    div { class: "flex justify-center",
                        div { class: "relative",
                            if let Some(image) = b.get_image() {
                                img {
                                    src: "{image}",
                                    alt: "{b.get_display_name()}",
                                    class: "w-48 h-48 rounded-xl object-contain bg-muted",
                                }
                            } else {
                                div { class: "w-48 h-48 rounded-xl bg-primary/20 flex items-center justify-center",
                                    span { class: "text-6xl font-bold text-primary",
                                        "{b.id.chars().next().unwrap_or('?').to_uppercase()}"
                                    }
                                }
                            }
                            if *is_accepted.read() {
                                div { class: "absolute -top-2 -right-2 w-8 h-8 bg-green-500 rounded-full flex items-center justify-center",
                                    svg {
                                        class: "w-5 h-5 text-white",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M5 13l4 4L19 7",
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "text-center space-y-2",
                        h2 { class: "text-2xl font-bold", "{b.get_display_name()}" }
                        if let Some(desc) = &b.description {
                            p { class: "text-muted-foreground", "{desc}" }
                        }
                    }
                    div { class: "bg-card border border-border rounded-xl p-4",
                        div { class: "flex items-center justify-between",
                            span { class: "text-muted-foreground", "Created by" }
                            Link {
                                to: Route::Profile {
                                    pubkey: b.pubkey.clone(),
                                },
                                class: "flex items-center gap-2 text-primary hover:underline",
                                if let Some(profile) = issuer_profile.read().as_ref() {
                                    if let Some(picture) = &profile.picture {
                                        img {
                                            src: "{picture}",
                                            alt: "Profile",
                                            class: "w-6 h-6 rounded-full",
                                        }
                                    }
                                }
                                span { class: "font-medium", "@{issuer_display_name}" }
                            }
                        }
                        div { class: "flex items-center justify-between mt-3 pt-3 border-t border-border",
                            span { class: "text-muted-foreground", "Created" }
                            span { "{format_relative_time(created_timestamp)}" }
                        }
                        div { class: "flex items-center justify-between mt-3 pt-3 border-t border-border",
                            span { class: "text-muted-foreground", "Badge ID" }
                            span { class: "font-mono text-sm", "{b.id}" }
                        }
                    }
                    if is_authenticated {
                        div { class: "space-y-3",
                            if *is_accepted.read() {
                                div { class: "bg-green-500/10 border border-green-500/20 rounded-xl p-4 text-center",
                                    p { class: "text-green-600 dark:text-green-400 font-medium",
                                        "✓ You have accepted this badge"
                                    }
                                }
                                button {
                                    class: "w-full px-4 py-3 rounded-xl border border-destructive text-destructive hover:bg-destructive/10 transition disabled:opacity-50",
                                    disabled: *processing.read(),
                                    onclick: {
                                        let coord = b.coordinate.clone();
                                        move |_| {
                                            processing.set(true);
                                            let c = coord.clone();
                                            spawn(async move {
                                                match nip58::reject_badge(&c).await {
                                                    Ok(_) => {
                                                        log::info!("Badge removed from profile");
                                                        is_accepted.set(false);
                                                    }
                                                    Err(e) => {
                                                        log::error!("Failed to remove badge: {}", e);
                                                    }
                                                }
                                                processing.set(false);
                                            });
                                        }
                                    },
                                    if *processing.read() {
                                        "Removing..."
                                    } else {
                                        "Remove from Profile"
                                    }
                                }
                            } else if let Some(award) = pending_award.read().clone() {
                                div { class: "bg-primary/10 border border-primary/20 rounded-xl p-4 text-center",
                                    p { class: "text-primary font-medium",
                                        "🎉 You have been awarded this badge!"
                                    }
                                    p { class: "text-sm text-muted-foreground mt-1",
                                        "Accept it to display on your profile"
                                    }
                                }
                                div { class: "flex gap-3",
                                    button {
                                        class: "flex-1 px-4 py-3 rounded-xl border border-border hover:bg-accent transition",
                                        onclick: move |_| {
                                            pending_award.set(None);
                                        },
                                        "Decline"
                                    }
                                    button {
                                        class: "flex-1 px-4 py-3 rounded-xl bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                                        disabled: *processing.read(),
                                        onclick: {
                                            let coord = b.coordinate.clone();
                                            let event_id = award.event_id.clone();
                                            move |_| {
                                                processing.set(true);
                                                let c = coord.clone();
                                                let e = event_id.clone();
                                                spawn(async move {
                                                    match nip58::accept_badge(&c, &e).await {
                                                        Ok(_) => {
                                                            log::info!("Badge accepted!");
                                                            is_accepted.set(true);
                                                            pending_award.set(None);
                                                        }
                                                        Err(err) => {
                                                            log::error!("Failed to accept badge: {}", err);
                                                        }
                                                    }
                                                    processing.set(false);
                                                });
                                            }
                                        },
                                        if *processing.read() {
                                            "Accepting..."
                                        } else {
                                            "Accept Badge"
                                        }
                                    }
                                }
                            } else {
                                div { class: "bg-muted rounded-xl p-4 text-center",
                                    p { class: "text-muted-foreground",
                                        "You haven't been awarded this badge"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "pt-4 border-t border-border",
                        button {
                            class: "w-full flex items-center justify-center gap-2 px-4 py-3 rounded-xl border border-border hover:bg-accent transition",
                            onclick: {
                                let naddr = b.naddr.clone();
                                move |_| {
                                    let uri = format!("nostr:{}", naddr);
                                    spawn(async move {
                                        if let Err(e) = crate::utils::clipboard::copy_to_clipboard(&uri).await {
                                            log::error!("Failed to copy to clipboard: {:?}", e);
                                        }
                                    });
                                }
                            },
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z",
                                }
                            }
                            "Share Badge"
                        }
                    }
                }
            }
        }
    }
}
