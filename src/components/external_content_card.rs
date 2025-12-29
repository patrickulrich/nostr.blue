//! External Content Card Components
//!
//! NIP-73 external content display cards for books, papers, Bitcoin, podcasts, etc.
//! Each card type provides appropriate visualization and linking.

use dioxus::prelude::*;
use nostr::nips::nip73::ExternalContentId;
use crate::utils::nip73;
use crate::components::icons;
use crate::services::{mempool, openlibrary::{self, CoverSize}};
use crate::stores::settings_store;
use crate::utils::format::format_sats_with_unit;

/// Generic external content card dispatcher
/// Routes to the appropriate card based on content type
#[component]
pub fn ExternalContentCard(
    content: ExternalContentId,
    #[props(default = false)] compact: bool,
) -> Element {
    match &content {
        ExternalContentId::Book(isbn) => rsx! {
            BookCard { isbn: isbn.clone(), compact }
        },
        ExternalContentId::Paper(doi) => rsx! {
            PaperCard { doi: doi.clone(), compact }
        },
        ExternalContentId::BlockchainTransaction { chain, transaction_hash, .. } if chain == "bitcoin" => rsx! {
            BitcoinTxCard { txid: transaction_hash.clone(), compact }
        },
        ExternalContentId::BlockchainAddress { chain, address, .. } if chain == "bitcoin" => rsx! {
            BitcoinAddressCard { address: address.clone(), compact }
        },
        ExternalContentId::PodcastFeed(guid) => rsx! {
            PodcastGuidCard { guid: guid.clone(), is_episode: false, compact }
        },
        ExternalContentId::PodcastEpisode(guid) => rsx! {
            PodcastGuidCard { guid: guid.clone(), is_episode: true, compact }
        },
        ExternalContentId::Geohash(hash) => rsx! {
            GeohashCard { geohash: hash.clone(), compact }
        },
        ExternalContentId::Movie(isan) => rsx! {
            MovieCard { isan: isan.clone(), compact }
        },
        // For unsupported types, show a generic link
        _ => {
            if let Some(url) = nip73::get_explorer_url(&content) {
                rsx! {
                    GenericContentCard {
                        content: content.clone(),
                        url,
                        compact
                    }
                }
            } else {
                rsx! {}
            }
        }
    }
}

/// List of external content cards from an event
#[component]
pub fn ExternalContentList(
    contents: Vec<(ExternalContentId, Option<String>)>,
    #[props(default = false)] compact: bool,
) -> Element {
    if contents.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "flex flex-wrap gap-2 mt-2",
            for (content, _hint) in contents {
                ExternalContentCard {
                    key: "{nip73::get_raw_identifier(&content)}",
                    content: content.clone(),
                    compact
                }
            }
        }
    }
}

// ============================================================================
// Individual Card Components
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct BookCardProps {
    isbn: String,
    #[props(default = false)]
    compact: bool,
}

/// Book card with OpenLibrary cover and metadata
#[component]
fn BookCard(props: BookCardProps) -> Element {
    let isbn = props.isbn.clone();
    let isbn_for_fetch = isbn.clone();

    // Fetch book metadata
    let book_data = use_resource(move || {
        let isbn = isbn_for_fetch.clone();
        async move { openlibrary::get_book_by_isbn(&isbn).await }
    });

    // Cover URL is direct - no API call needed
    let cover_url = openlibrary::get_cover_url(&isbn, CoverSize::Medium);
    let book_url = format!("https://openlibrary.org/isbn/{}", isbn);

    if props.compact {
        return rsx! {
            a {
                href: "{book_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-amber-500/10 text-amber-600 dark:text-amber-400 rounded-full hover:bg-amber-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::BOOK
                }
                "ISBN: {isbn}"
            }
        };
    }

    rsx! {
        a {
            href: "{book_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            // Book cover
            img {
                src: "{cover_url}",
                alt: "Book cover",
                class: "w-12 h-18 object-cover rounded shadow-sm flex-shrink-0 bg-muted"
            }

            div {
                class: "flex flex-col min-w-0",

                // Title and author (from API, or show ISBN if loading)
                match &*book_data.read() {
                    Some(Ok(book)) => rsx! {
                        h4 {
                            class: "font-semibold text-sm truncate",
                            "{book.title}"
                        }
                        if !book.authors.is_empty() {
                            p {
                                class: "text-xs text-muted-foreground truncate",
                                "by {book.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(\", \")}"
                            }
                        }
                    },
                    Some(Err(_)) => rsx! {
                        h4 {
                            class: "font-semibold text-sm",
                            "ISBN: {isbn}"
                        }
                        p {
                            class: "text-xs text-muted-foreground",
                            "Could not load book info"
                        }
                    },
                    None => rsx! {
                        h4 {
                            class: "font-semibold text-sm",
                            "ISBN: {isbn}"
                        }
                        p {
                            class: "text-xs text-muted-foreground",
                            "Loading book info..."
                        }
                    }
                }

                // OpenLibrary link
                span {
                    class: "text-xs text-primary mt-1",
                    "View on OpenLibrary →"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PaperCardProps {
    doi: String,
    #[props(default = false)]
    compact: bool,
}

/// Paper/DOI card
#[component]
fn PaperCard(props: PaperCardProps) -> Element {
    let doi_url = format!("https://doi.org/{}", props.doi);

    if props.compact {
        return rsx! {
            a {
                href: "{doi_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-blue-500/10 text-blue-600 dark:text-blue-400 rounded-full hover:bg-blue-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::FILE_TEXT
                }
                "DOI: {props.doi}"
            }
        };
    }

    rsx! {
        a {
            href: "{doi_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex items-center gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            div {
                class: "w-10 h-10 rounded bg-blue-500/10 flex items-center justify-center flex-shrink-0",
                span {
                    class: "w-5 h-5 text-blue-500",
                    dangerous_inner_html: icons::FILE_TEXT
                }
            }

            div {
                class: "flex flex-col min-w-0",
                h4 {
                    class: "font-semibold text-sm",
                    "Academic Paper"
                }
                p {
                    class: "text-xs text-muted-foreground truncate",
                    "DOI: {props.doi}"
                }
                span {
                    class: "text-xs text-primary mt-0.5",
                    "View paper →"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BitcoinTxCardProps {
    txid: String,
    #[props(default = false)]
    compact: bool,
}

/// Bitcoin transaction card with expandable details
#[component]
fn BitcoinTxCard(props: BitcoinTxCardProps) -> Element {
    let txid = props.txid.clone();
    let mut expanded = use_signal(|| false);
    let mut tx_data = use_signal(|| None::<Result<mempool::BitcoinTransaction, String>>);

    let mempool_endpoint = settings_store::get_mempool_endpoint();
    let mempool_url = format!("{}/tx/{}", mempool_endpoint.trim_end_matches("/api"), txid);

    // Truncated txid for display (using char-safe slicing)
    let short_txid = if txid.chars().count() > 16 {
        let start: String = txid.chars().take(8).collect();
        let end: String = txid.chars().skip(txid.chars().count().saturating_sub(8)).collect();
        format!("{}...{}", start, end)
    } else {
        txid.clone()
    };

    // Fetch data when expanded
    let fetch_tx = {
        let txid = txid.clone();
        let endpoint = mempool_endpoint.clone();
        move |_| {
            let txid = txid.clone();
            let endpoint = endpoint.clone();
            expanded.set(true);
            // Fetch if not loaded yet, or allow retry on previous error
            let should_fetch = match &*tx_data.read() {
                None => true,
                Some(Err(_)) => true,
                Some(Ok(_)) => false,
            };
            if should_fetch {
                spawn(async move {
                    let result = mempool::get_transaction(&endpoint, &txid).await;
                    tx_data.set(Some(result));
                });
            }
        }
    };

    if props.compact {
        return rsx! {
            a {
                href: "{mempool_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-orange-500/10 text-orange-600 dark:text-orange-400 rounded-full hover:bg-orange-500/20 transition font-mono",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::BITCOIN
                }
                "{short_txid}"
            }
        };
    }

    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden max-w-sm",

            // Header - always visible
            button {
                class: "w-full flex items-center gap-3 p-3 hover:bg-muted/50 transition text-left",
                onclick: fetch_tx,

                div {
                    class: "w-10 h-10 rounded bg-orange-500/10 flex items-center justify-center flex-shrink-0",
                    span {
                        class: "w-5 h-5 text-orange-500",
                        dangerous_inner_html: icons::BITCOIN
                    }
                }

                div {
                    class: "flex flex-col min-w-0 flex-1",
                    h4 {
                        class: "font-semibold text-sm",
                        "Bitcoin Transaction"
                    }
                    p {
                        class: "text-xs text-muted-foreground font-mono truncate",
                        "{short_txid}"
                    }
                }

                span {
                    class: "text-muted-foreground",
                    dangerous_inner_html: if *expanded.read() { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN }
                }
            }

            // Expanded details
            if *expanded.read() {
                div {
                    class: "border-t border-border p-3 bg-muted/30",

                    match &*tx_data.read() {
                        Some(Ok(tx)) => rsx! {
                            div {
                                class: "space-y-2 text-sm",

                                // Status
                                div {
                                    class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Status" }
                                    if tx.status.confirmed {
                                        span {
                                            class: "text-green-500 font-medium",
                                            "Confirmed"
                                        }
                                    } else {
                                        span {
                                            class: "text-yellow-500 font-medium",
                                            "Pending"
                                        }
                                    }
                                }

                                // Block height
                                if let Some(height) = tx.status.block_height {
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Block" }
                                        span { class: "font-mono", "{height}" }
                                    }
                                }

                                // Fee
                                div {
                                    class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Fee" }
                                    span {
                                        class: "font-mono",
                                        if tx.vsize > 0 {
                                            "{tx.fee} sats ({tx.fee / tx.vsize as u64} sat/vB)"
                                        } else {
                                            "{tx.fee} sats"
                                        }
                                    }
                                }

                                // Link to mempool
                                a {
                                    href: "{mempool_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    class: "flex items-center gap-1 text-primary hover:underline mt-2",
                                    "View on Mempool"
                                    span {
                                        class: "w-3 h-3",
                                        dangerous_inner_html: icons::EXTERNAL_LINK
                                    }
                                }
                            }
                        },
                        Some(Err(e)) => rsx! {
                            div {
                                class: "text-sm text-destructive",
                                "Error: {e}"
                            }
                            a {
                                href: "{mempool_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-1 text-primary hover:underline text-sm mt-2",
                                "View on Mempool"
                                span {
                                    class: "w-3 h-3",
                                    dangerous_inner_html: icons::EXTERNAL_LINK
                                }
                            }
                        },
                        None => rsx! {
                            div {
                                class: "flex items-center gap-2 text-sm text-muted-foreground",
                                div { class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading transaction..."
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BitcoinAddressCardProps {
    address: String,
    #[props(default = false)]
    compact: bool,
}

/// Bitcoin address card with expandable details
#[component]
fn BitcoinAddressCard(props: BitcoinAddressCardProps) -> Element {
    let address = props.address.clone();
    let mut expanded = use_signal(|| false);
    let mut addr_data = use_signal(|| None::<Result<mempool::BitcoinAddress, String>>);

    let mempool_endpoint = settings_store::get_mempool_endpoint();
    let mempool_url = format!("{}/address/{}", mempool_endpoint.trim_end_matches("/api"), address);

    // Truncated address for display (UTF-8 safe)
    let short_addr = {
        let chars: Vec<char> = address.chars().collect();
        if chars.len() > 20 {
            let prefix: String = chars[..10].iter().collect();
            let suffix: String = chars[chars.len() - 8..].iter().collect();
            format!("{}...{}", prefix, suffix)
        } else {
            address.clone()
        }
    };

    // Fetch data when expanded
    let fetch_addr = {
        let address = address.clone();
        let endpoint = mempool_endpoint.clone();
        move |_| {
            let address = address.clone();
            let endpoint = endpoint.clone();
            expanded.set(true);
            // Fetch if not loaded yet, or allow retry on previous error
            let should_fetch = match &*addr_data.read() {
                None => true,
                Some(Err(_)) => true,
                Some(Ok(_)) => false,
            };
            if should_fetch {
                spawn(async move {
                    let result = mempool::get_address(&endpoint, &address).await;
                    addr_data.set(Some(result));
                });
            }
        }
    };

    if props.compact {
        return rsx! {
            a {
                href: "{mempool_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-orange-500/10 text-orange-600 dark:text-orange-400 rounded-full hover:bg-orange-500/20 transition font-mono",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::BITCOIN
                }
                "{short_addr}"
            }
        };
    }

    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden max-w-sm",

            // Header
            button {
                class: "w-full flex items-center gap-3 p-3 hover:bg-muted/50 transition text-left",
                onclick: fetch_addr,

                div {
                    class: "w-10 h-10 rounded bg-orange-500/10 flex items-center justify-center flex-shrink-0",
                    span {
                        class: "w-5 h-5 text-orange-500",
                        dangerous_inner_html: icons::BITCOIN
                    }
                }

                div {
                    class: "flex flex-col min-w-0 flex-1",
                    h4 {
                        class: "font-semibold text-sm",
                        "Bitcoin Address"
                    }
                    p {
                        class: "text-xs text-muted-foreground font-mono truncate",
                        "{short_addr}"
                    }
                }

                span {
                    class: "text-muted-foreground",
                    dangerous_inner_html: if *expanded.read() { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN }
                }
            }

            // Expanded details
            if *expanded.read() {
                div {
                    class: "border-t border-border p-3 bg-muted/30",

                    match &*addr_data.read() {
                        Some(Ok(addr)) => {
                            let total_received = addr.chain_stats.funded_txo_sum;
                            let total_sent = addr.chain_stats.spent_txo_sum;
                            let balance = total_received.saturating_sub(total_sent);

                            rsx! {
                                div {
                                    class: "space-y-2 text-sm",

                                    // Balance
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Balance" }
                                        span {
                                            class: "font-mono font-medium",
                                            "{format_sats_with_unit(balance)}"
                                        }
                                    }

                                    // Total received
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Received" }
                                        span { class: "font-mono text-green-500", "{format_sats_with_unit(total_received)}" }
                                    }

                                    // Total sent
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Sent" }
                                        span { class: "font-mono text-red-500", "{format_sats_with_unit(total_sent)}" }
                                    }

                                    // Transaction count
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Transactions" }
                                        span { class: "font-mono", "{addr.chain_stats.tx_count}" }
                                    }

                                    // Link to mempool
                                    a {
                                        href: "{mempool_url}",
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        class: "flex items-center gap-1 text-primary hover:underline mt-2",
                                        "View on Mempool"
                                        span {
                                            class: "w-3 h-3",
                                            dangerous_inner_html: icons::EXTERNAL_LINK
                                        }
                                    }
                                }
                            }
                        },
                        Some(Err(e)) => rsx! {
                            div {
                                class: "text-sm text-destructive",
                                "Error: {e}"
                            }
                            a {
                                href: "{mempool_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-1 text-primary hover:underline text-sm mt-2",
                                "View on Mempool"
                                span {
                                    class: "w-3 h-3",
                                    dangerous_inner_html: icons::EXTERNAL_LINK
                                }
                            }
                        },
                        None => rsx! {
                            div {
                                class: "flex items-center gap-2 text-sm text-muted-foreground",
                                div { class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading address..."
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PodcastGuidCardProps {
    guid: String,
    #[props(default = false)]
    is_episode: bool,
    #[props(default = false)]
    compact: bool,
}

/// Podcast GUID card (links to internal podcast route or Podcast Index)
#[component]
fn PodcastGuidCard(props: PodcastGuidCardProps) -> Element {
    let podcast_index_url = if props.is_episode {
        format!("https://podcastindex.org/search?q={}", props.guid)
    } else {
        format!("https://podcastindex.org/podcast/{}", props.guid)
    };

    // Short GUID for display (UTF-8 safe)
    let short_guid = {
        let chars: Vec<char> = props.guid.chars().collect();
        if chars.len() > 20 {
            let prefix: String = chars[..17].iter().collect();
            format!("{}...", prefix)
        } else {
            props.guid.clone()
        }
    };

    let label = if props.is_episode { "Episode" } else { "Podcast" };

    if props.compact {
        return rsx! {
            a {
                href: "{podcast_index_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-purple-500/10 text-purple-600 dark:text-purple-400 rounded-full hover:bg-purple-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::PODCAST
                }
                "{label}"
            }
        };
    }

    rsx! {
        a {
            href: "{podcast_index_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex items-center gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            div {
                class: "w-10 h-10 rounded bg-purple-500/10 flex items-center justify-center flex-shrink-0",
                span {
                    class: "w-5 h-5 text-purple-500",
                    dangerous_inner_html: icons::PODCAST
                }
            }

            div {
                class: "flex flex-col min-w-0",
                h4 {
                    class: "font-semibold text-sm",
                    "{label}"
                }
                p {
                    class: "text-xs text-muted-foreground font-mono truncate",
                    "{short_guid}"
                }
                span {
                    class: "text-xs text-primary mt-0.5",
                    "View on Podcast Index →"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct GeohashCardProps {
    geohash: String,
    #[props(default = false)]
    compact: bool,
}

/// Geohash location card
#[component]
fn GeohashCard(props: GeohashCardProps) -> Element {
    let geohash_url = format!("https://geohash.org/{}", props.geohash);
    let osm_url = format!(
        "https://www.openstreetmap.org/search?query={}",
        props.geohash
    );

    if props.compact {
        return rsx! {
            a {
                href: "{geohash_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-green-500/10 text-green-600 dark:text-green-400 rounded-full hover:bg-green-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::MAP_PIN
                }
                "{props.geohash}"
            }
        };
    }

    rsx! {
        a {
            href: "{osm_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex items-center gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            div {
                class: "w-10 h-10 rounded bg-green-500/10 flex items-center justify-center flex-shrink-0",
                span {
                    class: "w-5 h-5 text-green-500",
                    dangerous_inner_html: icons::MAP_PIN
                }
            }

            div {
                class: "flex flex-col min-w-0",
                h4 {
                    class: "font-semibold text-sm",
                    "Location"
                }
                p {
                    class: "text-xs text-muted-foreground font-mono",
                    "{props.geohash}"
                }
                span {
                    class: "text-xs text-primary mt-0.5",
                    "View on OpenStreetMap →"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MovieCardProps {
    isan: String,
    #[props(default = false)]
    compact: bool,
}

/// Movie/ISAN card
#[component]
fn MovieCard(props: MovieCardProps) -> Element {
    let isan_url = format!("https://web.isan.org/public/en/search?isan={}", props.isan);

    if props.compact {
        return rsx! {
            a {
                href: "{isan_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-pink-500/10 text-pink-600 dark:text-pink-400 rounded-full hover:bg-pink-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::FILM
                }
                "ISAN"
            }
        };
    }

    rsx! {
        a {
            href: "{isan_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex items-center gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            div {
                class: "w-10 h-10 rounded bg-pink-500/10 flex items-center justify-center flex-shrink-0",
                span {
                    class: "w-5 h-5 text-pink-500",
                    dangerous_inner_html: icons::FILM
                }
            }

            div {
                class: "flex flex-col min-w-0",
                h4 {
                    class: "font-semibold text-sm",
                    "Movie/Video"
                }
                p {
                    class: "text-xs text-muted-foreground font-mono truncate",
                    "ISAN: {props.isan}"
                }
                span {
                    class: "text-xs text-primary mt-0.5",
                    "View on ISAN Registry →"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct GenericContentCardProps {
    content: ExternalContentId,
    url: String,
    #[props(default = false)]
    compact: bool,
}

/// Generic content card for unsupported types
#[component]
fn GenericContentCard(props: GenericContentCardProps) -> Element {
    let display_name = nip73::get_display_name(&props.content);

    if props.compact {
        return rsx! {
            a {
                href: "{props.url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-muted text-muted-foreground rounded-full hover:bg-muted/80 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::EXTERNAL_LINK
                }
                "{display_name}"
            }
        };
    }

    rsx! {
        a {
            href: "{props.url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "flex items-center gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition max-w-sm",

            div {
                class: "w-10 h-10 rounded bg-muted flex items-center justify-center flex-shrink-0",
                span {
                    class: "w-5 h-5 text-muted-foreground",
                    dangerous_inner_html: icons::EXTERNAL_LINK
                }
            }

            div {
                class: "flex flex-col min-w-0",
                h4 {
                    class: "font-semibold text-sm truncate",
                    "{display_name}"
                }
                span {
                    class: "text-xs text-primary mt-0.5",
                    "Open link →"
                }
            }
        }
    }
}

