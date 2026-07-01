//! Emoji Pack Detail Page
//!
//! Full view of a single NIP-51 emoji set (kind 30030). Reached by navigating
//! to an `naddr` that points at a kind 30030 event. Shows the pack title/about,
//! author, the full emoji grid, and an install/uninstall toggle that writes the
//! pack's coordinate into the viewer's kind 10030 emoji list.

use std::time::Duration;

use dioxus::prelude::*;

use crate::components::ClientInitializing;
use crate::platform::timer::sleep;
use crate::routes::Route;
use crate::stores::emoji_store::{
    fetch_emoji_set_by_naddr, is_pack_installed, toggle_emoji_pack, EmojiSet,
};
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;

/// Full-page viewer for a single NIP-51 emoji set (kind 30030).
#[component]
pub fn EmojiPackViewer(naddr: String) -> Element {
    let mut pack = use_signal(|| None::<EmojiSet>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut toggling = use_signal(|| false);
    let mut toggle_error = use_signal(|| None::<String>);
    let mut request_id = use_signal(|| 0u32);

    let navigator = use_navigator();
    let is_authenticated = auth_store::is_authenticated();

    let client_init = *nostr_client::CLIENT_INITIALIZED.read();
    use_effect(use_reactive(
        (&naddr, &client_init),
        move |(addr, client_initialized)| {
            if !client_initialized {
                return;
            }
            let current_id = request_id.peek().wrapping_add(1);
            request_id.set(current_id);
            loading.set(true);
            error.set(None);
            pack.set(None);
            spawn(async move {
                let fetched = fetch_emoji_set_by_naddr(&addr).await;
                if *request_id.peek() != current_id {
                    return;
                }
                match fetched {
                    Ok(Some(set)) => {
                        pack.set(Some(set));
                        loading.set(false);
                    }
                    Ok(None) => {
                        // Retry once after a delay — the first relay may have
                        // returned empty before the event propagated.
                        sleep(Duration::from_secs(8)).await;
                        if *request_id.peek() != current_id {
                            return;
                        }
                        match fetch_emoji_set_by_naddr(&addr).await {
                            Ok(Some(set)) => {
                                pack.set(Some(set));
                                loading.set(false);
                            }
                            Ok(None) => {
                                error.set(Some("Emoji pack not found".to_string()));
                                loading.set(false);
                            }
                            Err(e) => {
                                log::error!("Failed to fetch emoji pack: {}", e);
                                error.set(Some(e));
                                loading.set(false);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch emoji pack: {}", e);
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            });
        },
    ));

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    let loading_val = *loading.read();
    let error_val = error.read().clone();
    let pack_data = pack.read().clone();

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center gap-4 px-4 py-3",
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        onclick: move |_| navigator.go_back(),
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
                    h1 { class: "text-xl font-bold", "Emoji Pack" }
                }
            }

            if loading_val {
                div { class: "p-4 space-y-4",
                    div { class: "h-40 bg-muted rounded-xl animate-pulse" }
                    div { class: "h-8 w-1/2 bg-muted rounded animate-pulse" }
                    div { class: "grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 gap-3",
                        for _ in 0..16 {
                            div { class: "aspect-square bg-muted rounded-lg animate-pulse" }
                        }
                    }
                }
            } else if let Some(err) = error_val {
                div { class: "p-4",
                    div { class: "rounded-lg border border-border p-6 text-center",
                        p { class: "text-muted-foreground", "{err}" }
                    }
                }
            } else if let Some(p) = pack_data.as_ref() {
                div { class: "p-4 space-y-4 max-w-3xl mx-auto",
                    // Title
                    h2 { class: "text-2xl font-bold",
                        {p.name.clone().unwrap_or_else(|| p.identifier.clone())}
                    }

                    if let Some(about) = &p.about {
                        if !about.is_empty() {
                            p { class: "text-muted-foreground", "{about}" }
                        }
                    }

                    // Author + metadata
                    {
                        let author_profile = profiles::get_profile(&p.author);
                        let author_name = author_profile
                            .as_ref()
                            .and_then(|pr| pr.display_name.clone().or(pr.name.clone()))
                            .unwrap_or_else(|| truncate_pubkey(&p.author));
                        let author_picture = author_profile.as_ref().and_then(|pr| pr.picture.clone());

                        rsx! {
                            div { class: "flex items-center gap-3",
                                Link {
                                    to: Route::Profile { pubkey: profile_route_id(&p.author) },
                                    class: "flex items-center gap-2 hover:underline",
                                    if let Some(ref pic) = author_picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                        img {
                                            src: "{pic}",
                                            alt: "{author_name}",
                                            class: "w-8 h-8 rounded-full",
                                        }
                                    } else {
                                        div { class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center",
                                            span { class: "text-sm font-bold text-primary",
                                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                                            }
                                        }
                                    }
                                    span { class: "font-medium", "{author_name}" }
                                }
                                span { class: "text-muted-foreground", "·" }
                                span { class: "text-sm text-muted-foreground", "{p.emojis.len()} emoji" }
                            }
                        }
                    }

                    // Install / uninstall toggle
                    {
                        let coordinate = format!("30030:{}:{}", p.author, p.identifier);
                        let installed = is_pack_installed(&coordinate);
                        let toggle_err = toggle_error.read().clone();
                        let toggling_val = *toggling.read();
                        let button_label = if toggling_val {
                            "Updating..."
                        } else if installed {
                            "Remove from My Emojis"
                        } else {
                            "Add to My Emojis"
                        };
                        let button_class = if installed {
                            "px-4 py-2 rounded-lg text-sm font-medium transition disabled:opacity-50 hover:bg-accent border border-border"
                        } else {
                            "px-4 py-2 rounded-lg text-sm font-medium transition disabled:opacity-50 bg-primary text-primary-foreground hover:bg-primary/90"
                        };
                        is_authenticated.then(move || rsx! {
                            if let Some(err) = &toggle_err {
                                p { class: "text-sm text-red-500", "{err}" }
                            }
                            button {
                                class: "{button_class}",
                                disabled: toggling_val,
                                onclick: move |_| {
                                    if *toggling.peek() {
                                        return;
                                    }
                                    let coord = coordinate.clone();
                                    toggling.set(true);
                                    toggle_error.set(None);
                                    spawn(async move {
                                        if let Err(e) = toggle_emoji_pack(coord).await {
                                            log::error!("Failed to toggle emoji pack: {}", e);
                                            toggle_error.set(Some(e));
                                        }
                                        toggling.set(false);
                                    });
                                },
                                "{button_label}"
                            }
                        })
                    }

                    // Emoji grid
                    div { class: "grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 gap-3 pt-2",
                        for (idx, emoji) in p.emojis.iter().enumerate() {
                            div {
                                key: "{idx}-{emoji.shortcode}",
                                class: "flex flex-col items-center gap-1",
                                div { class: "h-12 w-12 rounded-lg bg-white flex items-center justify-center p-1.5",
                                    img {
                                        src: "{emoji.image_url}",
                                        alt: ":{emoji.shortcode}:",
                                        class: "max-h-full max-w-full object-contain",
                                        loading: "lazy",
                                    }
                                }
                                span { class: "text-[11px] text-muted-foreground truncate max-w-full",
                                    "{emoji.shortcode}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
