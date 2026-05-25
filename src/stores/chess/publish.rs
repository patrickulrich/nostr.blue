use std::collections::HashMap;

use nostr_sdk::{EventBuilder, EventId, Kind, PublicKey, Tag};

use super::jester::JesterContent;
use super::types::ChessColor;
use crate::stores::publish_queue;
use crate::stores::publish_queue::types::QueueEventType;
use crate::utils::nips::chess::{KIND_CHESS_PGN, KIND_JESTER, JESTER_START_POSITION_HASH};

pub async fn publish_challenge(
    color: ChessColor,
    opponent: Option<PublicKey>,
) -> Result<EventId, String> {
    let rs_color = match color {
        ChessColor::White => rschess::Color::White,
        ChessColor::Black => rschess::Color::Black,
    };
    let content = JesterContent::new_start(rs_color);
    let e_tag_value = match opponent {
        Some(ref pk) => crate::utils::nips::chess::jester_private_start_ref(&pk.to_hex()),
        None => JESTER_START_POSITION_HASH.to_string(),
    };
    let mut tags = vec![Tag::custom(
        nostr_sdk::TagKind::e(),
        vec![e_tag_value],
    )];
    if let Some(ref pk) = opponent {
        tags.push(Tag::custom(
            nostr_sdk::TagKind::e(),
            vec![JESTER_START_POSITION_HASH.to_string()],
        ));
        tags.push(Tag::public_key(*pk));
    }
    let color_label = match rs_color {
        rschess::Color::White => "White",
        rschess::Color::Black => "Black",
    };
    let alt = if opponent.is_some() {
        format!("Chess challenge (plays {})", color_label)
    } else {
        format!("Open chess challenge (plays {})", color_label)
    };
    tags.push(Tag::alt(alt));

    let builder = EventBuilder::new(Kind::Custom(KIND_JESTER), content.to_json()).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let event_id = event.id;
    let _ = publish_queue::enqueue(
        event,
        QueueEventType::Other("ChessChallenge".to_string()),
        Some(super::chess_config::chess_relay_urls()),
        HashMap::new(),
    )
    .await;
    Ok(event_id)
}

pub async fn publish_move(
    start_event_id: &EventId,
    head_event_id: &EventId,
    opponent: &PublicKey,
    content: JesterContent,
) -> Result<EventId, String> {
    let tags = vec![
        Tag::custom(nostr_sdk::TagKind::e(), vec![start_event_id.to_hex()]),
        Tag::custom(nostr_sdk::TagKind::e(), vec![head_event_id.to_hex()]),
        Tag::public_key(*opponent),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_JESTER), content.to_json()).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let event_id = event.id;
    let _ = publish_queue::enqueue(
        event,
        QueueEventType::Other("ChessMove".to_string()),
        Some(super::chess_config::chess_relay_urls()),
        HashMap::new(),
    )
    .await;
    Ok(event_id)
}

pub async fn publish_game_end(
    start_event_id: &EventId,
    head_event_id: &EventId,
    opponent: &PublicKey,
    content: JesterContent,
) -> Result<EventId, String> {
    let tags = vec![
        Tag::custom(nostr_sdk::TagKind::e(), vec![start_event_id.to_hex()]),
        Tag::custom(nostr_sdk::TagKind::e(), vec![head_event_id.to_hex()]),
        Tag::public_key(*opponent),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_JESTER), content.to_json()).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let event_id = event.id;
    let _ = publish_queue::enqueue(
        event,
        QueueEventType::Other("ChessEnd".to_string()),
        Some(super::chess_config::chess_relay_urls()),
        HashMap::new(),
    )
    .await;
    Ok(event_id)
}

pub async fn publish_pgn_game(
    pgn_content: String,
    opponent: Option<PublicKey>,
    alt: String,
    source_game_id: Option<EventId>,
) -> Result<EventId, String> {
    let mut tags = vec![Tag::alt(alt)];
    if let Some(pk) = opponent {
        tags.push(Tag::public_key(pk));
    }
    if let Some(game_id) = source_game_id {
        tags.push(Tag::event(game_id));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_CHESS_PGN), pgn_content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let event_id = event.id;
    let _ = publish_queue::enqueue(
        event,
        QueueEventType::Other("ChessPGN".to_string()),
        Some(super::chess_config::chess_relay_urls()),
        HashMap::new(),
    )
    .await;
    Ok(event_id)
}
