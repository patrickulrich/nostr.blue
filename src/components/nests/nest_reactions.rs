use crate::hooks::use_relay_subscription_to;
use dioxus::prelude::*;

/// How long a floating reaction stays on screen (10s sliding window).
const REACTION_DISPLAY_SECS: u64 = 10;

/// Cap on incoming reaction content rendered as a float — relays are
/// untrusted and kind 7 content is freeform.
const MAX_INCOMING_EMOJI_CHARS: usize = 8;

#[derive(Props, Clone, PartialEq)]
pub struct NestReactionsProps {
    pub room_coordinate: String,
    pub is_joined: bool,
    /// Effective relay set for the room (NIP-65 ∪ naddr hints ∪ room
    /// `relays` tag) — same targeting as the other room subscriptions.
    pub relay_urls: Vec<String>,
}

static REACTIONS: &[(&str, &str)] = &[
    ("❤️", "heart"),
    ("👏", "clap"),
    ("🔥", "fire"),
    ("😂", "laugh"),
    ("🎉", "party"),
];

#[component]
pub fn NestReactions(props: NestReactionsProps) -> Element {
    let floating_reactions = use_signal(Vec::<(String, f64, u32)>::new);
    let reaction_counter = use_signal(|| 0u32);
    let seen_reaction_ids = use_hook(std::collections::HashSet::<nostr_sdk::EventId>::new);

    // Incoming reactions (kind 7 tagged with the room's `a` coordinate) from
    // other users. Our own sends already render an optimistic local float,
    // so self-echo from relays is skipped; events are deduped by id (multiple
    // relays echo the same reaction). The `since` bound is pinned once per
    // mount so the filter stays PartialEq-stable across re-renders — the
    // parent re-renders on every 100ms level tick and a fresh `Timestamp::now()`
    // would trigger constant resubscription.
    {
        let coordinate = props.room_coordinate.clone();
        let relay_urls = props.relay_urls.clone();
        let mut floats = floating_reactions;
        let mut counter = reaction_counter;
        let mut seen = seen_reaction_ids;
        let my_pk = crate::stores::auth_store::get_pubkey().unwrap_or_default();
        let since_ts = use_hook(nostr_sdk::Timestamp::now);
        let reaction_filter = if coordinate.is_empty() {
            None
        } else {
            Some(
                nostr_sdk::Filter::new()
                    .kind(nostr_sdk::Kind::Reaction)
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                        coordinate.as_str(),
                    )
                    .since(since_ts)
                    .limit(50),
            )
        };
        use_relay_subscription_to(
            reaction_filter,
            None,
            relay_urls,
            move |event: &nostr::Event| {
                if event.kind.as_u16() != 7 {
                    return;
                }
                if !my_pk.is_empty() && event.pubkey.to_hex() == my_pk {
                    return;
                }
                if !seen.insert(event.id) {
                    return;
                }
                let emoji: String =
                    event.content.trim().chars().take(MAX_INCOMING_EMOJI_CHARS).collect();
                if emoji.is_empty() {
                    return;
                }
                let id = *counter.read();
                counter.set(id + 1);
                let now = crate::platform::timestamp::now_secs() as f64;
                floats.write().push((emoji, now, id));
            },
        );
    }

    // Float GC — runs regardless of joined state so late-expiring floats
    // from a just-left session still clear.
    {
        let mut reactions = floating_reactions;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep_ms(1000).await;
                let now = crate::platform::timestamp::now_secs();
                reactions.write().retain(|(_, created, _)| {
                    now.saturating_sub(*created as u64) < REACTION_DISPLAY_SECS
                });
            }
        });
    }

    if !props.is_joined {
        return rsx! {};
    }

    let coord = props.room_coordinate.clone();

    rsx! {
        div { class: "relative",
            div { class: "flex items-center gap-2 py-2 px-4",
                for (emoji, _label) in REACTIONS {
                    {
                        let emoji = emoji.to_string();
                        let coord = coord.clone();
                        rsx! {
                            button {
                                class: "w-10 h-10 rounded-full bg-muted hover:bg-accent flex items-center justify-center text-lg transition active:scale-90",
                                onclick: move |_: dioxus::prelude::Event<MouseData>| {
                                    let emoji_for_send = emoji.clone();
                                    let emoji_for_float = emoji.clone();
                                    let coord = coord.clone();
                                    let mut counter = reaction_counter;
                                    let mut floats = floating_reactions;
                                    spawn(async move {
                                        let _ = send_reaction(&coord, &emoji_for_send).await;
                                    });
                                    let id = *counter.read();
                                    counter.set(id + 1);
                                    let now = crate::platform::timestamp::now_secs() as f64;
                                    let mut floats_mut = floats.write();
                                    floats_mut.push((emoji_for_float, now, id));
                                },
                                "{emoji}"
                            }
                        }
                    }
                }
            }
            div { class: "absolute bottom-full left-0 right-0 pointer-events-none overflow-hidden h-20",
                for (emoji, _created, id) in floating_reactions.read().iter() {
                    div {
                        key: "{id}",
                        class: "absolute animate-float-up text-2xl",
                        style: "left: {(*id as f64 * 15.0) % 80.0}%; bottom: 0px;",
                        "{emoji}"
                    }
                }
            }
        }
    }
}

async fn send_reaction(room_coordinate: &str, emoji: &str) -> Result<(), String> {
    let _pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let tags = vec![
        nostr_sdk::Tag::custom(
            nostr_sdk::TagKind::custom("a"),
            [room_coordinate],
        ),
    ];
    let builder =
        nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Reaction, emoji).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("nest-reaction".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}
