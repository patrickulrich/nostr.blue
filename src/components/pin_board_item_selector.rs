//! Pin to Board Selector Component
//! Modal for selecting which board(s) to pin content to
//! Creates Kind 39067 pin events
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::pin_boards_store::{
    fetch_my_pinboards, publish_pin, PinContentType, PinInput, PinReference, Pinboard,
};
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
/// Props for PinToBoardModal
#[derive(Props, Clone, PartialEq)]
pub struct PinToBoardModalProps {
    /// Content type being pinned (for display)
    pub content_type: PinContentType,
    /// The pin reference (what to pin)
    pub reference: PinReference,
    /// Optional title for the pin
    #[props(default)]
    pub title: Option<String>,
    /// Close handler
    pub on_close: EventHandler<()>,
    /// Optional success callback
    #[props(default)]
    pub on_success: Option<EventHandler<()>>,
}
/// Modal for selecting which board to pin content to
#[component]
pub fn PinToBoardModal(props: PinToBoardModalProps) -> Element {
    let mut boards = use_signal(Vec::<Pinboard>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut success = use_signal(|| false);
    let mut selected_board_addrs = use_signal(Vec::<String>::new);
    let mut pin_note = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match fetch_my_pinboards().await {
                Ok(user_boards) => {
                    boards.set(user_boards);
                }
                Err(e) => {
                    log::error!("Failed to fetch boards: {}", e);
                    error_msg.set(Some(format!("Failed to load boards: {}", e)));
                }
            }
            loading.set(false);
        });
    });
    let content_type = props.content_type.clone();
    let reference = props.reference.clone();
    let title = props.title.clone();
    let on_close = props.on_close;
    let on_success = props.on_success;
    let handle_add = move |_| {
        let selected = selected_board_addrs.read().clone();
        let reference = reference.clone();
        let title = title.clone();
        let note = pin_note.read().clone();
        let _on_close = on_close;
        let on_success = on_success;
        saving.set(true);
        error_msg.set(None);
        spawn(async move {
            let input = PinInput {
                board_addresses: selected,
                reference,
                title,
                image: None,
                content: note,
                tags: vec![],
            };
            match publish_pin(input).await {
                Ok(_event_id) => {
                    log::info!("Successfully published pin");
                    success.set(true);
                    saving.set(false);
                    if let Some(handler) = on_success {
                        handler.call(());
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        on_close.call(());
                    }
                }
                Err(e) => {
                    log::error!("Failed to publish pin: {}", e);
                    error_msg.set(Some(format!("Failed to pin: {}", e)));
                    saving.set(false);
                }
            }
        });
    };
    let type_label = content_type.display_name().to_lowercase();
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full max-h-[80vh] overflow-hidden flex flex-col",
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.stop_propagation();
                        props.on_close.call(());
                    }
                },
                onclick: move |e| e.stop_propagation(),
                div { class: "flex justify-between items-center mb-4 shrink-0",
                    div {
                        h2 { class: "text-xl font-bold flex items-center gap-2",
                            span { "Pin to Board" }
                        }
                        p { class: "text-sm text-muted-foreground",
                            "Add this {type_label} to your collection"
                        }
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| props.on_close.call(()),
                        svg {
                            class: "w-5 h-5",
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
                if *success.read() {
                    div { class: "p-4 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600 text-center",
                        div { class: "text-2xl mb-2", "Done!" }
                        p { "Pinned successfully" }
                    }
                }
                if !*success.read() {
                    if !*HAS_SIGNER.read() {
                        div { class: "text-center py-8 text-muted-foreground",
                            p { "Please sign in to pin content." }
                        }
                    } else if *loading.read() {
                        div { class: "py-8 flex justify-center",
                            div { class: "w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        }
                    } else {
                        div { class: "overflow-y-auto flex-1 -mx-2 px-2",
                            div { class: "mb-4",
                                label { class: "block text-sm font-medium mb-1", "Add a note (optional)" }
                                textarea {
                                    class: "w-full px-3 py-2 border border-border rounded-lg bg-background text-sm resize-none focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    rows: "2",
                                    placeholder: "Why are you pinning this?",
                                    value: "{pin_note}",
                                    oninput: move |evt| pin_note.set(evt.value()),
                                }
                            }
                            if boards.read().is_empty() {
                                div { class: "text-center py-4",
                                    p { class: "text-muted-foreground mb-4",
                                        "You don't have any boards yet."
                                    }
                                    p { class: "text-sm text-muted-foreground",
                                        "Click \"Pin\" to save this to your profile, or create a board first."
                                    }
                                }
                            } else {
                                div { class: "mb-2",
                                    label { class: "block text-sm font-medium mb-2",
                                        "Select boards (optional)"
                                    }
                                    p { class: "text-xs text-muted-foreground mb-3",
                                        "Leave unselected to save as a profile pin"
                                    }
                                }
                                div { class: "space-y-2",
                                    for board in boards.read().iter() {
                                        BoardOption {
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
                        }
                        if let Some(err) = error_msg.read().as_ref() {
                            div { class: "mt-3 text-red-500 text-sm", "{err}" }
                        }
                        div { class: "mt-4 flex gap-2 justify-between items-center shrink-0",
                            Link {
                                to: Route::PinBoardNew {},
                                class: "text-sm text-primary hover:underline",
                                onclick: move |_| props.on_close.call(()),
                                "+ New Board"
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground",
                                    disabled: *saving.read(),
                                    onclick: move |_| props.on_close.call(()),
                                    "Cancel"
                                }
                                button {
                                    class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                                    disabled: *saving.read(),
                                    onclick: handle_add,
                                    if *saving.read() {
                                        span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                        "Pinning..."
                                    } else {
                                        if selected_board_addrs.read().is_empty() {
                                            "Pin to Profile"
                                        } else {
                                            "Pin"
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
/// Board option row component (checkbox style)
#[component]
fn BoardOption(
    board: Pinboard,
    is_selected: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let a_tag = board.a_tag.clone();
    rsx! {
        button {
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
                        div { class: "w-full h-full flex items-center justify-center text-muted-foreground",
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z",
                                }
                            }
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
/// Simple "Pin to Board" button that opens the modal
#[derive(Props, Clone, PartialEq)]
pub struct PinToBoardButtonProps {
    /// Content type being pinned
    pub content_type: PinContentType,
    /// The pin reference
    pub reference: PinReference,
    /// Optional title for the pin
    #[props(default)]
    pub title: Option<String>,
    /// Optional custom class
    #[props(default)]
    pub class: Option<String>,
    /// Optional custom label
    #[props(default)]
    pub label: Option<String>,
    /// Show as icon-only button
    #[props(default)]
    pub icon_only: bool,
}
#[component]
pub fn PinToBoardButton(props: PinToBoardButtonProps) -> Element {
    let mut show_modal = use_signal(|| false);
    if !*HAS_SIGNER.read() {
        return rsx! {};
    }
    let base_class = props
        .class
        .clone()
        .unwrap_or_else(|| {
            if props.icon_only {
                "p-2 rounded-lg hover:bg-muted transition text-muted-foreground hover:text-foreground"
                    .to_string()
            } else {
                "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg hover:bg-muted transition text-muted-foreground hover:text-foreground"
                    .to_string()
            }
        });
    rsx! {
        button {
            class: "{base_class}",
            onclick: move |_| show_modal.set(true),
            title: "Pin to Board",
            svg {
                class: if props.icon_only { "w-5 h-5" } else { "w-4 h-4" },
                fill: "none",
                stroke: "currentColor",
                view_box: "0 0 24 24",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 2L12 12" }
                circle { cx: "12", cy: "14", r: "2" }
                path { d: "M12 16L12 22" }
                path { d: "M6 6L6 8C6 10.2091 7.79086 12 10 12L14 12C16.2091 12 18 10.2091 18 8L18 6" }
            }
            if !props.icon_only {
                span { {props.label.clone().unwrap_or_else(|| "Pin".to_string())} }
            }
        }
        if *show_modal.read() {
            PinToBoardModal {
                content_type: props.content_type.clone(),
                reference: props.reference.clone(),
                title: props.title.clone(),
                on_close: move |_| show_modal.set(false),
            }
        }
    }
}
/// Compact pin button for content cards
#[component]
pub fn PinButton(
    content_type: PinContentType,
    reference: PinReference,
    #[props(default)]
    title: Option<String>,
) -> Element {
    rsx! {
        PinToBoardButton {
            content_type,
            reference,
            title,
            icon_only: true,
        }
    }
}
