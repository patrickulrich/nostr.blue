//! External Content Card Components
//!
//! NIP-73 external content display cards for books, papers, Bitcoin, podcasts, etc.
//! Each card type provides appropriate visualization and linking.
use crate::components::icons;
use crate::components::podcast::episode_card::DisplayEpisode;
use crate::routes::Route;
use crate::services::{
    mempool,
    openlibrary::{self, CoverSize},
    podcast_index,
    podcast_rss::format_duration,
};
use crate::stores::{music_player, settings_store};
use crate::utils::format::format_sats_with_unit;
use crate::utils::nip73;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use nostr::nips::nip73::ExternalContentId;
/// Truncate a GUID for privacy-conscious logging (first 8 chars + "...")
fn truncate_guid(guid: &str) -> String {
    if guid.chars().count() > 12 {
        let prefix: String = guid.chars().take(8).collect();
        format!("{}...", prefix)
    } else {
        guid.to_string()
    }
}

#[cfg(test)]
fn extract_podcast_feed_guid(contents: &[(ExternalContentId, Option<String>)]) -> Option<String> {
    contents.iter().find_map(|(content, _)| match content {
        ExternalContentId::PodcastFeed(guid) => Some(guid.clone()),
        _ => None,
    })
}

fn map_podcast_items(
    contents: &[(usize, (ExternalContentId, Option<String>))],
) -> Vec<(usize, ExternalContentId, Option<String>)> {
    let mut feed_guids_after = vec![None; contents.len()];
    let mut next_feed_guid = None;
    for (slot, (_, (content, _))) in contents.iter().enumerate().rev() {
        if let ExternalContentId::PodcastFeed(guid) = content {
            next_feed_guid = Some(guid.clone());
        }
        feed_guids_after[slot] = next_feed_guid.clone();
    }

    let mut current_feed_guid = None;
    let mut mapped = Vec::new();
    for (slot, (original_index, (content, _))) in contents.iter().enumerate() {
        match content {
            ExternalContentId::PodcastFeed(guid) => {
                current_feed_guid = Some(guid.clone());
                mapped.push((*original_index, content.clone(), None));
            }
            ExternalContentId::PodcastEpisode(_) => {
                let feed_guid = current_feed_guid
                    .clone()
                    .or_else(|| feed_guids_after[slot].clone());
                mapped.push((*original_index, content.clone(), feed_guid));
            }
            _ => {}
        }
    }
    mapped
}

/// Generic external content card dispatcher
/// Routes to the appropriate card based on content type
#[component]
pub fn ExternalContentCard(
    content: ExternalContentId,
    #[props(default = false)] compact: bool,
    /// Optional podcast GUID for episode lookups (from same note's podcast:guid tag)
    podcast_guid: Option<String>,
) -> Element {
    log::debug!("[ExternalContentCard] Dispatching content: {:?}", content);
    match &content {
        ExternalContentId::Book(isbn) => {
            rsx! {
                BookCard { isbn: isbn.clone(), compact }
            }
        }
        ExternalContentId::Paper(doi) => {
            rsx! {
                PaperCard { doi: doi.clone(), compact }
            }
        }
        ExternalContentId::BlockchainTransaction {
            chain,
            transaction_hash,
            ..
        } if chain == "bitcoin" => {
            rsx! {
                BitcoinTxCard { txid: transaction_hash.clone(), compact }
            }
        }
        ExternalContentId::BlockchainAddress { chain, address, .. } if chain == "bitcoin" => {
            rsx! {
                BitcoinAddressCard { address: address.clone(), compact }
            }
        }
        ExternalContentId::PodcastFeed(guid) => {
            rsx! {
                PodcastGuidCard { guid: guid.clone(), is_episode: false, compact }
            }
        }
        ExternalContentId::PodcastEpisode(guid) => {
            rsx! {
                PodcastEpisodeGuidCard {
                    guid: guid.clone(),
                    podcast_guid: podcast_guid.clone(),
                    compact,
                }
            }
        }
        ExternalContentId::Geohash(hash) => {
            rsx! {
                GeohashCard { geohash: hash.clone(), compact }
            }
        }
        ExternalContentId::Movie(isan) => {
            rsx! {
                MovieCard { isan: isan.clone(), compact }
            }
        }
        _ => {
            if let Some(url) = nip73::get_explorer_url(&content) {
                rsx! {
                    GenericContentCard { content: content.clone(), url, compact }
                }
            } else {
                rsx! {}
            }
        }
    }
}
/// List of external content cards from an event
/// Podcasts are always rendered as full cards (non-compact) for playback support
#[component]
pub fn ExternalContentList(
    contents: Vec<(ExternalContentId, Option<String>)>,
    #[props(default = false)] compact: bool,
) -> Element {
    if contents.is_empty() {
        return rsx! {};
    }
    let (podcasts, other): (Vec<_>, Vec<_>) =
        contents
            .into_iter()
            .enumerate()
            .partition(|(_, (content, _))| {
                matches!(
                    content,
                    ExternalContentId::PodcastFeed(_) | ExternalContentId::PodcastEpisode(_)
                )
            });
    let podcast_cards = map_podcast_items(&podcasts);
    rsx! {
        div { class: "flex flex-col gap-2 mt-2",
            for (index, content, podcast_guid) in podcast_cards.iter() {
                ExternalContentCard {
                    key: "{nip73::get_raw_identifier(content)}:{index}",
                    content: content.clone(),
                    compact: false,
                    podcast_guid: podcast_guid.clone(),
                }
            }
            if !other.is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for (index, (content, _hint)) in other.iter() {
                        ExternalContentCard {
                            key: "{nip73::get_raw_identifier(content)}:{index}",
                            content: content.clone(),
                            compact,
                            podcast_guid: None,
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_podcast_feed_guid, map_podcast_items};
    use nostr::nips::nip73::ExternalContentId;

    #[test]
    fn extract_podcast_feed_guid_returns_first_feed_guid() {
        let contents = vec![
            (
                ExternalContentId::PodcastEpisode("episode-guid".to_string()),
                Some("https://fountain.fm/episode/example".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("feed-guid".to_string()),
                Some("https://example.com/show".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("second-feed-guid".to_string()),
                None,
            ),
        ];

        assert_eq!(
            extract_podcast_feed_guid(&contents),
            Some("feed-guid".to_string())
        );
    }

    #[test]
    fn extract_podcast_feed_guid_returns_none_without_feed() {
        let contents = vec![
            (
                ExternalContentId::PodcastEpisode("episode-guid".to_string()),
                Some("https://fountain.fm/episode/example".to_string()),
            ),
            (ExternalContentId::Book("9780765382030".to_string()), None),
        ];

        assert_eq!(extract_podcast_feed_guid(&contents), None);
    }

    #[test]
    fn extract_podcast_feed_guid_ignores_hint_urls() {
        let contents = vec![
            (
                ExternalContentId::PodcastEpisode("episode-guid".to_string()),
                Some("https://wrong.example/podcast-guid".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("actual-feed-guid".to_string()),
                Some("https://fountain.fm/show/example".to_string()),
            ),
        ];

        assert_eq!(
            extract_podcast_feed_guid(&contents),
            Some("actual-feed-guid".to_string())
        );
    }

    #[test]
    fn extract_podcast_feed_guid_mixed_feed_regression() {
        let contents = vec![
            (
                ExternalContentId::PodcastFeed("feed-A".to_string()),
                Some("https://example.com/feed-a".to_string()),
            ),
            (
                ExternalContentId::PodcastEpisode("episode-1".to_string()),
                Some("https://example.com/episode-1".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("feed-B".to_string()),
                Some("https://example.com/feed-b".to_string()),
            ),
            (
                ExternalContentId::PodcastEpisode("episode-2".to_string()),
                Some("https://example.com/episode-2".to_string()),
            ),
        ];

        assert_eq!(
            extract_podcast_feed_guid(&contents),
            Some("feed-A".to_string())
        );

        let enumerated: Vec<_> = contents.into_iter().enumerate().collect();
        let mapped = map_podcast_items(&enumerated);
        assert_eq!(mapped[1].2.as_deref(), Some("feed-A"));
        assert_eq!(mapped[3].2.as_deref(), Some("feed-B"));
    }

    #[test]
    fn map_podcast_items_looks_ahead_when_episode_precedes_feed() {
        let contents = vec![
            (
                ExternalContentId::PodcastEpisode("episode-1".to_string()),
                Some("https://example.com/episode-1".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("feed-A".to_string()),
                Some("https://example.com/feed-a".to_string()),
            ),
        ];

        let enumerated: Vec<_> = contents.into_iter().enumerate().collect();
        let mapped = map_podcast_items(&enumerated);
        assert_eq!(mapped[0].2.as_deref(), Some("feed-A"));
    }

    #[test]
    fn map_podcast_items_preserves_original_indices_for_keys() {
        let contents = vec![
            (
                ExternalContentId::Book("9780765382030".to_string()),
                Some("https://example.com/book".to_string()),
            ),
            (
                ExternalContentId::PodcastFeed("feed-A".to_string()),
                Some("https://example.com/feed-a".to_string()),
            ),
            (
                ExternalContentId::PodcastEpisode("episode-1".to_string()),
                Some("https://example.com/episode-1".to_string()),
            ),
        ];

        let podcasts: Vec<_> = contents
            .into_iter()
            .enumerate()
            .filter(|(_, (content, _))| {
                matches!(
                    content,
                    ExternalContentId::PodcastFeed(_) | ExternalContentId::PodcastEpisode(_)
                )
            })
            .collect();

        let mapped = map_podcast_items(&podcasts);
        assert_eq!(mapped[0].0, 1);
        assert_eq!(mapped[1].0, 2);
    }
}

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
    let book_data = use_resource(move || {
        let isbn = isbn_for_fetch.clone();
        async move { openlibrary::get_book_by_isbn(&isbn).await }
    });
    let cover_url = openlibrary::get_cover_url(&isbn, CoverSize::Medium);
    let book_url = format!("https://openlibrary.org/isbn/{}", isbn);
    if props.compact {
        return rsx! {
            a {
                href: "{book_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-amber-500/10 text-amber-600 dark:text-amber-400 rounded-full hover:bg-amber-500/20 transition",
                span { class: "w-3.5 h-3.5", dangerous_inner_html: icons::BOOK }
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
            img {
                src: "{cover_url}",
                alt: "Book cover",
                class: "w-12 h-18 object-cover rounded shadow-xs shrink-0 bg-muted",
            }
            div { class: "flex flex-col min-w-0",
                match &*book_data.read() {
                    Some(Ok(book)) => rsx! {
                        h4 { class: "font-semibold text-sm truncate", "{book.title}" }
                        if !book.authors.is_empty() {
                            p { class: "text-xs text-muted-foreground truncate",
                                "by {book.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(\", \")}"
                            }
                        }
                    },
                    Some(Err(_)) => rsx! {
                        h4 { class: "font-semibold text-sm", "ISBN: {isbn}" }
                        p { class: "text-xs text-muted-foreground", "Could not load book info" }
                    },
                    None => rsx! {
                        h4 { class: "font-semibold text-sm", "ISBN: {isbn}" }
                        p { class: "text-xs text-muted-foreground", "Loading book info..." }
                    },
                }
                span { class: "text-xs text-primary mt-1", "View on OpenLibrary →" }
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
                    dangerous_inner_html: icons::FILE_TEXT,
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
            div { class: "w-10 h-10 rounded bg-blue-500/10 flex items-center justify-center shrink-0",
                span {
                    class: "w-5 h-5 text-blue-500",
                    dangerous_inner_html: icons::FILE_TEXT,
                }
            }
            div { class: "flex flex-col min-w-0",
                h4 { class: "font-semibold text-sm", "Academic Paper" }
                p { class: "text-xs text-muted-foreground truncate", "DOI: {props.doi}" }
                span { class: "text-xs text-primary mt-0.5", "View paper →" }
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
    let mempool_url = format!("{}/tx/{}", mempool_endpoint.trim_end_matches("/api"), txid,);
    let short_txid = if txid.chars().count() > 16 {
        let start: String = txid.chars().take(8).collect();
        let end: String = txid
            .chars()
            .skip(txid.chars().count().saturating_sub(8))
            .collect();
        format!("{}...{}", start, end)
    } else {
        txid.clone()
    };
    let fetch_tx = {
        let txid = txid.clone();
        let endpoint = mempool_endpoint.clone();
        move |_| {
            let txid = txid.clone();
            let endpoint = endpoint.clone();
            expanded.set(true);
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
                    dangerous_inner_html: icons::BITCOIN,
                }
                "{short_txid}"
            }
        };
    }
    rsx! {
        div { class: "rounded-lg border border-border bg-card overflow-hidden max-w-sm",
            button {
                class: "w-full flex items-center gap-3 p-3 hover:bg-muted/50 transition text-left",
                onclick: fetch_tx,
                div { class: "w-10 h-10 rounded bg-orange-500/10 flex items-center justify-center shrink-0",
                    span {
                        class: "w-5 h-5 text-orange-500",
                        dangerous_inner_html: icons::BITCOIN,
                    }
                }
                div { class: "flex flex-col min-w-0 flex-1",
                    h4 { class: "font-semibold text-sm", "Bitcoin Transaction" }
                    p { class: "text-xs text-muted-foreground font-mono truncate",
                        "{short_txid}"
                    }
                }
                span {
                    class: "text-muted-foreground",
                    dangerous_inner_html: if *expanded.read() { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN },
                }
            }
            if *expanded.read() {
                div { class: "border-t border-border p-3 bg-muted/30",
                    match &*tx_data.read() {
                        Some(Ok(tx)) => rsx! {
                            div { class: "space-y-2 text-sm",
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Status" }
                                    if tx.status.confirmed {
                                        span { class: "text-green-500 font-medium", "Confirmed" }
                                    } else {
                                        span { class: "text-yellow-500 font-medium", "Pending" }
                                    }
                                }
                                if let Some(height) = tx.status.block_height {
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Block" }
                                        span { class: "font-mono", "{height}" }
                                    }
                                }
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Fee" }
                                    span { class: "font-mono",
                                        if tx.vsize > 0 {
                                            "{tx.fee} sats ({tx.fee / tx.vsize as u64} sat/vB)"
                                        } else {
                                            "{tx.fee} sats"
                                        }
                                    }
                                }
                                a {
                                    href: "{mempool_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    class: "flex items-center gap-1 text-primary hover:underline mt-2",
                                    "View on Mempool"
                                    span { class: "w-3 h-3", dangerous_inner_html: icons::EXTERNAL_LINK }
                                }
                            }
                        },
                        Some(Err(e)) => rsx! {
                            div { class: "text-sm text-destructive", "Error: {e}" }
                            a {
                                href: "{mempool_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-1 text-primary hover:underline text-sm mt-2",
                                "View on Mempool"
                                span { class: "w-3 h-3", dangerous_inner_html: icons::EXTERNAL_LINK }
                            }
                        },
                        None => rsx! {
                            div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                                div { class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading transaction..."
                            }
                        },
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
    let mempool_url = format!(
        "{}/address/{}",
        mempool_endpoint.trim_end_matches("/api"),
        address,
    );
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
    let fetch_addr = {
        let address = address.clone();
        let endpoint = mempool_endpoint.clone();
        move |_| {
            let address = address.clone();
            let endpoint = endpoint.clone();
            expanded.set(true);
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
                    dangerous_inner_html: icons::BITCOIN,
                }
                "{short_addr}"
            }
        };
    }
    rsx! {
        div { class: "rounded-lg border border-border bg-card overflow-hidden max-w-sm",
            button {
                class: "w-full flex items-center gap-3 p-3 hover:bg-muted/50 transition text-left",
                onclick: fetch_addr,
                div { class: "w-10 h-10 rounded bg-orange-500/10 flex items-center justify-center shrink-0",
                    span {
                        class: "w-5 h-5 text-orange-500",
                        dangerous_inner_html: icons::BITCOIN,
                    }
                }
                div { class: "flex flex-col min-w-0 flex-1",
                    h4 { class: "font-semibold text-sm", "Bitcoin Address" }
                    p { class: "text-xs text-muted-foreground font-mono truncate",
                        "{short_addr}"
                    }
                }
                span {
                    class: "text-muted-foreground",
                    dangerous_inner_html: if *expanded.read() { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN },
                }
            }
            if *expanded.read() {
                div { class: "border-t border-border p-3 bg-muted/30",
                    match &*addr_data.read() {
                        Some(Ok(addr)) => {
                            let total_received = addr.chain_stats.funded_txo_sum;
                            let total_sent = addr.chain_stats.spent_txo_sum;
                            let balance = total_received.saturating_sub(total_sent);
                            rsx! {
                                div { class: "space-y-2 text-sm",
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Balance" }
                                        span { class: "font-mono font-medium", "{format_sats_with_unit(balance)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Received" }
                                        span { class: "font-mono text-green-500", "{format_sats_with_unit(total_received)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Sent" }
                                        span { class: "font-mono text-red-500", "{format_sats_with_unit(total_sent)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Transactions" }
                                        span { class: "font-mono", "{addr.chain_stats.tx_count}" }
                                    }
                                    a {
                                        href: "{mempool_url}",
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        class: "flex items-center gap-1 text-primary hover:underline mt-2",
                                        "View on Mempool"
                                        span { class: "w-3 h-3", dangerous_inner_html: icons::EXTERNAL_LINK }
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => rsx! {
                            div { class: "text-sm text-destructive", "Error: {e}" }
                            a {
                                href: "{mempool_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-1 text-primary hover:underline text-sm mt-2",
                                "View on Mempool"
                                span { class: "w-3 h-3", dangerous_inner_html: icons::EXTERNAL_LINK }
                            }
                        },
                        None => rsx! {
                            div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                                div { class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading address..."
                            }
                        },
                    }
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct PodcastEpisodeGuidCardProps {
    guid: String,
    /// Optional podcast GUID for more reliable episode lookups
    podcast_guid: Option<String>,
    #[props(default = false)]
    compact: bool,
}
/// Podcast episode card that fetches episode data and provides playback
#[component]
fn PodcastEpisodeGuidCard(props: PodcastEpisodeGuidCardProps) -> Element {
    let guid = props.guid.clone();
    let guid_for_fetch = guid.clone();
    let podcast_guid_for_fetch = props.podcast_guid.clone();
    log::debug!(
        "[PodcastEpisodeGuidCard] Rendering: {}",
        truncate_guid(&guid)
    );
    let episode_data = use_resource(move || {
        let guid = guid_for_fetch.clone();
        let podcast_guid = podcast_guid_for_fetch.clone();
        async move {
            log::debug!(
                "[PodcastEpisodeGuidCard] Fetching episode: {}",
                truncate_guid(&guid)
            );
            let result = podcast_index::get_episode_by_guid(&guid, podcast_guid.as_deref()).await;
            if let Err(e) = &result {
                log::error!("[PodcastEpisodeGuidCard] Fetch failed: {}", e);
            }
            result
        }
    });
    let display_episode = use_memo(move || {
        if let Some(Ok((ref episode, ref feed))) = *episode_data.read() {
            if let Some(feed) = feed {
                Some(DisplayEpisode::from_podcast_index_episode(episode, feed))
            } else {
                let minimal_feed = podcast_index::PodcastFeed {
                    id: episode.feed_id.unwrap_or(0),
                    title: episode
                        .feed_title
                        .clone()
                        .unwrap_or_else(|| "Unknown Podcast".to_string()),
                    url: episode.feed_url.clone().unwrap_or_default(),
                    original_url: None,
                    link: None,
                    description: None,
                    author: None,
                    owner_name: None,
                    image: episode.feed_image.clone(),
                    artwork: None,
                    language: None,
                    itunes_id: None,
                    podcast_guid: episode.podcast_guid.clone(),
                    episode_count: None,
                    categories: None,
                    trending_score: None,
                    value: None,
                };
                Some(DisplayEpisode::from_podcast_index_episode(
                    episode,
                    &minimal_feed,
                ))
            }
        } else {
            None
        }
    });
    let episode_id = use_memo(move || {
        display_episode
            .read()
            .as_ref()
            .map(|e| e.id.clone())
            .unwrap_or_default()
    });
    let episode_id_for_playing = episode_id;
    let is_playing = use_memo(move || {
        let ep_id = episode_id_for_playing();
        if ep_id.is_empty() {
            return false;
        }
        let player_state = music_player::MUSIC_PLAYER.read();
        if let Some(ref current) = player_state.current_track {
            current.id == ep_id && player_state.is_playing
        } else {
            false
        }
    });
    let handle_play = move |e: MouseEvent| {
        e.prevent_default();
        e.stop_propagation();
        if let Some(ref episode) = *display_episode.read() {
            let player_state = music_player::MUSIC_PLAYER.read();
            if let Some(ref current) = player_state.current_track {
                if current.id == episode.id && player_state.is_playing {
                    drop(player_state);
                    music_player::toggle_play();
                    return;
                }
            }
            drop(player_state);
            let track = episode.to_music_track();
            if !is_valid_http_url(&track.media_url) {
                log::warn!("[PodcastEpisodeGuidCard] Invalid track URL, skipping playback");
                return;
            }
            music_player::play_track(track, None, None);
        }
    };
    let route_ids = use_memo(move || {
        if let Some(Ok((ref episode, _))) = *episode_data.read() {
            Some((
                episode.feed_id.unwrap_or(0).to_string(),
                episode.id.to_string(),
            ))
        } else {
            None
        }
    });
    if episode_data.read().is_none() {
        return rsx! {
            div { class: "flex items-center gap-2 p-2 rounded-lg border border-border bg-card animate-pulse",
                div { class: "w-12 h-12 rounded bg-muted" }
                div { class: "flex-1 space-y-2",
                    div { class: "h-3 bg-muted rounded w-3/4" }
                    div { class: "h-2 bg-muted rounded w-1/2" }
                }
            }
        };
    }
    if let Some(Err(_)) = *episode_data.read() {
        let podcast_index_url = format!(
            "https://podcastindex.org/search?q={}",
            urlencoding::encode(&guid),
        );
        return rsx! {
            a {
                href: "{podcast_index_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-purple-500/10 text-purple-600 dark:text-purple-400 rounded-full hover:bg-purple-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::PODCAST,
                }
                "Episode"
            }
        };
    }
    let episode_opt = display_episode.read().clone();
    let Some(episode) = episode_opt else {
        return rsx! {};
    };
    let Some((podcast_id, episode_id)) = route_ids.read().clone() else {
        return rsx! {};
    };
    let image_url = episode
        .image
        .as_ref()
        .or(episode.podcast_image.as_ref())
        .filter(|url| is_valid_http_url(url))
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                episode.id
            )
        });
    let duration_str = episode
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());
    let has_v4v = episode.value.is_some();
    let title = episode.title.clone();
    let podcast_title = episode.podcast_title.clone();
    let pub_date = episode.pub_date.clone();
    if props.compact {
        return rsx! {
            div {
                class: "flex items-center gap-2 p-2 rounded-lg border border-border bg-card hover:bg-muted/50 transition group max-w-md",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "relative shrink-0",
                    img {
                        src: "{image_url}",
                        alt: "{title}",
                        class: "w-10 h-10 rounded object-cover",
                        loading: "lazy",
                    }
                    button {
                        class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 focus:opacity-100 group-focus-within:opacity-100 transition rounded",
                        onclick: handle_play,
                        aria_label: if *is_playing.read() { "Pause audio" } else { "Play audio" },
                        span {
                            class: "w-4 h-4 text-white",
                            dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    Link {
                        to: Route::PodcastRssEpisodeDetail {
                            podcast_id: podcast_id.clone(),
                            episode_id: episode_id.clone(),
                        },
                        class: "font-medium text-sm truncate block hover:underline cursor-pointer",
                        onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                        "{title}"
                    }
                    div { class: "text-xs text-muted-foreground truncate", "{podcast_title}" }
                }
                div { class: "flex items-center gap-1 text-xs text-muted-foreground shrink-0",
                    if has_v4v {
                        span {
                            class: "text-amber-500",
                            title: "Value 4 Value enabled",
                            dangerous_inner_html: icons::ZAP,
                        }
                    }
                    span { "{duration_str}" }
                }
            }
        };
    }
    rsx! {
        div {
            class: "flex items-start gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition group max-w-md",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "relative shrink-0",
                img {
                    src: "{image_url}",
                    alt: "{title}",
                    class: "w-16 h-16 rounded-lg object-cover",
                    loading: "lazy",
                }
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 focus:opacity-100 group-focus-within:opacity-100 transition rounded-lg",
                    onclick: handle_play,
                    aria_label: if *is_playing.read() { "Pause audio" } else { "Play audio" },
                    span {
                        class: "w-6 h-6 text-white",
                        dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                Link {
                    to: Route::PodcastRssEpisodeDetail {
                        podcast_id: podcast_id.clone(),
                        episode_id: episode_id.clone(),
                    },
                    class: if *is_playing.read() { "font-medium text-sm text-primary line-clamp-2 hover:underline cursor-pointer" } else { "font-medium text-sm line-clamp-2 hover:underline cursor-pointer" },
                    onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                    "{title}"
                }
                div { class: "text-xs text-muted-foreground truncate mt-0.5", "{podcast_title}" }
                div { class: "flex items-center gap-2 mt-1 text-xs text-muted-foreground",
                    span { "{duration_str}" }
                    if has_v4v {
                        span {
                            class: "text-amber-500",
                            title: "Value 4 Value enabled",
                            dangerous_inner_html: icons::ZAP,
                        }
                    }
                    if let Some(ref date) = pub_date {
                        span { "• {date}" }
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
/// Podcast GUID card - fetches podcast data and links to internal route
#[component]
fn PodcastGuidCard(props: PodcastGuidCardProps) -> Element {
    let guid = props.guid.clone();
    let guid_for_fetch = guid.clone();
    log::debug!("[PodcastGuidCard] Rendering: {}", truncate_guid(&guid));
    let podcast_data = use_resource(move || {
        let guid = guid_for_fetch.clone();
        async move {
            log::debug!(
                "[PodcastGuidCard] Fetching podcast: {}",
                truncate_guid(&guid)
            );
            let result = podcast_index::get_podcast_by_guid(&guid).await;
            if let Err(e) = &result {
                log::error!("[PodcastGuidCard] Fetch failed: {}", e);
            }
            result
        }
    });
    if podcast_data.read().is_none() {
        return rsx! {
            div { class: "flex items-center gap-2 p-2 rounded-lg border border-border bg-card animate-pulse",
                div { class: "w-12 h-12 rounded bg-muted" }
                div { class: "flex-1 space-y-2",
                    div { class: "h-3 bg-muted rounded w-3/4" }
                    div { class: "h-2 bg-muted rounded w-1/2" }
                }
            }
        };
    }
    if let Some(Err(_)) = *podcast_data.read() {
        let podcast_index_url = format!(
            "https://podcastindex.org/podcast/{}",
            urlencoding::encode(&guid),
        );
        return rsx! {
            a {
                href: "{podcast_index_url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "inline-flex items-center gap-1.5 px-2 py-1 text-xs bg-purple-500/10 text-purple-600 dark:text-purple-400 rounded-full hover:bg-purple-500/20 transition",
                span {
                    class: "w-3.5 h-3.5",
                    dangerous_inner_html: icons::PODCAST,
                }
                "Podcast"
            }
        };
    }
    let Some(Ok(podcast)) = &*podcast_data.read() else {
        return rsx! {};
    };
    let podcast = podcast.clone();
    let podcast_id = podcast.id.to_string();
    let title = podcast.title.clone();
    let image_url = podcast
        .get_image()
        .filter(|url| is_valid_http_url(url))
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                podcast.id
            )
        });
    let author = podcast.author.clone().or(podcast.owner_name.clone());
    let episode_count = podcast.episode_count;
    let has_v4v = podcast.value.is_some();
    if props.compact {
        return rsx! {
            Link {
                to: Route::PodcastRssFeedDetail {
                    podcast_id: podcast_id.clone(),
                },
                class: "block",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center gap-2 p-2 rounded-lg border border-border bg-card hover:bg-muted/50 transition group max-w-md cursor-pointer",
                    img {
                        src: "{image_url}",
                        alt: "{title}",
                        class: "w-10 h-10 rounded object-cover shrink-0",
                        loading: "lazy",
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm truncate", "{title}" }
                        if let Some(ref auth) = author {
                            div { class: "text-xs text-muted-foreground truncate", "{auth}" }
                        }
                    }
                    if has_v4v {
                        span {
                            class: "text-amber-500 shrink-0",
                            title: "Value 4 Value enabled",
                            dangerous_inner_html: icons::ZAP,
                        }
                    }
                }
            }
        };
    }
    rsx! {
        Link {
            to: Route::PodcastRssFeedDetail {
                podcast_id: podcast_id.clone(),
            },
            class: "block",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-start gap-3 p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition group max-w-md cursor-pointer",
                img {
                    src: "{image_url}",
                    alt: "{title}",
                    class: "w-16 h-16 rounded-lg object-cover shrink-0",
                    loading: "lazy",
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-sm line-clamp-2", "{title}" }
                    if let Some(ref auth) = author {
                        div { class: "text-xs text-muted-foreground truncate mt-0.5",
                            "{auth}"
                        }
                    }
                    div { class: "flex items-center gap-2 mt-1 text-xs text-muted-foreground",
                        if let Some(count) = episode_count {
                            span { "{count} episodes" }
                        }
                        if has_v4v {
                            span {
                                class: "text-amber-500",
                                title: "Value 4 Value enabled",
                                dangerous_inner_html: icons::ZAP,
                            }
                        }
                    }
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
        props.geohash,
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
                    dangerous_inner_html: icons::MAP_PIN,
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
            div { class: "w-10 h-10 rounded bg-green-500/10 flex items-center justify-center shrink-0",
                span {
                    class: "w-5 h-5 text-green-500",
                    dangerous_inner_html: icons::MAP_PIN,
                }
            }
            div { class: "flex flex-col min-w-0",
                h4 { class: "font-semibold text-sm", "Location" }
                p { class: "text-xs text-muted-foreground font-mono", "{props.geohash}" }
                span { class: "text-xs text-primary mt-0.5", "View on OpenStreetMap →" }
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
                span { class: "w-3.5 h-3.5", dangerous_inner_html: icons::FILM }
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
            div { class: "w-10 h-10 rounded bg-pink-500/10 flex items-center justify-center shrink-0",
                span {
                    class: "w-5 h-5 text-pink-500",
                    dangerous_inner_html: icons::FILM,
                }
            }
            div { class: "flex flex-col min-w-0",
                h4 { class: "font-semibold text-sm", "Movie/Video" }
                p { class: "text-xs text-muted-foreground font-mono truncate", "ISAN: {props.isan}" }
                span { class: "text-xs text-primary mt-0.5", "View on ISAN Registry →" }
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
                    dangerous_inner_html: icons::EXTERNAL_LINK,
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
            div { class: "w-10 h-10 rounded bg-muted flex items-center justify-center shrink-0",
                span {
                    class: "w-5 h-5 text-muted-foreground",
                    dangerous_inner_html: icons::EXTERNAL_LINK,
                }
            }
            div { class: "flex flex-col min-w-0",
                h4 { class: "font-semibold text-sm truncate", "{display_name}" }
                span { class: "text-xs text-primary mt-0.5", "Open link →" }
            }
        }
    }
}
