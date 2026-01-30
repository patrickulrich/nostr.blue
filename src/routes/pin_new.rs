//! Create New Pin Page
//! Allows users to create pins by entering a URL, nostr event ID, or address
//! Features metadata fetch preview and collaborative board support
use crate::components::{ClientInitializing, MediaUploader};
use crate::routes::Route;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{
    self, PinContentType, PinInput, PinReference, Pinboard,
};
use crate::utils::nip73::ExternalContentId;
use crate::utils::pin_metadata::{self, PinPreviewMetadata};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
/// Fetch state for metadata preview
#[derive(Clone, Debug, PartialEq)]
enum FetchState {
    Idle,
    Fetching,
    Success,
    Error(String),
}
/// Detected input type from user's reference input
#[derive(Clone, Debug, PartialEq)]
enum DetectedInputType {
    Unknown,
    Url(String),
    EventId(String),
    Address(String),
    Isbn(String),
    Podcast(String),
    PodcastEpisode(String),
}
impl DetectedInputType {
    fn detect(input: &str) -> Self {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Self::Unknown;
        }
        if trimmed.starts_with("nevent1") || trimmed.starts_with("note1") {
            return Self::EventId(trimmed.to_string());
        }
        if trimmed.starts_with("naddr1") {
            return Self::Address(trimmed.to_string());
        }
        if trimmed.starts_with("isbn:") {
            return Self::Isbn(trimmed.to_string());
        }
        if trimmed.starts_with("podcast:item:guid:") {
            return Self::PodcastEpisode(trimmed.to_string());
        }
        if trimmed.starts_with("podcast:guid:") {
            return Self::Podcast(trimmed.to_string());
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Self::Url(trimmed.to_string());
        }
        if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Self::EventId(trimmed.to_string());
        }
        Self::Unknown
    }
    fn to_pin_reference(&self) -> Option<PinReference> {
        match self {
            Self::Unknown => None,
            Self::Url(url) => {
                if let Ok(parsed_url) = url.parse::<url::Url>() {
                    Some(PinReference::External {
                        content: ExternalContentId::Url(parsed_url),
                        hint: None,
                    })
                } else {
                    None
                }
            }
            Self::EventId(id) => {
                if id.starts_with("nevent1") || id.starts_with("note1") {
                    if let Ok(event_id) = EventId::from_bech32(id) {
                        return Some(PinReference::Event {
                            id: event_id.to_hex(),
                            relay_hint: None,
                        });
                    }
                    None
                } else {
                    Some(PinReference::Event {
                        id: id.clone(),
                        relay_hint: None,
                    })
                }
            }
            Self::Address(addr) => {
                if let Ok(coord) = Coordinate::from_bech32(addr) {
                    let coord_str = format!(
                        "{}:{}:{}",
                        coord.kind.as_u16(),
                        coord.public_key.to_hex(),
                        coord.identifier,
                    );
                    return Some(PinReference::Coordinate {
                        address: coord_str,
                        relay_hint: None,
                    });
                }
                None
            }
            Self::Isbn(isbn) => {
                let isbn_value = isbn.strip_prefix("isbn:").unwrap_or(isbn);
                Some(PinReference::External {
                    content: ExternalContentId::Book(isbn_value.to_string()),
                    hint: None,
                })
            }
            Self::Podcast(guid) => {
                let guid_value = guid.strip_prefix("podcast:guid:").unwrap_or(guid);
                Some(PinReference::External {
                    content: ExternalContentId::PodcastFeed(guid_value.to_string()),
                    hint: None,
                })
            }
            Self::PodcastEpisode(guid) => {
                let guid_value = guid.strip_prefix("podcast:item:guid:").unwrap_or(guid);
                Some(PinReference::External {
                    content: ExternalContentId::PodcastEpisode(guid_value.to_string()),
                    hint: None,
                })
            }
        }
    }
    fn description(&self) -> &'static str {
        match self {
            Self::Unknown => "Enter a URL, nostr event, or address",
            Self::Url(_) => "Web link detected",
            Self::EventId(_) => "Nostr event detected",
            Self::Address(_) => "Nostr address detected",
            Self::Isbn(_) => "Book (ISBN) detected",
            Self::Podcast(_) => "Podcast detected",
            Self::PodcastEpisode(_) => "Podcast episode detected",
        }
    }
    fn inferred_content_type(&self) -> Option<PinContentType> {
        match self {
            Self::Unknown => None,
            Self::Url(url) => {
                let lower = url.to_lowercase();
                if is_image_url(&lower) {
                    Some(PinContentType::Image)
                } else if is_video_url(&lower) {
                    Some(PinContentType::Video)
                } else {
                    Some(PinContentType::Link)
                }
            }
            Self::EventId(_) => Some(PinContentType::Note),
            Self::Address(addr) => {
                if let Ok(coord) = Coordinate::from_bech32(addr) {
                    Some(infer_content_type_from_kind(coord.kind.as_u16()))
                } else {
                    Some(PinContentType::Note)
                }
            }
            Self::Isbn(_) => Some(PinContentType::Book),
            Self::Podcast(_) | Self::PodcastEpisode(_) => Some(PinContentType::Podcast),
        }
    }
    /// Check if this type supports fetching preview metadata
    fn supports_fetch(&self) -> bool {
        matches!(self, Self::Url(_) | Self::EventId(_) | Self::Address(_))
    }
}
fn is_image_url(url: &str) -> bool {
    let extensions = [".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp"];
    extensions.iter().any(|ext| url.ends_with(ext))
}
fn is_video_url(url: &str) -> bool {
    let extensions = [".mp4", ".webm", ".mov", ".avi", ".mkv"];
    extensions.iter().any(|ext| url.ends_with(ext)) || url.contains("youtube.com")
        || url.contains("youtu.be") || url.contains("vimeo.com")
}
fn infer_content_type_from_kind(kind: u16) -> PinContentType {
    match kind {
        1 => PinContentType::Note,
        20 => PinContentType::Image,
        21 | 22 => PinContentType::Video,
        30023 => PinContentType::Article,
        30078 => PinContentType::Recipe,
        30067 => PinContentType::Pinboard,
        34550 => PinContentType::Community,
        30617 => PinContentType::CodeRepo,
        31922 | 31923 => PinContentType::CalendarEvent,
        30311 => PinContentType::LiveStream,
        32123 => PinContentType::Podcast,
        31337 | 32267 => PinContentType::Music,
        _ => PinContentType::Note,
    }
}
/// Validate and normalize a board address (naddr or coordinate format)
fn validate_board_address(input: &str) -> std::result::Result<String, String> {
    let trimmed = input.trim();
    if trimmed.starts_with("naddr1") {
        match Coordinate::from_bech32(trimmed) {
            Ok(coord) => {
                if coord.kind.as_u16() != 30067 {
                    return Err(format!("Not a pinboard (Kind {})", coord.kind.as_u16()));
                }
                Ok(trimmed.to_string())
            }
            Err(e) => Err(format!("Invalid naddr: {}", e)),
        }
    } else if trimmed.starts_with("30067:") {
        let parts: Vec<&str> = trimmed.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Err(
                "Invalid coordinate format (expected 30067:pubkey:d-tag)".to_string(),
            );
        }
        if parts[1].len() != 64 || !parts[1].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid pubkey in coordinate".to_string());
        }
        Ok(trimmed.to_string())
    } else {
        Err("Enter naddr1... or 30067:pubkey:d-tag format".to_string())
    }
}
#[component]
pub fn PinNew() -> Element {
    let navigator = use_navigator();
    let mut reference_input = use_signal(String::new);
    let mut title = use_signal(String::new);
    let mut comment = use_signal(String::new);
    let mut tags_input = use_signal(String::new);
    let mut fetch_state = use_signal(|| FetchState::Idle);
    let mut fetched_metadata = use_signal(PinPreviewMetadata::default);
    let mut custom_image_url = use_signal(|| None::<String>);
    let mut boards = use_signal(Vec::<Pinboard>::new);
    let mut boards_loading = use_signal(|| true);
    let mut selected_board_addrs = use_signal(Vec::<String>::new);
    let mut other_board_input = use_signal(String::new);
    let mut other_boards = use_signal(Vec::<String>::new);
    let mut other_board_error = use_signal(|| None::<String>);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let detected_type = use_memo(move || DetectedInputType::detect(
        &reference_input.read(),
    ));
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            match pin_boards_store::fetch_my_pinboards().await {
                Ok(user_boards) => {
                    boards.set(user_boards);
                }
                Err(e) => {
                    log::error!("Failed to fetch boards: {}", e);
                }
            }
            boards_loading.set(false);
        });
    });
    let handle_fetch = move |_| {
        let current_type = detected_type.read().clone();
        fetch_state.set(FetchState::Fetching);
        spawn(async move {
            let result = match current_type {
                DetectedInputType::Url(url) => {
                    pin_metadata::fetch_url_preview(&url).await
                }
                DetectedInputType::EventId(id) => {
                    pin_metadata::fetch_event_preview(&id).await
                }
                DetectedInputType::Address(addr) => {
                    pin_metadata::fetch_address_preview(&addr).await
                }
                _ => Err("Unsupported type for preview".to_string()),
            };
            match result {
                Ok(meta) => {
                    if title.read().is_empty() {
                        if let Some(ref t) = meta.title {
                            title.set(t.clone());
                        }
                    }
                    fetched_metadata.set(meta);
                    fetch_state.set(FetchState::Success);
                }
                Err(e) => {
                    fetch_state.set(FetchState::Error(e));
                }
            }
        });
    };
    let handle_add_other_board = move |_| {
        let input = other_board_input.read().clone();
        match validate_board_address(&input) {
            Ok(addr) => {
                let mut boards_list = other_boards.read().clone();
                if !boards_list.contains(&addr) {
                    boards_list.push(addr);
                    other_boards.set(boards_list);
                }
                other_board_input.set(String::new());
                other_board_error.set(None);
            }
            Err(e) => {
                other_board_error.set(Some(e));
            }
        }
    };
    let handle_submit = move |evt: dioxus::events::FormEvent| {
        evt.prevent_default();
        let current_type = detected_type.read().clone();
        let pin_reference = match current_type.to_pin_reference() {
            Some(r) => r,
            None => {
                error
                    .set(
                        Some(
                            "Please enter a valid URL, nostr event, or address"
                                .to_string(),
                        ),
                    );
                return;
            }
        };
        error.set(None);
        is_submitting.set(true);
        let tags: Vec<String> = tags_input
            .read()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let mut all_boards = selected_board_addrs.read().clone();
        all_boards.extend(other_boards.read().clone());
        let pin_title = if title.read().is_empty() {
            None
        } else {
            Some(title.read().clone())
        };
        let pin_comment = comment.read().clone();
        let pin_image = custom_image_url
            .read()
            .clone()
            .or_else(|| fetched_metadata.read().image.clone());
        let input = PinInput {
            board_addresses: all_boards,
            reference: pin_reference,
            title: pin_title,
            image: pin_image,
            content: pin_comment,
            tags,
        };
        spawn(async move {
            match pin_boards_store::publish_pin(input).await {
                Ok(_event_id) => {
                    log::info!("Successfully published pin");
                    navigator.push(Route::PinBoardsHome {});
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };
    let current_detected = detected_type.read().clone();
    let has_valid_reference = current_detected.to_pin_reference().is_some();
    let supports_fetch = current_detected.supports_fetch();
    let inferred_type = current_detected.inferred_content_type();
    let is_fetching = matches!(*fetch_state.read(), FetchState::Fetching);
    let fetch_succeeded = matches!(*fetch_state.read(), FetchState::Success);
    let total_boards_selected = selected_board_addrs.read().len()
        + other_boards.read().len();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::PinBoardsHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
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
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-2xl", "+" }
                        "New Pin"
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !*HAS_SIGNER.read() {
                div { class: "p-4",
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "+" }
                        h2 { class: "text-xl font-semibold mb-2", "Sign In Required" }
                        p { class: "text-muted-foreground mb-4",
                            "You need to sign in to create a pin"
                        }
                    }
                }
            } else {
                div { class: "p-4 max-w-2xl mx-auto",
                    form { onsubmit: handle_submit, class: "space-y-6",
                        if let Some(ref err) = *error.read() {
                            div { class: "p-4 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg",
                                "{err}"
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "What do you want to pin? "
                                span { class: "text-red-500", "*" }
                            }
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "https://... or nevent1... or naddr1...",
                                    value: "{reference_input}",
                                    oninput: move |evt| {
                                        reference_input.set(evt.value());
                                        fetch_state.set(FetchState::Idle);
                                        fetched_metadata.set(PinPreviewMetadata::default());
                                    },
                                    required: true,
                                }
                                button {
                                    r#type: "button",
                                    class: "px-4 py-2 bg-secondary hover:bg-secondary/80 text-secondary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2 shrink-0",
                                    disabled: !has_valid_reference || !supports_fetch || is_fetching,
                                    onclick: handle_fetch,
                                    if is_fetching {
                                        span { class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                    } else {
                                        svg {
                                            class: "w-4 h-4",
                                            fill: "none",
                                            stroke: "currentColor",
                                            view_box: "0 0 24 24",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                stroke_width: "2",
                                                d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15",
                                            }
                                        }
                                    }
                                    "Fetch"
                                }
                            }
                            div { class: "mt-2 flex items-center gap-2 text-sm",
                                if has_valid_reference {
                                    span { class: "text-green-600 dark:text-green-400 flex items-center gap-1",
                                        svg {
                                            class: "w-4 h-4",
                                            fill: "none",
                                            stroke: "currentColor",
                                            view_box: "0 0 24 24",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                stroke_width: "2",
                                                d: "M5 13l4 4L19 7",
                                            }
                                        }
                                        "{current_detected.description()}"
                                    }
                                    if let Some(ref ct) = inferred_type {
                                        span { class: "px-2 py-0.5 bg-muted rounded text-xs",
                                            "{ct.display_name()}"
                                        }
                                    }
                                } else if !reference_input.read().is_empty() {
                                    span { class: "text-muted-foreground",
                                        "Enter a valid URL, nostr event (nevent/note), or address (naddr)"
                                    }
                                }
                            }
                        }
                        if fetch_succeeded {
                            div { class: "p-4 bg-muted/50 rounded-lg border border-border",
                                div { class: "flex items-center gap-2 mb-3",
                                    span { class: "text-sm font-medium text-muted-foreground",
                                        "Preview"
                                    }
                                    if !fetched_metadata.read().source_type.is_empty() {
                                        span { class: "px-2 py-0.5 bg-primary/10 text-primary text-xs rounded",
                                            "{fetched_metadata.read().source_type}"
                                        }
                                    }
                                }
                                div { class: "flex gap-4",
                                    if let Some(ref img) = fetched_metadata.read().image {
                                        div { class: "w-32 h-24 rounded-lg overflow-hidden bg-muted shrink-0",
                                            img {
                                                src: "{img}",
                                                alt: "Preview",
                                                class: "w-full h-full object-cover",
                                                loading: "lazy",
                                            }
                                        }
                                    }
                                    div { class: "flex-1 min-w-0",
                                        if let Some(ref meta_title) = fetched_metadata.read().title {
                                            h4 { class: "font-medium text-sm line-clamp-2",
                                                "{meta_title}"
                                            }
                                        }
                                        if let Some(ref desc) = fetched_metadata.read().description {
                                            p { class: "text-xs text-muted-foreground line-clamp-3 mt-1",
                                                "{desc}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let FetchState::Error(ref err) = *fetch_state.read() {
                            div { class: "p-3 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm",
                                "Failed to fetch preview: {err}"
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Pin Image "
                                span { class: "text-muted-foreground font-normal", "(optional)" }
                            }
                            {
                                let display_image = custom_image_url
                                    .read()
                                    .clone()
                                    .or_else(|| fetched_metadata.read().image.clone());
                                rsx! {
                                    if let Some(ref img_url) = display_image {
                                        div { class: "relative w-full h-48 rounded-lg overflow-hidden mb-2 bg-muted",
                                            img {
                                                src: "{img_url}",
                                                alt: "Pin image",
                                                class: "w-full h-full object-cover",
                                            }
                                            div { class: "absolute top-2 right-2 flex gap-2",
                                                if custom_image_url.read().is_some() {
                                                    span { class: "px-2 py-1 bg-black/60 text-white text-xs rounded", "Custom" }
                                                } else {
                                                    span { class: "px-2 py-1 bg-black/60 text-white text-xs rounded", "Auto-fetched" }
                                                }
                                                button {
                                                    r#type: "button",
                                                    class: "p-1.5 rounded-full bg-red-500 text-white hover:bg-red-600 transition",
                                                    onclick: move |_| {
                                                        custom_image_url.set(None);
                                                        let mut meta = fetched_metadata.read().clone();
                                                        meta.image = None;
                                                        fetched_metadata.set(meta);
                                                    },
                                                    svg {
                                                        class: "w-4 h-4",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        view_box: "0 0 24 24",
                                                        path {
                                                            stroke_linecap: "round",
                                                            stroke_linejoin: "round",
                                                            stroke_width: "2",
                                                            d: "M6 18L18 6M6 6l12 12",
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        div { class: "p-4 border-2 border-dashed border-border rounded-lg text-center text-muted-foreground mb-2",
                                            p { class: "text-sm", "No image detected" }
                                            p { class: "text-xs", "Upload an image to make your pin stand out" }
                                        }
                                    }
                                }
                            }
                            MediaUploader {
                                on_upload: move |url: String| custom_image_url.set(Some(url)),
                                button_label: if custom_image_url.read().is_some() || fetched_metadata.read().image.is_some() { "Replace image".to_string() } else { "Upload image".to_string() },
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Title "
                                if fetch_succeeded && fetched_metadata.read().title.is_some() {
                                    span { class: "text-muted-foreground font-normal",
                                        "(auto-filled)"
                                    }
                                } else {
                                    span { class: "text-muted-foreground font-normal",
                                        "(optional)"
                                    }
                                }
                            }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Give your pin a title",
                                value: "{title}",
                                oninput: move |evt| title.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Comment (optional)" }
                            textarea {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                                rows: "3",
                                placeholder: "Why are you pinning this?",
                                value: "{comment}",
                                oninput: move |evt| comment.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Tags (optional)" }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "travel, favorites, inspiration (comma separated)",
                                value: "{tags_input}",
                                oninput: move |evt| tags_input.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Pin to boards (optional)" }
                            p { class: "text-xs text-muted-foreground mb-3",
                                "Leave unselected to save as a profile pin visible on your profile"
                            }
                            if *boards_loading.read() {
                                div { class: "py-4 flex justify-center",
                                    div { class: "w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                                }
                            } else {
                                div { class: "space-y-2",
                                    if !boards.read().is_empty() {
                                        div { class: "space-y-2 max-h-60 overflow-y-auto",
                                            for board in boards.read().iter() {
                                                BoardCheckbox {
                                                    key: "{board.d_tag}",
                                                    board: board.clone(),
                                                    is_selected: selected_board_addrs.read().contains(&board.a_tag),
                                                    on_toggle: move |a_tag: String| {
                                                        let mut addrs = selected_board_addrs.read().clone();
                                                        if addrs.contains(&a_tag) {
                                                            addrs.retain(|a| a != &a_tag);
                                                        } else {
                                                            addrs.push(a_tag);
                                                        }
                                                        selected_board_addrs.set(addrs);
                                                    },
                                                }
                                            }
                                        }
                                    }
                                    for (idx , board_addr) in other_boards.read().iter().enumerate() {
                                        {
                                            let board_addr_clone = board_addr.clone();
                                            rsx! {
                                                div {
                                                    key: "other-{idx}",
                                                    class: "flex items-center gap-2 p-3 bg-primary/5 border-2 border-primary rounded-lg",
                                                    div { class: "w-5 h-5 rounded bg-primary flex items-center justify-center shrink-0",
                                                        svg {
                                                            class: "w-3 h-3 text-primary-foreground",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            view_box: "0 0 24 24",
                                                            path {
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                stroke_width: "3",
                                                                d: "M5 13l4 4L19 7",
                                                            }
                                                        }
                                                    }
                                                    span { class: "flex-1 text-xs font-mono truncate", "{board_addr_clone}" }
                                                    button {
                                                        r#type: "button",
                                                        class: "p-1 text-muted-foreground hover:text-red-500 transition",
                                                        onclick: move |_| {
                                                            let mut boards_list = other_boards.read().clone();
                                                            boards_list.retain(|b| b != &board_addr_clone);
                                                            other_boards.set(boards_list);
                                                        },
                                                        svg {
                                                            class: "w-4 h-4",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            view_box: "0 0 24 24",
                                                            path {
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                stroke_width: "2",
                                                                d: "M6 18L18 6M6 6l12 12",
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div { class: "flex gap-2 pt-2",
                                        input {
                                            class: "flex-1 px-3 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary font-mono text-sm",
                                            r#type: "text",
                                            placeholder: "naddr1... or 30067:pubkey:d-tag",
                                            value: "{other_board_input}",
                                            oninput: move |evt| {
                                                other_board_input.set(evt.value());
                                                other_board_error.set(None);
                                            },
                                        }
                                        button {
                                            r#type: "button",
                                            class: "px-4 py-2 bg-secondary hover:bg-secondary/80 text-secondary-foreground rounded-lg font-medium transition",
                                            onclick: handle_add_other_board,
                                            "Add"
                                        }
                                    }
                                    if let Some(ref err) = *other_board_error.read() {
                                        p { class: "text-xs text-red-500", "{err}" }
                                    }
                                    if boards.read().is_empty() && other_boards.read().is_empty() {
                                        div { class: "text-center py-2",
                                            Link {
                                                to: Route::PinBoardNew {},
                                                class: "text-primary hover:underline text-sm",
                                                "Create a board"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "pt-4",
                            button {
                                r#type: "submit",
                                class: "w-full px-6 py-3 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_submitting.read() || !has_valid_reference,
                                if *is_submitting.read() {
                                    span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                    "Creating Pin..."
                                } else {
                                    if total_boards_selected == 0 {
                                        "Create Profile Pin"
                                    } else {
                                        "Create Pin"
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
/// Board checkbox component
#[component]
fn BoardCheckbox(
    board: Pinboard,
    is_selected: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let a_tag = board.a_tag.clone();
    rsx! {
        button {
            r#type: "button",
            class: if is_selected { "w-full p-3 rounded-lg border-2 border-primary bg-primary/5 text-left transition" } else { "w-full p-3 rounded-lg border border-border hover:border-primary/50 hover:bg-muted/50 text-left transition" },
            onclick: move |_| on_toggle.call(a_tag.clone()),
            div { class: "flex items-center gap-3",
                div { class: if is_selected { "w-5 h-5 rounded border-2 border-primary bg-primary flex items-center justify-center shrink-0" } else { "w-5 h-5 rounded border-2 border-muted-foreground shrink-0" },
                    if is_selected {
                        svg {
                            class: "w-3 h-3 text-primary-foreground",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "3",
                                d: "M5 13l4 4L19 7",
                            }
                        }
                    }
                }
                div { class: "w-10 h-10 rounded-lg overflow-hidden bg-muted shrink-0",
                    if let Some(ref img) = board.image {
                        img {
                            src: "{img}",
                            alt: "{board.title}",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-muted-foreground text-lg",
                            "+"
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium truncate", "{board.title}" }
                    if !board.tags.is_empty() {
                        div { class: "text-xs text-muted-foreground flex items-center gap-1 mt-0.5",
                            for tag in board.tags.iter().take(2) {
                                span { "#{tag}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
