//! Simple Modal Component
//!
//! A reliable modal overlay component with proper fixed positioning.
//! Does not rely on external dialog primitives.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    /// Whether the modal is open
    pub open: Signal<bool>,
    /// Optional callback when modal closes
    #[props(default)]
    pub on_close: Option<EventHandler<()>>,
    /// Modal content
    pub children: Element,
}

/// A simple modal overlay with centered content
#[component]
pub fn Modal(props: ModalProps) -> Element {
    let mut open = props.open;

    // Don't render anything if not open
    if !*open.read() {
        return rsx! {};
    }

    let handle_backdrop_click = move |_| {
        open.set(false);
        if let Some(on_close) = &props.on_close {
            on_close.call(());
        }
    };

    let handle_content_click = move |e: MouseEvent| {
        // Prevent clicks inside the modal from closing it
        e.stop_propagation();
    };

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm",
            onclick: handle_backdrop_click,

            // Modal content container
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-h-[90vh] overflow-auto",
                onclick: handle_content_click,
                {props.children}
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ModalHeaderProps {
    /// Modal title
    pub title: String,
    /// Close button handler
    pub on_close: EventHandler<()>,
}

/// Modal header with title and close button
#[component]
pub fn ModalHeader(props: ModalHeaderProps) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-between p-4 border-b border-border",

            h2 {
                class: "text-lg font-bold",
                "{props.title}"
            }

            button {
                class: "p-2 rounded-full hover:bg-accent text-muted-foreground hover:text-foreground transition",
                onclick: move |_| props.on_close.call(()),
                svg {
                    class: "w-5 h-5",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M6 18L18 6M6 6l12 12"
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ModalBodyProps {
    pub children: Element,
}

/// Modal body content area
#[component]
pub fn ModalBody(props: ModalBodyProps) -> Element {
    rsx! {
        div {
            class: "p-4",
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ModalFooterProps {
    pub children: Element,
}

/// Modal footer for actions
#[component]
pub fn ModalFooter(props: ModalFooterProps) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-end gap-3 p-4 border-t border-border",
            {props.children}
        }
    }
}
