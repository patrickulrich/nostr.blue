use crate::stores::auth_store;
use crate::stores::emoji_store::{
    fetch_discoverable_emoji_packs, is_pack_installed, should_refresh_discoverable_emoji_packs,
    toggle_emoji_pack, DiscoverableEmojiPacksStoreStoreExt, EmojiSetsStoreStoreExt,
    DISCOVERABLE_EMOJI_PACKS, DISCOVERABLE_EMOJI_PACKS_LOADING, EMOJI_SETS,
};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct EmojiPackManagerModalProps {
    pub show: ReadSignal<bool>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn EmojiPackManagerModal(props: EmojiPackManagerModalProps) -> Element {
    let mut search_query = use_signal(String::new);
    let mut pending_coordinate = use_signal(|| None::<String>);
    let mut pending_any_toggle = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    let installed_sets = EMOJI_SETS.read();
    let discoverable_packs = DISCOVERABLE_EMOJI_PACKS.read();
    let discoverable_loading = *DISCOVERABLE_EMOJI_PACKS_LOADING.read();
    let is_authenticated = auth_store::is_authenticated();

    use_effect(move || {
        if !(props.show)() || !should_refresh_discoverable_emoji_packs() {
            return;
        }

        spawn(async move {
            if let Err(e) = fetch_discoverable_emoji_packs(80).await {
                log::error!("Failed to fetch discoverable emoji packs: {}", e);
                error_message.set(Some(e));
            }
        });
    });

    let query = search_query.read().trim().to_lowercase();
    let installed_filtered: Vec<_> = installed_sets
        .data()
        .read()
        .iter()
        .filter(|set| {
            if query.is_empty() {
                return true;
            }

            let name = set
                .name
                .clone()
                .unwrap_or_else(|| set.identifier.clone())
                .to_lowercase();
            let about = set.about.clone().unwrap_or_default().to_lowercase();
            name.contains(&query)
                || about.contains(&query)
                || set.author.to_lowercase().contains(&query)
                || set
                    .emojis
                    .iter()
                    .any(|emoji| emoji.shortcode.to_lowercase().contains(&query))
        })
        .cloned()
        .collect();

    let discoverable_filtered: Vec<_> = discoverable_packs
        .data()
        .read()
        .iter()
        .filter(|pack| {
            if is_pack_installed(&pack.coordinate) {
                return false;
            }
            if query.is_empty() {
                return true;
            }

            pack.name.to_lowercase().contains(&query)
                || pack
                    .about
                    .clone()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
                || pack.author.to_lowercase().contains(&query)
                || pack
                    .emojis
                    .iter()
                    .any(|emoji| emoji.shortcode.to_lowercase().contains(&query))
        })
        .cloned()
        .collect();

    rsx! {
        if (props.show)() {
            div {
                class: "fixed inset-0 z-[80] bg-black/50 backdrop-blur-sm",
                onclick: move |_| props.on_close.call(()),
            }
            div { class: "fixed inset-0 z-[81] flex items-center justify-center p-4 pointer-events-none",
                div {
                    class: "pointer-events-auto w-full max-w-4xl max-h-[calc(100vh-2rem)] overflow-hidden rounded-2xl border border-border bg-background shadow-2xl flex flex-col",
                    onclick: move |e| e.stop_propagation(),
                    div { class: "flex items-center justify-between gap-4 border-b border-border px-5 py-4",
                        div {
                            h2 { class: "text-lg font-semibold", "Manage Emoji Packs" }
                            p { class: "text-sm text-muted-foreground",
                                "Browse installed packs and discover new emoji collections."
                            }
                        }
                        button {
                            class: "rounded-full p-2 text-muted-foreground hover:bg-accent hover:text-foreground transition",
                            onclick: move |_| props.on_close.call(()),
                            "✕"
                        }
                    }
                    div { class: "border-b border-border px-5 py-4",
                        input {
                            r#type: "text",
                            class: "w-full rounded-xl border border-border bg-muted px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "Search packs or emoji shortcodes...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                    }
                    div { class: "flex-1 overflow-y-auto px-5 py-4 space-y-6",
                        if let Some(error) = error_message.read().clone() {
                            div { class: "rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-500",
                                "{error}"
                            }
                        }

                        div { class: "space-y-3",
                            div { class: "flex items-center justify-between",
                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground", "Installed" }
                                span { class: "text-xs text-muted-foreground", "{installed_filtered.len()} packs" }
                            }
                            if installed_filtered.is_empty() {
                                div { class: "rounded-xl border border-dashed border-border px-4 py-6 text-sm text-muted-foreground text-center",
                                    "No installed emoji packs yet."
                                }
                            } else {
                                div { class: "grid gap-3 md:grid-cols-2",
                                    for set in installed_filtered {
                                        {
                                            let coordinate = format!("30030:{}:{}", set.author, set.identifier);
                                            let is_pending = pending_coordinate.read().as_ref() == Some(&coordinate);
                                            let any_pending = *pending_any_toggle.read();
                                            let display_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                            let emoji_count = set.emojis.len();
                                            let preview = set.emojis.iter().take(18).cloned().collect::<Vec<_>>();
                                            let picture = set.picture.clone();
                                            let about = set.about.clone();
                                            rsx! {
                                                div {
                                                    key: "{coordinate}",
                                                    class: "rounded-2xl border border-border bg-card p-4 space-y-3",
                                                    div { class: "flex items-start gap-3",
                                                        if let Some(picture_url) = picture {
                                                            img {
                                                                src: "{picture_url}",
                                                                alt: "{display_name}",
                                                                class: "h-12 w-12 rounded-xl object-cover border border-border",
                                                                loading: "lazy",
                                                            }
                                                        }
                                                        div { class: "min-w-0 flex-1",
                                                            div { class: "flex items-center gap-2",
                                                                h4 { class: "truncate font-medium", "{display_name}" }
                                                                span { class: "rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground", "{emoji_count} emoji" }
                                                            }
                                                            if let Some(about_text) = about {
                                                                p { class: "mt-1 text-sm text-muted-foreground line-clamp-2", "{about_text}" }
                                                            }
                                                        }
                                                    }
                                                    div { class: "flex flex-wrap gap-1.5 rounded-xl bg-muted/60 p-3",
                                                        for emoji in preview {
                                                            img {
                                                                key: "{emoji.shortcode}",
                                                                src: "{emoji.image_url}",
                                                                alt: ":{emoji.shortcode}:",
                                                                class: "h-8 w-8 rounded object-contain",
                                                                loading: "lazy",
                                                            }
                                                        }
                                                    }
                                                    if is_authenticated {
                                                        button {
                                                            class: "w-full rounded-xl border border-border px-4 py-2 text-sm font-medium hover:bg-accent transition disabled:opacity-50",
                                                            disabled: any_pending,
                                                            onclick: {
                                                                let coordinate = coordinate.clone();
                                                                move |_| {
                                                                    let pack_coordinate = coordinate.clone();
                                                                    pending_any_toggle.set(true);
                                                                    pending_coordinate.set(Some(pack_coordinate.clone()));
                                                                    error_message.set(None);
                                                                    spawn(async move {
                                                                        if let Err(e) = toggle_emoji_pack(pack_coordinate.clone()).await {
                                                                            log::error!("Failed to remove emoji pack {}: {}", pack_coordinate, e);
                                                                            error_message.set(Some(e));
                                                                        }
                                                                        pending_coordinate.set(None);
                                                                        pending_any_toggle.set(false);
                                                                    });
                                                                }
                                                            },
                                                            if is_pending { "Updating..." } else { "Remove Pack" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "space-y-3",
                            div { class: "flex items-center justify-between",
                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground", "Discover" }
                                if discoverable_loading {
                                    span { class: "text-xs text-muted-foreground", "Loading..." }
                                } else {
                                    span { class: "text-xs text-muted-foreground", "{discoverable_filtered.len()} packs" }
                                }
                            }
                            if discoverable_loading && discoverable_filtered.is_empty() {
                                div { class: "rounded-xl border border-dashed border-border px-4 py-6 text-sm text-muted-foreground text-center",
                                    "Loading emoji packs from relays..."
                                }
                            } else if discoverable_filtered.is_empty() {
                                div { class: "rounded-xl border border-dashed border-border px-4 py-6 text-sm text-muted-foreground text-center",
                                    "No discoverable packs match your search."
                                }
                            } else {
                                div { class: "grid gap-3 md:grid-cols-2",
                                    for pack in discoverable_filtered {
                                        {
                                            let coordinate = pack.coordinate.clone();
                                            let is_pending = pending_coordinate.read().as_ref() == Some(&coordinate);
                                            let any_pending = *pending_any_toggle.read();
                                            let emoji_count = pack.emojis.len();
                                            let preview = pack.emojis.iter().take(18).cloned().collect::<Vec<_>>();
                                            let picture = pack.picture.clone();
                                            let about = pack.about.clone();
                                            let name = pack.name.clone();
                                            rsx! {
                                                div {
                                                    key: "{coordinate}",
                                                    class: "rounded-2xl border border-border bg-card p-4 space-y-3",
                                                    div { class: "flex items-start gap-3",
                                                        if let Some(picture_url) = picture {
                                                            img {
                                                                src: "{picture_url}",
                                                                alt: "{name}",
                                                                class: "h-12 w-12 rounded-xl object-cover border border-border",
                                                                loading: "lazy",
                                                            }
                                                        }
                                                        div { class: "min-w-0 flex-1",
                                                            div { class: "flex items-center gap-2",
                                                                h4 { class: "truncate font-medium", "{name}" }
                                                                span { class: "rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground", "{emoji_count} emoji" }
                                                            }
                                                            p { class: "mt-1 text-xs text-muted-foreground break-all", "{pack.author}" }
                                                            if let Some(about_text) = about {
                                                                p { class: "mt-1 text-sm text-muted-foreground line-clamp-2", "{about_text}" }
                                                            }
                                                        }
                                                    }
                                                    div { class: "flex flex-wrap gap-1.5 rounded-xl bg-muted/60 p-3",
                                                        for emoji in preview {
                                                            img {
                                                                key: "{emoji.shortcode}",
                                                                src: "{emoji.image_url}",
                                                                alt: ":{emoji.shortcode}:",
                                                                class: "h-8 w-8 rounded object-contain",
                                                                loading: "lazy",
                                                            }
                                                        }
                                                    }
                                                    if is_authenticated {
                                                        button {
                                                            class: "w-full rounded-xl bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                                                            disabled: any_pending,
                                                            onclick: {
                                                                let coordinate = coordinate.clone();
                                                                move |_| {
                                                                    let pack_coordinate = coordinate.clone();
                                                                    pending_any_toggle.set(true);
                                                                    pending_coordinate.set(Some(pack_coordinate.clone()));
                                                                    error_message.set(None);
                                                                    spawn(async move {
                                                                        if let Err(e) = toggle_emoji_pack(pack_coordinate.clone()).await {
                                                                            log::error!("Failed to add emoji pack {}: {}", pack_coordinate, e);
                                                                            error_message.set(Some(e));
                                                                        }
                                                                        pending_coordinate.set(None);
                                                                        pending_any_toggle.set(false);
                                                                    });
                                                                }
                                                            },
                                                            if is_pending { "Updating..." } else { "Add Pack" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
