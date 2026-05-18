use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct NestReactionsProps {
    pub room_coordinate: String,
    pub is_joined: bool,
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
                                    let mut floats_mut = floats.write();
                                    floats_mut.push((emoji_for_float, 0.0, id));
                                },
                                "{emoji}"
                            }
                        }
                    }
                }
            }
            div { class: "absolute bottom-full left-0 right-0 pointer-events-none overflow-hidden h-20",
                for (emoji, offset, id) in floating_reactions.read().iter() {
                    div {
                        key: "{id}",
                        class: "absolute animate-float-up text-2xl",
                        style: "left: {offset}%; bottom: 0px;",
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
