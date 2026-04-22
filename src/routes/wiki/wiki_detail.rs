//! Wiki Detail Route
//! View individual NIP-54 wiki page (Kind 30818)
use crate::components::icons::{
    ArrowLeftIcon, BookOpenIcon, CheckIcon, ChevronDownIcon, GitMergeIcon, Link2Icon,
    PenSquareIcon, ShareIcon, UserIcon, XIcon,
};
use crate::components::{
    ShareModal, WikiBacklinks, WikiDownloadMenu, WikiForwardLinks, WikiMetadataCard, WikiOutline,
    WikiPageContent, WikiPageNotFound, WikiPageSkeleton,
};
use crate::routes::Route;
use crate::stores::wiki_store::{self, CachedWikiPage, WikiMetadata};
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::nip54::WikiMergeRequest;
use crate::utils::time::format_relative_time_ex;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::ToBech32;
/// Wiki page detail view
#[component]
pub fn WikiDetail(npub: String, identifier: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut page = use_signal(|| None::<CachedWikiPage>);
    let mut error = use_signal(|| None::<String>);
    let mut show_share_modal = use_signal(|| false);
    let identifier_for_notfound = identifier.clone();
    let auth = auth_store::AUTH_STATE.read();
    let is_logged_in = auth.pubkey.is_some();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let n = npub.clone();
        let id = identifier.clone();
        spawn(async move {
            loading.set(true);
            if let Some(target) = wiki_store::get_redirect_target(&id) {
                log::info!("Wiki redirect: {} -> {}", id, target);
                nav.push(Route::WikiDetail {
                    npub: n,
                    identifier: target,
                });
                return;
            }
            match wiki_store::fetch_wiki_page(&n, &id).await {
                Ok(result) => {
                    page.set(result);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });
    let go_back = move |_| {
        nav.push(Route::WikiHome {});
    };
    let handle_edit = move || {
        nav.push(Route::WikiNew {});
    };
    let handle_create = move |_id: String| {
        nav.push(Route::WikiNew {});
    };
    rsx! {
        div { class: "max-w-5xl mx-auto px-4 py-6",
            div { class: "flex items-center justify-between mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back to Wiki"
                }
                div { class: "flex items-center gap-2",
                    if is_logged_in && page.read().is_some() {
                        button {
                            class: "flex items-center gap-2 px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                            onclick: move |_| handle_edit(),
                            PenSquareIcon { class: "w-4 h-4" }
                            "Edit"
                        }
                    }
                    if let Some(ref wiki_page) = *page.read() {
                        WikiDownloadMenu { page: wiki_page.clone() }
                    }
                    if page.read().is_some() {
                        button {
                            class: "p-2 rounded-lg hover:bg-accent transition-colors",
                            title: "Share",
                            onclick: move |_| show_share_modal.set(true),
                            ShareIcon { class: "w-5 h-5" }
                        }
                    }
                }
            }
            if *show_share_modal.read() {
                if let Some(ref wiki_page) = *page.read() {
                    ShareModal {
                        event: wiki_page.event.clone(),
                        on_close: move |_| show_share_modal.set(false),
                    }
                }
            }
            if let Some(ref e) = *error.read() {
                div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading page: {e}"
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && page.read().is_none())
            {
                WikiPageSkeleton {}
            } else if let Some(ref wiki_page) = *page.read() {
                div { class: "flex gap-8",
                    div { class: "flex-1 min-w-0",
                        WikiPageContent {
                            page: wiki_page.clone(),
                            show_header: true,
                            show_edit: is_logged_in,
                            on_edit: Some(EventHandler::new(move |_: ()| handle_edit())),
                        }
                        if !wiki_page.forward_links.is_empty() {
                            div { class: "mt-8 pt-6 border-t border-border",
                                WikiForwardLinks {
                                    links: wiki_page.forward_links.clone(),
                                    author_npub: Some(wiki_page.event.pubkey.to_bech32().unwrap_or_default()),
                                }
                            }
                        }
                        {
                            let author_hex = wiki_page.event.pubkey.to_hex();
                            let author_profile = profiles::get_profile(&author_hex);
                            let author_name = author_profile
                                .as_ref()
                                .and_then(|p| p.display_name.clone().or(p.name.clone()))
                                .unwrap_or_else(|| truncate_pubkey(&author_hex));
                            let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
                            let time_ago = format_relative_time_ex(wiki_page.event.created_at, true, true);
                            rsx! {
                                div { class: "mt-8 pt-6 border-t border-border",
                                    div { class: "flex items-start gap-4",
                                        Link {
                                            to: Route::Profile {
                                                pubkey: author_hex.clone(),
                                            },
                                            class: "shrink-0",
                                            if let Some(ref picture) = author_picture {
                                                img {
                                                    class: "w-12 h-12 rounded-full object-cover",
                                                    src: "{picture}",
                                                    alt: "{author_name}",
                                                }
                                            } else {
                                                div { class: "w-12 h-12 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-lg font-medium text-primary-foreground",
                                                    "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                                                }
                                            }
                                        }
                                        div { class: "flex-1",
                                            div { class: "flex items-center gap-2",
                                                Link {
                                                    to: Route::Profile {
                                                        pubkey: author_hex.clone(),
                                                    },
                                                    class: "font-medium text-foreground hover:underline",
                                                    "{author_name}"
                                                }
                                                span { class: "text-xs text-muted-foreground", "• {time_ago}" }
                                            }
                                            div { class: "flex items-center gap-4 mt-2 text-sm",
                                                Link {
                                                    to: Route::Profile {
                                                        pubkey: author_hex.clone(),
                                                    },
                                                    class: "flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors",
                                                    UserIcon { class: "w-4 h-4" }
                                                    "View profile"
                                                }
                                                Link {
                                                    to: Route::WikiSlug {
                                                        slug: wiki_page.event.pubkey.to_bech32().unwrap_or_else(|_| author_hex.clone()),
                                                    },
                                                    class: "flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors",
                                                    BookOpenIcon { class: "w-4 h-4" }
                                                    "More articles"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        BacklinksCollapsiblePanel {
                            identifier: wiki_page.article.identifier.clone(),
                            backlink_count: wiki_page.backward_links.len(),
                            author_npub: Some(wiki_page.event.pubkey.to_bech32().unwrap_or_default()),
                        }                        {
                            let author_pubkey = wiki_page.event.pubkey.to_hex();
                            let current_user = auth_store::AUTH_STATE.read().pubkey.clone();
                            let is_author = current_user.map(|p| p == author_pubkey).unwrap_or(false);
                            if is_author {
                                rsx! {
                                    MergeRequestsPanel { a_tag: wiki_page.a_tag.clone() }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                    aside { class: "hidden lg:block w-56 shrink-0",
                        div { class: "sticky top-6 space-y-4",
                            WikiMetadataCard {
                                metadata: WikiMetadata {
                                    identifier: wiki_page.article.identifier.clone(),
                                    title: wiki_page.article.title.clone(),
                                    summary: wiki_page.article.summary.clone(),
                                    author_pubkey: wiki_page.event.pubkey.to_hex(),
                                    created_at: wiki_page.event.created_at.as_secs(),
                                    naddr: wiki_page.article.naddr.clone(),
                                },
                            }
                            WikiOutline {
                                content: wiki_page.article.content.clone(),
                                class: "p-4 bg-muted/30 rounded-lg".to_string(),
                            }
                        }
                    }
                }
            } else {
                WikiPageNotFound {
                    identifier: identifier_for_notfound.clone(),
                    can_create: is_logged_in,
                    on_create: Some(EventHandler::new(handle_create)),
                }
            }
        }
    }
}
/// Collapsible backlinks panel
#[component]
fn BacklinksCollapsiblePanel(
    identifier: String,
    backlink_count: usize,
    #[props(default = None)] author_npub: Option<String>,
) -> Element {
    let mut is_expanded = use_signal(|| false);
    if backlink_count == 0 {
        return rsx! {};
    }
    let expanded = *is_expanded.read();
    rsx! {
        div { class: "mt-8 pt-6 border-t border-border",
            button {
                class: "w-full flex items-center justify-between text-left group",
                onclick: move |_| is_expanded.set(!expanded),
                div { class: "flex items-center gap-2",
                    Link2Icon { class: "w-5 h-5 text-muted-foreground" }
                    h4 { class: "font-medium text-foreground", "Pages linking here" }
                    span { class: "px-2 py-0.5 text-xs font-medium rounded-full bg-primary/10 text-primary",
                        "{backlink_count}"
                    }
                }
                ChevronDownIcon { class: if expanded { "w-5 h-5 text-muted-foreground transition-transform" } else { "w-5 h-5 text-muted-foreground transition-transform -rotate-90" } }
            }
            if expanded {
                div { class: "mt-4",
                    WikiBacklinks {
                        identifier: identifier.clone(),
                        author_npub,
                        class: "bg-muted/30 rounded-lg p-4".to_string(),
                    }
                }
            }
        }
    }
}
/// Collapsible merge requests panel (for article authors)
#[component]
fn MergeRequestsPanel(a_tag: String) -> Element {
    let mut is_expanded = use_signal(|| false);
    let mut is_loading = use_signal(|| false);
    let mut merge_requests = use_signal(Vec::<WikiMergeRequest>::new);
    let mut has_fetched = use_signal(|| false);
    let a_tag_fetch = a_tag.clone();
    use_effect(move || {
        let expanded = *is_expanded.read();
        let fetched = *has_fetched.read();
        if expanded && !fetched {
            let tag = a_tag_fetch.clone();
            spawn(async move {
                is_loading.set(true);
                match wiki_store::fetch_wiki_merge_requests(&tag, 20).await {
                    Ok(requests) => {
                        merge_requests.set(requests);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch merge requests: {}", e);
                    }
                }
                is_loading.set(false);
                has_fetched.set(true);
            });
        }
    });
    let expanded = *is_expanded.read();
    let requests = merge_requests.read();
    let request_count = requests.len();
    rsx! {
        div { class: "mt-8 pt-6 border-t border-border",
            button {
                class: "w-full flex items-center justify-between text-left group",
                onclick: move |_| is_expanded.set(!expanded),
                div { class: "flex items-center gap-2",
                    GitMergeIcon { class: "w-5 h-5 text-muted-foreground" }
                    h4 { class: "font-medium text-foreground", "Merge Requests" }
                    if *has_fetched.read() && request_count > 0 {
                        span { class: "px-2 py-0.5 text-xs font-medium rounded-full bg-amber-500/10 text-amber-600",
                            "{request_count}"
                        }
                    }
                }
                ChevronDownIcon { class: if expanded { "w-5 h-5 text-muted-foreground transition-transform" } else { "w-5 h-5 text-muted-foreground transition-transform -rotate-90" } }
            }
            if expanded {
                div { class: "mt-4",
                    if *is_loading.read() {
                        div { class: "flex items-center justify-center py-8",
                            div { class: "w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        }
                    } else if requests.is_empty() {
                        div { class: "text-center py-8 text-muted-foreground",
                            p { "No pending merge requests" }
                            p { class: "text-sm mt-1",
                                "When others fork and edit your article, their merge requests will appear here."
                            }
                        }
                    } else {
                        div { class: "space-y-3",
                            for request in requests.iter() {
                                MergeRequestCard {
                                    key: "{request.event_id}",
                                    request: request.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Individual merge request card
#[component]
fn MergeRequestCard(request: WikiMergeRequest) -> Element {
    let requester_profile = profiles::get_profile(&request.pubkey);
    let requester_name = requester_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&request.pubkey));
    let requester_picture = requester_profile.as_ref().and_then(|p| p.picture.clone());
    let time_ago =
        format_relative_time_ex(nostr_sdk::Timestamp::from(request.created_at), true, true);
    let source_id_short = if request.source_event_id.len() > 12 {
        format!("{}...", &request.source_event_id[..12])
    } else {
        request.source_event_id.clone()
    };
    rsx! {
        div { class: "p-4 bg-muted/30 rounded-lg border border-border",
            div { class: "flex items-start gap-3",
                Link {
                    to: Route::Profile {
                        pubkey: request.pubkey.clone(),
                    },
                    class: "shrink-0",
                    if let Some(ref picture) = requester_picture {
                        img {
                            class: "w-10 h-10 rounded-full object-cover",
                            src: "{picture}",
                            alt: "{requester_name}",
                        }
                    } else {
                        div { class: "w-10 h-10 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-sm font-medium text-primary-foreground",
                            "{requester_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2",
                        Link {
                            to: Route::Profile {
                                pubkey: request.pubkey.clone(),
                            },
                            class: "font-medium text-foreground hover:underline truncate",
                            "{requester_name}"
                        }
                        span { class: "text-xs text-muted-foreground shrink-0", "• {time_ago}" }
                    }
                    if !request.content.trim().is_empty() {
                        p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                            "{request.content}"
                        }
                    }
                    p { class: "text-xs text-muted-foreground mt-2",
                        "Source: "
                        code { class: "text-xs bg-muted px-1 py-0.5 rounded", "{source_id_short}" }
                    }
                }
            }
            div { class: "flex items-center gap-2 mt-4 pt-3 border-t border-border",
                button {
                    class: "flex items-center gap-1.5 px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                    title: "Accept merge request",
                    CheckIcon { class: "w-4 h-4" }
                    "Accept"
                }
                button {
                    class: "flex items-center gap-1.5 px-3 py-1.5 text-sm bg-muted text-muted-foreground rounded-md hover:bg-muted/80 transition-colors",
                    title: "View changes",
                    "View Changes"
                }
                button {
                    class: "flex items-center gap-1.5 px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10 rounded-md transition-colors ml-auto",
                    title: "Reject merge request",
                    XIcon { class: "w-4 h-4" }
                    "Reject"
                }
            }
        }
    }
}
