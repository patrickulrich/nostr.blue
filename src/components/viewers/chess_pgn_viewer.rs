use crate::components::chess::ChessBoard;
use crate::components::{ClientInitializing, ReplyComposer, ThreadedComment};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::services::aggregation::get_counts_with_count_fallback;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::types::ViewerRole;
use crate::stores::nostr_client;
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use crate::utils::build_thread_tree;
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind, Timestamp};
use std::collections::HashMap;
use std::time::Duration;

fn merge_comments(existing: Vec<NostrEvent>, fetched: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut by_id: HashMap<EventId, NostrEvent> = existing
        .into_iter()
        .map(|event| (event.id, event))
        .collect();
    for event in fetched {
        by_id.insert(event.id, event);
    }
    let mut merged: Vec<NostrEvent> = by_id.into_values().collect();
    merged.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged
}

fn insert_comment(existing: Vec<NostrEvent>, event: NostrEvent) -> Vec<NostrEvent> {
    merge_comments(existing, vec![event])
}

fn pgn_tag_value(tags: &[(String, String)], key: &str) -> Option<String> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

async fn fetch_chess_pgn_by_id(
    event_id: EventId,
) -> Result<Option<NostrEvent>, String> {
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::Custom(64))
        .limit(1);

    let events = crate::stores::nostr_client::fetching::fetch_chess_events(
        filter.clone(),
        Duration::from_secs(10),
    )
    .await
    .map_err(|e| format!("Failed to fetch chess game: {}", e))?;

    if !events.is_empty() {
        return Ok(events.into_iter().next());
    }

    let fallback =
        crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
            filter,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| format!("Failed to fetch from connected relays: {}", e))?;

    Ok(fallback.into_iter().next())
}

#[component]
pub fn ChessPgnViewer(note_id: String) -> Element {
    let mut chess_event = use_signal(|| None::<NostrEvent>);
    let mut game_state: Signal<GameState> = use_signal(GameState::new_game);
    let mut pgn_headers: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut comments_error = use_signal(|| None::<String>);
    let mut loading_comments = use_signal(|| false);
    let mut reply_total = use_signal(|| 0usize);
    let mut comments_refresh = use_signal(|| 0u64);
    let mut show_comment_composer = use_signal(|| false);
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();

    use_effect(use_reactive!(|note_id| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            let resolved_id = crate::utils::nips::chess::parse_game_id(&note_id)
                .and_then(|hex| EventId::from_hex(&hex).ok());
            let Some(eid) = resolved_id else {
                error.set(Some("Invalid note ID".to_string()));
                loading.set(false);
                return;
            };
            match fetch_chess_pgn_by_id(eid).await {
                Ok(Some(event)) => {
                    match GameState::from_pgn(&event.content) {
                        Ok((gs, tags)) => {
                            game_state.set(gs);
                            pgn_headers.set(tags);
                            chess_event.set(Some(event));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                    loading.set(false);
                }
                Ok(None) => {
                    error.set(Some("Chess game not found".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    }));

    use_effect(move || {
        let _ = *comments_refresh.read();
        let event = chess_event.read().clone();
        let Some(event) = event else {
            return;
        };
        let event_id = event.id;
        spawn(async move {
            if comments.read().is_empty() {
                loading_comments.set(true);
            }
            comments_error.set(None);
            let counts = get_counts_with_count_fallback(&event_id, Duration::from_secs(10)).await;
            reply_total.with_mut(|total| *total = (*total).max(counts.replies));

            let filter = Filter::new()
                .kinds([Kind::Comment, Kind::TextNote])
                .event(event_id)
                .limit(500);
            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                Ok(comment_events) => {
                    invalidate_thread_tree_cache(&event_id);
                    let merged = merge_comments(comments.read().clone(), comment_events);
                    comments.set(merged);
                }
                Err(e) => {
                    log::error!("Failed to fetch chess game comments: {}", e);
                    if comments.read().is_empty() {
                        comments_error.set(Some(format!("Failed to load comments: {}", e)));
                    }
                }
            }
            loading_comments.set(false);
        });
    });

    {
        let event_id = chess_event.read().clone().map(|e| e.id);
        let comment_filter = event_id.map(|eid| {
            Filter::new()
                .kinds([Kind::Comment, Kind::TextNote])
                .event(eid)
                .since(Timestamp::now())
                .limit(0)
        });
        let mut comments_mut = comments;
        let mut comments_error_mut = comments_error;
        let mut reply_total_mut = reply_total;
        use_relay_subscription(comment_filter, move |event: &nostr::Event| {
            let already_exists = comments_mut.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                if let Some(eid) = event_id {
                    invalidate_thread_tree_cache(&eid);
                }
                comments_error_mut.set(None);
                let next_comments = insert_comment(comments_mut.read().clone(), event.clone());
                comments_mut.set(next_comments);
                reply_total_mut.with_mut(|count| *count = count.saturating_add(1));
            }
        });
    }

    let headers = pgn_headers.read();
    let white = pgn_tag_value(&headers, "White").unwrap_or_else(|| "?".to_string());
    let black = pgn_tag_value(&headers, "Black").unwrap_or_else(|| "?".to_string());
    let result_str = pgn_tag_value(&headers, "Result").unwrap_or_else(|| "*".to_string());
    let event_name = pgn_tag_value(&headers, "Event");
    let date = pgn_tag_value(&headers, "Date");
    drop(headers);

    let current_event = chess_event.read().clone();
    let route_loaded_comments = comments.read().len();
    let route_replies_count = std::cmp::max(*reply_total.read(), route_loaded_comments);
    let route_comments_partial = route_loaded_comments == 0 && route_replies_count > 0;

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: crate::routes::Route::GamesHub {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                        "Back"
                    }
                    h1 { class: "text-xl font-bold", "\u{265F} Chess Game" }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if *loading.read() {
                    div { class: "flex items-center justify-center py-12",
                        div { class: "flex flex-col items-center gap-3 text-muted-foreground",
                            span { class: "inline-block w-8 h-8 border-4 border-current border-t-transparent rounded-full animate-spin" }
                            "Loading chess game..."
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{265F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: crate::routes::Route::GamesHub {},
                            class: "inline-block px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Games"
                        }
                    }
                } else if current_event.is_some() {
                    // Metadata
                    div { class: "p-4 space-y-4",
                        div { class: "rounded-xl border border-border bg-card p-3 space-y-1",
                            if let Some(name) = event_name.clone() {
                                p { class: "text-sm font-medium text-foreground", {name} }
                            }
                            div { class: "flex items-center justify-between",
                                span { class: "text-sm text-foreground", "{white} vs {black}" }
                                span { class: "text-sm font-mono text-muted-foreground", {result_str.clone()} }
                            }
                            if let Some(d) = date.clone() {
                                p { class: "text-xs text-muted-foreground", {d} }
                            }
                        }

                        // Board
                        ChessBoard {
                            game_state: game_state,
                            interactive: false,
                            viewer_role: ViewerRole::Spectator,
                            perspective: rschess::Color::White,
                            on_move: None,
                        }

                        // Move navigation
                        div { class: "flex items-center justify-center gap-2",
                            button {
                                class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                                disabled: game_state.read().pointer() == 0,
                                onclick: move |_| { game_state.write().go_to_start(); },
                                "\u{23EE}"
                            }
                            button {
                                class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                                disabled: game_state.read().pointer() == 0,
                                onclick: move |_| { game_state.write().step_back(); },
                                "\u{25C0}"
                            }
                            span { class: "text-xs text-muted-foreground min-w-[3rem] text-center",
                                {
                                    let gs = game_state.read();
                                    format!("{}/{}", gs.pointer(), gs.total_moves())
                                }
                            }
                            button {
                                class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                                disabled: game_state.read().pointer() >= game_state.read().total_moves(),
                                onclick: move |_| { game_state.write().step_forward(); },
                                "\u{25B6}"
                            }
                            button {
                                class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                                disabled: game_state.read().pointer() >= game_state.read().total_moves(),
                                onclick: move |_| { game_state.write().go_to_end(); },
                                "\u{23ED}"
                            }
                        }

                        // Moves
                        div { class: "mt-2 rounded-xl border border-border bg-card p-3",
                            h3 { class: "text-sm font-medium text-foreground mb-2", "Moves" }
                            pre { class: "text-xs text-muted-foreground whitespace-pre-wrap font-mono leading-relaxed",
                                { game_state.read().movetext() }
                            }
                        }
                    }

                    // Comments section
                    div { class: "border-t border-border pt-4 px-4",
                        div { class: "mb-4 flex items-center justify-between",
                            h3 { class: "text-lg font-semibold",
                                if route_comments_partial {
                                    "Comments (showing {route_loaded_comments} of {route_replies_count})"
                                } else if route_replies_count > route_loaded_comments {
                                    "Comments (showing {route_loaded_comments} of {route_replies_count})"
                                } else if route_replies_count > 0 {
                                    "Comments ({route_replies_count})"
                                } else {
                                    "Comments"
                                }
                            }
                            if let Some(_event) = current_event.clone() {
                                button {
                                    class: "px-3 py-1.5 text-sm rounded-lg border border-border hover:bg-accent transition",
                                    onclick: move |_| show_comment_composer.set(true),
                                    "Reply"
                                }
                            }
                        }

                        if *loading_comments.read() {
                            div { class: "flex items-center justify-center py-10",
                                div { class: "text-center",
                                    div { class: "animate-spin text-4xl mb-2", "\u{26A1}" }
                                    p { class: "text-muted-foreground", "Loading comments..." }
                                }
                            }
                        } else {
                            {
                                let comment_vec = comments.read().clone();
                                let event_clone = chess_event.read().clone();
                                if let Some(err) = comments_error.read().as_ref() {
                                    rsx! {
                                        div { class: "flex flex-col items-center justify-center py-10 px-4 text-center",
                                            p { class: "text-destructive font-medium", "Could not load comments" }
                                            p { class: "text-sm text-muted-foreground mt-1 mb-4", "{err}" }
                                            button {
                                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                                onclick: move |_| comments_refresh.with_mut(|v| *v = v.wrapping_add(1)),
                                                "Retry"
                                            }
                                        }
                                    }
                                } else if let Some(event) = event_clone {
                                    let thread_tree = build_thread_tree(comment_vec, &event.id);
                                    if route_comments_partial && thread_tree.is_empty() {
                                        rsx! {
                                            div { class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                                p { "Comments unavailable" }
                                                p { class: "text-sm", "Showing 0 of {route_replies_count} known comments." }
                                            }
                                        }
                                    } else if thread_tree.is_empty() {
                                        rsx! {
                                            div { class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                                p { "No comments yet" }
                                                p { class: "text-sm", "Be the first to comment!" }
                                            }
                                        }
                                    } else {
                                        let cached_muted = cached_muted_posts.read().clone();
                                        let cached_blocked = cached_blocked_users.read().clone();
                                        let _event_id_for_reply = event.id;
                                        rsx! {
                                            div { class: "divide-y divide-border",
                                                for node in thread_tree {
                                                    ThreadedComment {
                                                        key: "{node.event.id}",
                                                        node: node.clone(),
                                                        depth: 0,
                                                        cached_muted_posts: cached_muted.clone(),
                                                        cached_blocked_users: cached_blocked.clone(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! { div {} }
                                }
                            }
                        }
                    }

                    // Reply composer
                    if let Some(event) = chess_event.read().clone() {
                        if *show_comment_composer.read() {
                            ReplyComposer {
                                target: event.clone(),
                                root_event: None,
                                on_close: move |_| show_comment_composer.set(false),
                                on_success: move |reply_event: NostrEvent| {
                                    show_comment_composer.set(false);
                                    let event_id = event.id;
                                    invalidate_thread_tree_cache(&event_id);
                                    comments_error.set(None);
                                    let next_comments = insert_comment(comments.read().clone(), reply_event);
                                    comments.set(next_comments);
                                    reply_total.with_mut(|count| *count = count.saturating_add(1));
                                },
                            }
                        }
                    }
                } else {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{265F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Chess game not found" }
                        Link {
                            to: crate::routes::Route::GamesHub {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Games"
                        }
                    }
                }
            }
        }
    }
}
