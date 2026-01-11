use dioxus::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

/// Counter for generating unique modal IDs for accessibility
static MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

use crate::stores::{
    auth_store,
    nostr_client,
    blossom_store::{
        self, BlossomTab, MediaFilter, MediaItem,
        MEDIA_ITEMS, MEDIA_LOADING, MEDIA_ERROR,
        UPLOAD_PROGRESS,
    },
};

/// Blossom Media Management Page
/// Full-page layout for managing media files across Blossom servers
#[component]
pub fn BlossomPage() -> Element {
    let is_authenticated = auth_store::is_authenticated();

    // Redirect if not authenticated
    if !is_authenticated {
        return rsx! {
            div { class: "flex flex-col items-center justify-center min-h-[60vh] p-8",
                div { class: "w-16 h-16 mb-4 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-8 h-8 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path { d: "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" }
                    }
                }
                h2 { class: "text-xl font-semibold mb-2", "Sign in to access Blossom" }
                p { class: "text-muted-foreground text-center max-w-md",
                    "Blossom is your personal media library. Sign in to upload, manage, and mirror your files across decentralized storage servers."
                }
            }
        };
    }

    // State
    let mut active_tab = use_signal(|| BlossomTab::Images);
    let mut selected_items = use_signal(HashSet::<String>::new);
    let mut show_upload_modal = use_signal(|| false);
    let mut selected_file = use_signal(|| None::<MediaItem>);
    let mut deleting = use_signal(|| false);
    let mut mirroring = use_signal(|| false);

    // Load files when client is initialized
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        spawn(async move {
            if let Err(e) = blossom_store::list_files().await {
                log::error!("Failed to load files: {}", e);
            }
        });
    });

    // Load servers when client is initialized
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        spawn(async move {
            if let Err(e) = blossom_store::fetch_user_servers().await {
                log::warn!("Failed to load servers: {}", e);
            }
        });
    });

    // Get current items filtered by tab
    let media_items = MEDIA_ITEMS.read();
    let loading = *MEDIA_LOADING.read();
    let error = MEDIA_ERROR.read().clone();
    let servers = blossom_store::get_servers();

    let filtered_items: Vec<MediaItem> = match active_tab() {
        BlossomTab::Images => media_items.iter()
            .filter(|i| MediaFilter::Images.matches(&i.mime_type))
            .cloned()
            .collect(),
        BlossomTab::Videos => media_items.iter()
            .filter(|i| MediaFilter::Videos.matches(&i.mime_type))
            .cloned()
            .collect(),
        BlossomTab::Audio => media_items.iter()
            .filter(|i| MediaFilter::Audio.matches(&i.mime_type))
            .cloned()
            .collect(),
        BlossomTab::Files => media_items.iter()
            .filter(|i| MediaFilter::Files.matches(&i.mime_type))
            .cloned()
            .collect(),
        BlossomTab::Servers => vec![],
    };

    let selected_count = selected_items.read().len();
    let total_storage = blossom_store::get_total_storage();

    // Handlers
    let handle_delete_selected = move |_| {
        let items_to_delete: Vec<String> = selected_items.read().iter().cloned().collect();
        if items_to_delete.is_empty() {
            return;
        }

        deleting.set(true);
        spawn(async move {
            match blossom_store::delete_files(&items_to_delete).await {
                Ok(count) => {
                    log::info!("Deleted {} files", count);
                    selected_items.write().clear();
                }
                Err(e) => {
                    log::error!("Failed to delete files: {}", e);
                }
            }
            deleting.set(false);
        });
    };

    let handle_mirror_selected = move |_| {
        let items_to_mirror: Vec<MediaItem> = {
            let selected = selected_items.read();
            let current_items = MEDIA_ITEMS.read();
            current_items.iter()
                .filter(|i| selected.contains(&i.sha256))
                .cloned()
                .collect()
        };

        if items_to_mirror.is_empty() {
            return;
        }

        mirroring.set(true);
        spawn(async move {
            match blossom_store::mirror_files(&items_to_mirror).await {
                Ok(count) => {
                    log::info!("Mirrored {} files", count);
                    // Refresh the file list to show updated mirrors
                    let _ = blossom_store::list_files().await;
                }
                Err(e) => {
                    log::error!("Failed to mirror files: {}", e);
                }
            }
            mirroring.set(false);
        });
    };

    let handle_refresh = move |_| {
        spawn(async move {
            if let Err(e) = blossom_store::list_files().await {
                log::error!("Failed to refresh files: {}", e);
            }
        });
    };

    rsx! {
        div { class: "max-w-6xl mx-auto p-4 space-y-6",
            // Header
            div { class: "flex items-center justify-between",
                div {
                    h1 { class: "text-2xl font-bold", "Blossom" }
                    p { class: "text-muted-foreground text-sm",
                        "Media storage: {blossom_store::format_bytes(total_storage)} across {servers.len()} server(s)"
                    }
                }
                div { class: "flex items-center gap-2",
                    // Refresh button
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        onclick: handle_refresh,
                        disabled: loading,
                        svg {
                            class: if loading { "w-5 h-5 animate-spin" } else { "w-5 h-5" },
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" }
                        }
                    }
                    // Upload button
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg transition flex items-center gap-2",
                        onclick: move |_| show_upload_modal.set(true),
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" }
                        }
                        "Upload"
                    }
                }
            }

            // Tab Navigation
            div { class: "flex border-b border-border",
                for tab in [BlossomTab::Images, BlossomTab::Videos, BlossomTab::Audio, BlossomTab::Files, BlossomTab::Servers] {
                    {
                        let is_active = active_tab() == tab;
                        let count = match tab {
                            BlossomTab::Images => media_items.iter().filter(|i| MediaFilter::Images.matches(&i.mime_type)).count(),
                            BlossomTab::Videos => media_items.iter().filter(|i| MediaFilter::Videos.matches(&i.mime_type)).count(),
                            BlossomTab::Audio => media_items.iter().filter(|i| MediaFilter::Audio.matches(&i.mime_type)).count(),
                            BlossomTab::Files => media_items.iter().filter(|i| MediaFilter::Files.matches(&i.mime_type)).count(),
                            BlossomTab::Servers => servers.len(),
                        };
                        rsx! {
                            button {
                                key: "{tab:?}",
                                class: if is_active {
                                    "px-4 py-3 font-medium border-b-2 border-blue-500 text-blue-500"
                                } else {
                                    "px-4 py-3 font-medium text-muted-foreground hover:text-foreground transition"
                                },
                                onclick: move |_| {
                                    active_tab.set(tab);
                                    selected_items.write().clear();
                                },
                                "{tab.label()}"
                                if count > 0 {
                                    span { class: "ml-2 text-xs bg-muted px-2 py-0.5 rounded-full",
                                        "{count}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Error message
            if let Some(err) = error {
                div { class: "p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500",
                    "{err}"
                }
            }

            // Content based on active tab
            if active_tab() == BlossomTab::Servers {
                // Servers tab content
                ServerList {
                    servers: servers.clone(),
                    on_add_server: move |url: String| {
                        blossom_store::add_server(url);
                    },
                    on_remove_server: move |url: String| {
                        blossom_store::remove_server(&url);
                    },
                    on_set_preferred: move |url: String| {
                        blossom_store::set_as_preferred(&url);
                    },
                }
            } else if !*nostr_client::CLIENT_INITIALIZED.read() || (loading && media_items.is_empty()) {
                // Loading skeleton - shown during client initialization or initial data load
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                    for _ in 0..8 {
                        div { class: "aspect-square bg-muted animate-pulse rounded-lg" }
                    }
                }
            } else if filtered_items.is_empty() {
                // Empty state
                div { class: "flex flex-col items-center justify-center py-16",
                    div { class: "w-16 h-16 mb-4 rounded-full bg-muted flex items-center justify-center",
                        svg {
                            class: "w-8 h-8 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" }
                        }
                    }
                    p { class: "text-muted-foreground font-medium", "No {active_tab().label().to_lowercase()} found" }
                    p { class: "text-muted-foreground text-sm", "Upload some files to get started" }
                }
            } else {
                // Media grid
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                    for item in filtered_items.iter() {
                        {
                            let sha256 = item.sha256.clone();
                            let is_selected = selected_items.read().contains(&sha256);
                            let item_clone = item.clone();
                            rsx! {
                                MediaCard {
                                    key: "{sha256}",
                                    item: item.clone(),
                                    is_selected: is_selected,
                                    on_select: move |_| {
                                        let mut sel = selected_items.write();
                                        if sel.contains(&sha256) {
                                            sel.remove(&sha256);
                                        } else {
                                            sel.insert(sha256.clone());
                                        }
                                    },
                                    on_click: move |_| {
                                        selected_file.set(Some(item_clone.clone()));
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Batch Actions Bar (when items selected)
            if selected_count > 0 {
                div { class: "fixed bottom-4 left-1/2 -translate-x-1/2 bg-card border border-border rounded-lg shadow-lg p-4 flex items-center gap-4 z-50",
                    span { class: "text-sm font-medium",
                        "{selected_count} selected"
                    }
                    div { class: "h-6 w-px bg-border" }
                    button {
                        class: "px-3 py-1.5 text-sm bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition flex items-center gap-2",
                        onclick: handle_mirror_selected,
                        disabled: mirroring(),
                        if mirroring() {
                            svg { class: "w-4 h-4 animate-spin", xmlns: "http://www.w3.org/2000/svg", fill: "none", view_box: "0 0 24 24",
                                circle { class: "opacity-25", cx: "12", cy: "12", r: "10", stroke: "currentColor", stroke_width: "4" }
                                path { class: "opacity-75", fill: "currentColor", d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" }
                            }
                        }
                        "Mirror"
                    }
                    button {
                        class: "px-3 py-1.5 text-sm bg-red-500 hover:bg-red-600 text-white rounded-lg transition flex items-center gap-2",
                        onclick: handle_delete_selected,
                        disabled: deleting(),
                        if deleting() {
                            svg { class: "w-4 h-4 animate-spin", xmlns: "http://www.w3.org/2000/svg", fill: "none", view_box: "0 0 24 24",
                                circle { class: "opacity-25", cx: "12", cy: "12", r: "10", stroke: "currentColor", stroke_width: "4" }
                                path { class: "opacity-75", fill: "currentColor", d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" }
                            }
                        }
                        "Delete"
                    }
                    button {
                        class: "px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| selected_items.write().clear(),
                        "Cancel"
                    }
                }
            }
        }

        // File Detail Modal
        if let Some(file) = selected_file() {
            FileDetailModal {
                item: file.clone(),
                on_close: move |_| selected_file.set(None),
                on_delete: move |sha256: String| {
                    spawn(async move {
                        if let Err(e) = blossom_store::delete_file(&sha256).await {
                            log::error!("Failed to delete file: {}", e);
                        }
                        selected_file.set(None);
                    });
                },
                on_mirror: move |item: MediaItem| {
                    spawn(async move {
                        if let Err(e) = blossom_store::mirror_file(&item.sha256, &item.url, None).await {
                            log::error!("Failed to mirror file: {}", e);
                        }
                        // Refresh to show updated mirrors
                        let _ = blossom_store::list_files().await;
                    });
                },
            }
        }

        // Upload Modal
        if show_upload_modal() {
            UploadModal {
                on_close: move |_| show_upload_modal.set(false),
                on_upload_complete: move |_| {
                    show_upload_modal.set(false);
                    // Refresh file list
                    spawn(async move {
                        let _ = blossom_store::list_files().await;
                    });
                },
            }
        }
    }
}

/// Individual media card component
#[component]
fn MediaCard(
    item: MediaItem,
    is_selected: bool,
    on_select: EventHandler<()>,
    on_click: EventHandler<()>,
) -> Element {
    let is_image = item.mime_type.starts_with("image/");
    let is_video = item.mime_type.starts_with("video/");
    let mirror_count = item.mirrors.len();
    let is_fully_mirrored = blossom_store::is_fully_mirrored(&item);

    rsx! {
        div {
            class: "relative group cursor-pointer",
            onclick: move |_| on_click.call(()),

            // Thumbnail
            div {
                class: if is_selected {
                    "aspect-square rounded-lg overflow-hidden border-2 border-blue-500 bg-muted"
                } else {
                    "aspect-square rounded-lg overflow-hidden border border-border bg-muted hover:border-blue-500/50 transition"
                },

                if is_image {
                    img {
                        class: "w-full h-full object-cover",
                        src: "{item.url}",
                        alt: "Media thumbnail",
                        referrerpolicy: "no-referrer",
                        loading: "lazy",
                    }
                } else if is_video {
                    div { class: "w-full h-full flex items-center justify-center bg-gray-900",
                        svg {
                            class: "w-12 h-12 text-white/60",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" }
                            path { d: "M21 12a9 9 0 11-18 0 9 9 0 0118 0z" }
                        }
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center",
                        svg {
                            class: "w-12 h-12 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" }
                        }
                    }
                }
            }

            // Selection checkbox
            div {
                class: "absolute top-2 left-2",
                onclick: move |e| {
                    e.stop_propagation();
                    on_select.call(());
                },
                div {
                    class: if is_selected {
                        "w-6 h-6 rounded-md bg-blue-500 flex items-center justify-center"
                    } else {
                        "w-6 h-6 rounded-md border-2 border-white/80 bg-black/30 group-hover:bg-black/50 transition"
                    },
                    if is_selected {
                        svg {
                            class: "w-4 h-4 text-white",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "3",
                            path { d: "M5 13l4 4L19 7" }
                        }
                    }
                }
            }

            // Mirror count badge
            if mirror_count > 0 {
                div {
                    class: if is_fully_mirrored {
                        "absolute top-2 right-2 px-2 py-0.5 text-xs font-medium rounded-full bg-green-500 text-white"
                    } else {
                        "absolute top-2 right-2 px-2 py-0.5 text-xs font-medium rounded-full bg-yellow-500 text-white"
                    },
                    "{mirror_count + 1}"
                }
            }

            // File info overlay
            div { class: "absolute bottom-0 left-0 right-0 p-2 bg-gradient-to-t from-black/60 to-transparent opacity-0 group-hover:opacity-100 transition",
                p { class: "text-white text-xs truncate", "{blossom_store::format_bytes(item.size)}" }
            }
        }
    }
}

/// File detail modal component
#[component]
fn FileDetailModal(
    item: MediaItem,
    on_close: EventHandler<()>,
    on_delete: EventHandler<String>,
    on_mirror: EventHandler<MediaItem>,
) -> Element {
    let is_image = item.mime_type.starts_with("image/");
    let is_video = item.mime_type.starts_with("video/");
    let is_audio = item.mime_type.starts_with("audio/");
    #[allow(unused_mut)] // mut required for WASM target
    let mut copied = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);

    // Generate unique ID for ARIA accessibility
    let modal_id = use_signal(|| MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let title_id = format!("file-detail-title-{}", modal_id());

    #[allow(unused_variables)]
    let handle_copy = {
        let url_for_copy = item.url.clone();
        move |_| {
            #[cfg(target_arch = "wasm32")]
            {
                let url = url_for_copy.clone();
                spawn(async move {
                    if let Some(window) = web_sys::window() {
                        let clipboard = window.navigator().clipboard();
                        match wasm_bindgen_futures::JsFuture::from(
                            clipboard.write_text(&url)
                        ).await {
                            Ok(_) => {
                                copied.set(true);
                                gloo_timers::future::TimeoutFuture::new(2000).await;
                                copied.set(false);
                            }
                            Err(e) => {
                                log::warn!("Failed to copy to clipboard: {:?}", e);
                            }
                        }
                    }
                });
            }
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/80 z-50 flex items-center justify-center p-4",
            role: "dialog",
            aria_modal: "true",
            aria_labelledby: "{title_id}",
            tabindex: "-1",
            onclick: move |_| on_close.call(()),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },

            div {
                class: "bg-card rounded-xl max-w-4xl w-full max-h-[90vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h3 { id: "{title_id}", class: "font-semibold", "File Details" }
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M6 18L18 6M6 6l12 12" }
                        }
                    }
                }

                // Content
                div { class: "flex-1 overflow-y-auto p-4 space-y-4",
                    // Preview
                    div { class: "flex justify-center bg-muted rounded-lg overflow-hidden",
                        if is_image {
                            img {
                                class: "max-h-[400px] object-contain",
                                src: "{item.url}",
                                alt: "Media preview",
                                referrerpolicy: "no-referrer",
                            }
                        } else if is_video {
                            video {
                                class: "max-h-[400px]",
                                src: "{item.url}",
                                controls: true,
                            }
                        } else if is_audio {
                            div { class: "p-8",
                                audio {
                                    src: "{item.url}",
                                    controls: true,
                                    class: "w-full",
                                }
                            }
                        } else {
                            div { class: "p-16 flex items-center justify-center",
                                svg {
                                    class: "w-24 h-24 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path { d: "M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" }
                                }
                            }
                        }
                    }

                    // Metadata
                    div { class: "grid grid-cols-2 gap-4 text-sm",
                        div {
                            p { class: "text-muted-foreground", "Type" }
                            p { class: "font-medium", "{item.mime_type}" }
                        }
                        div {
                            p { class: "text-muted-foreground", "Size" }
                            p { class: "font-medium", "{blossom_store::format_bytes(item.size)}" }
                        }
                        div { class: "col-span-2",
                            p { class: "text-muted-foreground", "SHA-256" }
                            p { class: "font-mono text-xs break-all", "{item.sha256}" }
                        }
                    }

                    // Primary URL
                    div {
                        p { class: "text-muted-foreground text-sm mb-1", "Primary URL" }
                        div { class: "flex items-center gap-2",
                            input {
                                class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm font-mono",
                                readonly: true,
                                value: "{item.url}",
                            }
                            button {
                                class: "px-3 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition text-sm",
                                onclick: handle_copy,
                                if copied() { "Copied!" } else { "Copy" }
                            }
                        }
                    }

                    // Mirrors
                    if !item.mirrors.is_empty() {
                        div {
                            p { class: "text-muted-foreground text-sm mb-2", "Mirrors ({item.mirrors.len()})" }
                            div { class: "space-y-2",
                                for mirror in item.mirrors.iter() {
                                    div {
                                        key: "{mirror}",
                                        class: "px-3 py-2 bg-muted rounded-lg text-sm font-mono truncate",
                                        "{mirror}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Actions
                div { class: "p-4 border-t border-border flex items-center gap-2",
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition flex items-center gap-2",
                        onclick: {
                            let item_clone = item.clone();
                            move |_| on_mirror.call(item_clone.clone())
                        },
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" }
                        }
                        "Mirror to Other Servers"
                    }
                    div { class: "flex-1" }
                    if confirm_delete() {
                        span { class: "text-red-500 text-sm mr-2", "Confirm delete?" }
                        button {
                            class: "px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition",
                            onclick: {
                                let sha256 = item.sha256.clone();
                                move |_| on_delete.call(sha256.clone())
                            },
                            "Yes, Delete"
                        }
                        button {
                            class: "px-4 py-2 text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| confirm_delete.set(false),
                            "Cancel"
                        }
                    } else {
                        button {
                            class: "px-4 py-2 text-red-500 hover:bg-red-500/10 rounded-lg transition",
                            onclick: move |_| confirm_delete.set(true),
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}

/// Upload modal component
#[component]
fn UploadModal(
    on_close: EventHandler<()>,
    on_upload_complete: EventHandler<()>,
) -> Element {
    // Generate unique ID for ARIA accessibility
    let modal_id = use_signal(|| MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let title_id = format!("upload-modal-title-{}", modal_id());

    let mut file_data = use_signal(|| None::<(String, Vec<u8>, String)>);
    let mut quality = use_signal(|| 80u8);
    let mut uploading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let upload_progress = *UPLOAD_PROGRESS.read();

    let handle_file_select = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let mut error = error;
            spawn(async move {
                use wasm_bindgen::JsCast;

                // Defensive error handling - avoid unwrap() chains
                let Some(window) = web_sys::window() else {
                    error.set(Some("Window not available".to_string()));
                    return;
                };
                let Some(document) = window.document() else {
                    error.set(Some("Document not available".to_string()));
                    return;
                };
                let Some(body) = document.body() else {
                    error.set(Some("Document body not available".to_string()));
                    return;
                };
                let Ok(input_el) = document.create_element("input") else {
                    error.set(Some("Failed to create input element".to_string()));
                    return;
                };
                let Ok(input) = input_el.dyn_into::<web_sys::HtmlInputElement>() else {
                    error.set(Some("Failed to cast input element".to_string()));
                    return;
                };
                input.set_type("file");
                input.set_accept("image/*,video/*,audio/*");
                // Hide the input element
                input.set_attribute("style", "display: none").ok();

                // Append to body so it's in the DOM
                body.append_child(&input).ok();

                let (tx, rx) = futures::channel::oneshot::channel::<Option<web_sys::File>>();
                let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));

                // Capture input directly - no DOM query needed
                let input_for_closure = input.clone();
                let input_for_cleanup = input.clone();

                // Use Closure::once - automatically cleaned up after first call
                let closure = wasm_bindgen::closure::Closure::once(Box::new(move || {
                    if let Some(tx) = tx.borrow_mut().take() {
                        let file = input_for_closure.files().and_then(|f| f.get(0));
                        let _ = tx.send(file);
                    }
                }) as Box<dyn FnOnce()>);

                input.set_onchange(Some(closure.as_ref().unchecked_ref()));
                input.click();

                // Wait for file selection with timeout to prevent hanging
                use futures::future::{select, Either};
                use gloo_timers::future::TimeoutFuture;

                let timeout = TimeoutFuture::new(300_000); // 5 minute timeout

                let file = match select(std::pin::pin!(rx), std::pin::pin!(timeout)).await {
                    Either::Left((Ok(Some(file)), _)) => file,
                    Either::Left((Ok(None), _)) => {
                        // User cancelled - clean up and return
                        body.remove_child(&input_for_cleanup).ok();
                        return;
                    }
                    Either::Left((Err(_), _)) | Either::Right((_, _)) => {
                        // Channel error or timeout - clean up and return
                        body.remove_child(&input_for_cleanup).ok();
                        return;
                    }
                };

                let name = file.name();
                let mime_type = file.type_();

                let array_buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await;
                if let Ok(buffer) = array_buffer {
                    let uint8_array = js_sys::Uint8Array::new(&buffer);
                    let mut data = vec![0u8; uint8_array.length() as usize];
                    uint8_array.copy_to(&mut data);

                    file_data.set(Some((name, data, mime_type)));
                }

                // Clean up: remove from DOM (closure auto-drops with Closure::once)
                body.remove_child(&input_for_cleanup).ok();
            });
        }
    };

    let handle_upload = move |_| {
        // Take the data instead of cloning to avoid duplicating the Vec<u8>
        if let Some((_name, data, mime_type)) = file_data.write().take() {
            uploading.set(true);
            error.set(None);

            let q = quality();
            spawn(async move {
                let result = if mime_type.starts_with("image/") {
                    blossom_store::upload_image(data, mime_type, q, None).await
                } else if mime_type.starts_with("audio/") {
                    blossom_store::upload_audio(data, mime_type, None).await
                } else {
                    // For video and other files, use upload_image with 100% quality (no compression)
                    blossom_store::upload_image(data, mime_type, 100, None).await
                };

                match result {
                    Ok(url) => {
                        log::info!("Uploaded file: {}", url);
                        on_upload_complete.call(());
                    }
                    Err(e) => {
                        error.set(Some(e));
                        uploading.set(false);
                    }
                }
            });
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/80 z-50 flex items-center justify-center p-4",
            role: "dialog",
            aria_modal: "true",
            aria_labelledby: "{title_id}",
            tabindex: "-1",
            onclick: move |_| {
                if !uploading() {
                    on_close.call(());
                }
            },
            onkeydown: move |e| {
                if e.key() == Key::Escape && !uploading() {
                    on_close.call(());
                }
            },

            div {
                class: "bg-card rounded-xl max-w-lg w-full",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h3 { id: "{title_id}", class: "font-semibold", "Upload Media" }
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        disabled: uploading(),
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M6 18L18 6M6 6l12 12" }
                        }
                    }
                }

                // Content
                div { class: "p-4 space-y-4",
                    // File picker - use read().as_ref() to avoid cloning large Vec<u8>
                    if let Some((name, data, mime_type)) = file_data.read().as_ref() {
                        div { class: "p-4 bg-muted rounded-lg",
                            div { class: "flex items-center gap-3",
                                if mime_type.starts_with("image/") {
                                    svg {
                                        class: "w-8 h-8 text-blue-500",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" }
                                    }
                                } else {
                                    svg {
                                        class: "w-8 h-8 text-blue-500",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" }
                                    }
                                }
                                div { class: "flex-1 min-w-0",
                                    p { class: "font-medium truncate", "{name}" }
                                    p { class: "text-sm text-muted-foreground", "{blossom_store::format_bytes(data.len() as u64)}" }
                                }
                                button {
                                    class: "p-2 hover:bg-accent rounded-lg transition",
                                    disabled: uploading(),
                                    onclick: move |_| file_data.set(None),
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M6 18L18 6M6 6l12 12" }
                                    }
                                }
                            }
                        }

                        // Quality slider (for images only)
                        if mime_type.starts_with("image/") {
                            div {
                                label { class: "block text-sm font-medium mb-2",
                                    "Quality: {quality()}%"
                                }
                                input {
                                    r#type: "range",
                                    class: "w-full",
                                    min: "10",
                                    max: "100",
                                    value: "{quality()}",
                                    disabled: uploading(),
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            quality.set(val);
                                        }
                                    },
                                }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "Lower quality = smaller file size"
                                }
                            }
                        }
                    } else {
                        button {
                            class: "w-full p-8 border-2 border-dashed border-border rounded-lg hover:border-blue-500 hover:bg-blue-500/5 transition",
                            onclick: handle_file_select,
                            div { class: "flex flex-col items-center gap-2",
                                svg {
                                    class: "w-10 h-10 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path { d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" }
                                }
                                p { class: "font-medium", "Click to select a file" }
                                p { class: "text-sm text-muted-foreground", "Images, videos, or audio" }
                            }
                        }
                    }

                    // Upload progress
                    if uploading() {
                        div {
                            div { class: "flex items-center justify-between text-sm mb-2",
                                span { "Uploading..." }
                                span { "{upload_progress.unwrap_or(0.0) as u32}%" }
                            }
                            div { class: "h-2 bg-muted rounded-full overflow-hidden",
                                div {
                                    class: "h-full bg-blue-500 transition-all duration-300",
                                    style: "width: {upload_progress.unwrap_or(0.0)}%",
                                }
                            }
                        }
                    }

                    // Error
                    if let Some(err) = error() {
                        div { class: "p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500 text-sm",
                            "{err}"
                        }
                    }
                }

                // Footer
                div { class: "p-4 border-t border-border flex justify-end gap-2",
                    button {
                        class: "px-4 py-2 text-muted-foreground hover:text-foreground transition",
                        disabled: uploading(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition disabled:opacity-50",
                        disabled: file_data.read().is_none() || uploading(),
                        onclick: handle_upload,
                        if uploading() { "Uploading..." } else { "Upload" }
                    }
                }
            }
        }
    }
}

/// Validate and normalize a server URL
fn validate_and_normalize_server_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Try parsing with https:// prefix if no scheme
    let url_str = if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };

    let url = url::Url::parse(&url_str)
        .map_err(|e| format!("Invalid URL: {}", e))?;

    // Validate scheme
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("URL must use http or https".to_string());
    }

    // Ensure host is present
    let host = url.host_str().ok_or("URL must have a host")?;
    if host.is_empty() {
        return Err("URL must have a host".to_string());
    }

    // Normalize: scheme://host[:port] only (strip paths)
    let mut normalized = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        normalized.push_str(&format!(":{}", port));
    }

    Ok(normalized)
}

/// Server list component for the Servers tab
#[component]
fn ServerList(
    servers: Vec<String>,
    on_add_server: EventHandler<String>,
    on_remove_server: EventHandler<String>,
    on_set_preferred: EventHandler<String>,
) -> Element {
    let mut new_server_url = use_signal(String::new);
    let mut new_server_error = use_signal(|| None::<String>);
    let mut publishing = use_signal(|| false);
    let mut publish_result = use_signal(|| None::<Result<String, String>>);

    let handle_add = move |_| {
        match validate_and_normalize_server_url(&new_server_url()) {
            Ok(normalized) => {
                on_add_server.call(normalized);
                new_server_url.set(String::new());
                new_server_error.set(None);
            }
            Err(e) => {
                new_server_error.set(Some(e));
            }
        }
    };

    let handle_publish = move |_| {
        publishing.set(true);
        publish_result.set(None);
        spawn(async move {
            match blossom_store::publish_user_servers().await {
                Ok(id) => {
                    publish_result.set(Some(Ok(format!("Published: {}", &id[..16]))));
                }
                Err(e) => {
                    publish_result.set(Some(Err(e)));
                }
            }
            publishing.set(false);
        });
    };

    rsx! {
        div { class: "space-y-6",
            // Server list
            div { class: "space-y-2",
                for (i, server) in servers.iter().enumerate() {
                    div {
                        key: "{server}",
                        class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg",

                        // Preferred indicator
                        button {
                            class: if i == 0 { "text-yellow-500" } else { "text-muted-foreground hover:text-yellow-500" },
                            title: if i == 0 { "Primary server" } else { "Set as primary" },
                            onclick: {
                                let url = server.clone();
                                move |_| on_set_preferred.call(url.clone())
                            },
                            svg {
                                class: "w-5 h-5",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: if i == 0 { "currentColor" } else { "none" },
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path { d: "M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" }
                            }
                        }

                        // Server URL
                        span { class: "flex-1 font-mono text-sm truncate", "{server}" }

                        // Remove button
                        if servers.len() > 1 {
                            button {
                                class: "p-1 text-muted-foreground hover:text-red-500 transition",
                                title: "Remove server",
                                onclick: {
                                    let url = server.clone();
                                    move |_| on_remove_server.call(url.clone())
                                },
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path { d: "M6 18L18 6M6 6l12 12" }
                                }
                            }
                        }
                    }
                }
            }

            // Add server
            div { class: "flex gap-2",
                input {
                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm",
                    placeholder: "https://blossom.example.com",
                    value: "{new_server_url()}",
                    oninput: move |e| new_server_url.set(e.value()),
                    onkeypress: move |e| {
                        if e.key() == Key::Enter {
                            match validate_and_normalize_server_url(&new_server_url()) {
                                Ok(normalized) => {
                                    on_add_server.call(normalized);
                                    new_server_url.set(String::new());
                                    new_server_error.set(None);
                                }
                                Err(err) => {
                                    new_server_error.set(Some(err));
                                }
                            }
                        }
                    },
                }
                button {
                    class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                    onclick: handle_add,
                    "Add"
                }
            }

            // URL validation error
            if let Some(err) = new_server_error() {
                div { class: "text-red-500 text-sm", "{err}" }
            }

            // Publish to Nostr
            div { class: "pt-4 border-t border-border",
                div { class: "flex items-center justify-between",
                    div {
                        p { class: "font-medium", "Publish Server List" }
                        p { class: "text-sm text-muted-foreground", "Save your server preferences to Nostr (kind 10063)" }
                    }
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition disabled:opacity-50",
                        disabled: publishing(),
                        onclick: handle_publish,
                        if publishing() { "Publishing..." } else { "Publish" }
                    }
                }

                // Result message
                if let Some(result) = publish_result() {
                    match result {
                        Ok(msg) => rsx! {
                            div { class: "mt-2 p-2 bg-green-500/10 border border-green-500/20 rounded-lg text-green-500 text-sm",
                                "{msg}"
                            }
                        },
                        Err(err) => rsx! {
                            div { class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500 text-sm",
                                "{err}"
                            }
                        },
                    }
                }
            }

            // Preset servers
            div { class: "pt-4 border-t border-border",
                p { class: "text-sm font-medium mb-2", "Preset Servers" }
                div { class: "flex flex-wrap gap-2",
                    for (name, url) in [
                        ("Primal", "https://blossom.primal.net"),
                        ("Blossom Band", "https://blossom.band"),
                        ("F7Z", "https://blossom.f7z.io"),
                        ("Nostria EU", "https://mibo.eu.nostria.app"),
                        ("Nostria US", "https://mibo.us.nostria.app"),
                    ] {
                        {
                            // Use URL parsing for proper comparison (avoids false positives from substring matching)
                            let is_added = servers.iter().any(|s| {
                                match (url::Url::parse(s), url::Url::parse(url)) {
                                    (Ok(server_url), Ok(preset_url)) => {
                                        server_url.host_str() == preset_url.host_str()
                                            && server_url.port_or_known_default() == preset_url.port_or_known_default()
                                    }
                                    _ => false
                                }
                            });
                            rsx! {
                                button {
                                    key: "{url}",
                                    class: if is_added {
                                        "px-3 py-1.5 text-sm bg-muted text-muted-foreground rounded-lg cursor-not-allowed"
                                    } else {
                                        "px-3 py-1.5 text-sm bg-muted hover:bg-accent rounded-lg transition"
                                    },
                                    disabled: is_added,
                                    onclick: {
                                        let url = url.to_string();
                                        move |_| on_add_server.call(url.clone())
                                    },
                                    if is_added { "✓ " } else { "+ " }
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
