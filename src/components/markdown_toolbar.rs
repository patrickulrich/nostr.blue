//! Markdown Formatting Toolbar Component
//!
//! A toolbar with buttons for common markdown formatting operations.
//! Uses dioxus-primitives Toolbar for accessibility (keyboard navigation, roving focus).
use dioxus::prelude::*;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};
/// Type of markdown formatting to apply
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkdownFormat {
    /// Bold text: **text**
    Bold,
    /// Italic text: *text*
    Italic,
    /// Strikethrough text: ~~text~~
    Strikethrough,
    /// Heading level 1: #
    Heading1,
    /// Heading level 2: ##
    Heading2,
    /// Heading level 3: ###
    Heading3,
    /// Unordered list: -
    BulletList,
    /// Ordered list: 1.
    NumberedList,
    /// Quote: >
    Quote,
    /// Inline code: `code`
    InlineCode,
    /// Code block: ```\ncode\n```
    CodeBlock,
    /// Link: [text](url)
    Link,
    /// Image: ![alt](url)
    Image,
    /// Horizontal rule: ---
    HorizontalRule,
}
#[cfg_attr(not(any(feature = "web", test)), allow(dead_code))]
impl MarkdownFormat {
    /// Get the markdown prefix to insert
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Strikethrough => "~~",
            Self::Heading1 => "# ",
            Self::Heading2 => "## ",
            Self::Heading3 => "### ",
            Self::BulletList => "- ",
            Self::NumberedList => "1. ",
            Self::Quote => "> ",
            Self::InlineCode => "`",
            Self::CodeBlock => "```\n",
            Self::Link => "[",
            Self::Image => "![",
            Self::HorizontalRule => "\n---\n",
        }
    }
    /// Get the markdown suffix to insert
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Strikethrough => "~~",
            Self::Heading1 | Self::Heading2 | Self::Heading3 => "",
            Self::BulletList | Self::NumberedList | Self::Quote => "",
            Self::InlineCode => "`",
            Self::CodeBlock => "\n```",
            Self::Link => "](url)",
            Self::Image => "](image-url)",
            Self::HorizontalRule => "",
        }
    }
    /// Get placeholder text for empty selection
    pub fn placeholder(&self) -> &'static str {
        match self {
            Self::Bold | Self::Italic | Self::Strikethrough => "text",
            Self::Heading1 | Self::Heading2 | Self::Heading3 => "heading",
            Self::BulletList | Self::NumberedList => "item",
            Self::Quote => "quote",
            Self::InlineCode | Self::CodeBlock => "code",
            Self::Link => "link text",
            Self::Image => "alt text",
            Self::HorizontalRule => "",
        }
    }
    /// Whether this format wraps selection vs inserts at cursor
    pub fn wraps_selection(&self) -> bool {
        matches!(
            self,
            Self::Bold
                | Self::Italic
                | Self::Strikethrough
                | Self::InlineCode
                | Self::CodeBlock
                | Self::Link
                | Self::Image
        )
    }
    /// Whether this format should be inserted at line start
    pub fn is_line_prefix(&self) -> bool {
        matches!(
            self,
            Self::Heading1
                | Self::Heading2
                | Self::Heading3
                | Self::BulletList
                | Self::NumberedList
                | Self::Quote
        )
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct MarkdownToolbarProps {
    /// Callback when a format button is clicked
    pub on_format: EventHandler<MarkdownFormat>,
    /// Callback to request opening the image upload dialog
    #[props(default)]
    pub on_image_upload_request: Option<EventHandler<()>>,
    /// Callback to request opening the Nostr mention dialog
    #[props(default)]
    pub on_mention_request: Option<EventHandler<()>>,
    /// Whether the toolbar is disabled
    #[props(default = false)]
    pub disabled: bool,
    /// Additional CSS classes for the toolbar container
    #[props(default)]
    pub class: String,
}
#[component]
pub fn MarkdownToolbar(props: MarkdownToolbarProps) -> Element {
    let mut disabled = use_signal(|| props.disabled);
    use_effect(move || {
        disabled.set(props.disabled);
    });
    let on_format = props.on_format;
    rsx! {
        Toolbar {
            aria_label: "Markdown formatting",
            disabled: ReadSignal::from(disabled),
            class: "flex flex-wrap items-center gap-1 p-2 border-b border-border bg-muted/30 rounded-t-lg {props.class}",
            ToolbarButton {
                index: 0usize,
                on_click: move |_| on_format.call(MarkdownFormat::Bold),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Bold (Ctrl+B)",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2.5",
                    view_box: "0 0 24 24",
                    path { d: "M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" }
                    path { d: "M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" }
                }
            }
            ToolbarButton {
                index: 1usize,
                on_click: move |_| on_format.call(MarkdownFormat::Italic),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Italic (Ctrl+I)",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    line {
                        x1: "19",
                        y1: "4",
                        x2: "10",
                        y2: "4",
                    }
                    line {
                        x1: "14",
                        y1: "20",
                        x2: "5",
                        y2: "20",
                    }
                    line {
                        x1: "15",
                        y1: "4",
                        x2: "9",
                        y2: "20",
                    }
                }
            }
            ToolbarButton {
                index: 2usize,
                on_click: move |_| on_format.call(MarkdownFormat::Strikethrough),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Strikethrough",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path { d: "M16 4H9a3 3 0 00-3 3v.5" }
                    path { d: "M4 12h16" }
                    path { d: "M9 20h6a3 3 0 003-3v-.5" }
                }
            }
            ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
            ToolbarButton {
                index: 3usize,
                on_click: move |_| on_format.call(MarkdownFormat::Heading1),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Heading 1",
                span { class: "font-bold text-sm", "H1" }
            }
            ToolbarButton {
                index: 4usize,
                on_click: move |_| on_format.call(MarkdownFormat::Heading2),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Heading 2",
                span { class: "font-bold text-sm", "H2" }
            }
            ToolbarButton {
                index: 5usize,
                on_click: move |_| on_format.call(MarkdownFormat::Heading3),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Heading 3",
                span { class: "font-bold text-sm", "H3" }
            }
            ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
            ToolbarButton {
                index: 6usize,
                on_click: move |_| on_format.call(MarkdownFormat::BulletList),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Bullet list",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    line {
                        x1: "8",
                        y1: "6",
                        x2: "21",
                        y2: "6",
                    }
                    line {
                        x1: "8",
                        y1: "12",
                        x2: "21",
                        y2: "12",
                    }
                    line {
                        x1: "8",
                        y1: "18",
                        x2: "21",
                        y2: "18",
                    }
                    circle {
                        cx: "3",
                        cy: "6",
                        r: "1.5",
                        fill: "currentColor",
                    }
                    circle {
                        cx: "3",
                        cy: "12",
                        r: "1.5",
                        fill: "currentColor",
                    }
                    circle {
                        cx: "3",
                        cy: "18",
                        r: "1.5",
                        fill: "currentColor",
                    }
                }
            }
            ToolbarButton {
                index: 7usize,
                on_click: move |_| on_format.call(MarkdownFormat::NumberedList),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Numbered list",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    line {
                        x1: "10",
                        y1: "6",
                        x2: "21",
                        y2: "6",
                    }
                    line {
                        x1: "10",
                        y1: "12",
                        x2: "21",
                        y2: "12",
                    }
                    line {
                        x1: "10",
                        y1: "18",
                        x2: "21",
                        y2: "18",
                    }
                    text {
                        x: "3",
                        y: "7",
                        font_size: "7",
                        fill: "currentColor",
                        "1"
                    }
                    text {
                        x: "3",
                        y: "13",
                        font_size: "7",
                        fill: "currentColor",
                        "2"
                    }
                    text {
                        x: "3",
                        y: "19",
                        font_size: "7",
                        fill: "currentColor",
                        "3"
                    }
                }
            }
            ToolbarButton {
                index: 8usize,
                on_click: move |_| on_format.call(MarkdownFormat::Quote),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Quote",
                svg {
                    class: "w-4 h-4",
                    fill: "currentColor",
                    view_box: "0 0 24 24",
                    path { d: "M6 17h3l2-4V7H5v6h3zm8 0h3l2-4V7h-6v6h3z" }
                }
            }
            ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
            ToolbarButton {
                index: 9usize,
                on_click: move |_| on_format.call(MarkdownFormat::InlineCode),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Inline code",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    polyline { points: "16 18 22 12 16 6" }
                    polyline { points: "8 6 2 12 8 18" }
                }
            }
            ToolbarButton {
                index: 10usize,
                on_click: move |_| on_format.call(MarkdownFormat::CodeBlock),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Code block",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    rect {
                        x: "3",
                        y: "3",
                        width: "18",
                        height: "18",
                        rx: "2",
                    }
                    polyline { points: "8 10 6 12 8 14" }
                    polyline { points: "16 10 18 12 16 14" }
                    line {
                        x1: "12",
                        y1: "8",
                        x2: "12",
                        y2: "16",
                    }
                }
            }
            ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
            ToolbarButton {
                index: 11usize,
                on_click: move |_| on_format.call(MarkdownFormat::Link),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Insert link",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                    path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                }
            }
            if let Some(on_upload) = &props.on_image_upload_request {
                {
                    let on_upload = *on_upload;
                    rsx! {
                        ToolbarButton {
                            index: 12usize,
                            on_click: move |_| on_upload.call(()),
                            class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                            title: "Upload image",
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                rect {
                                    x: "3",
                                    y: "3",
                                    width: "18",
                                    height: "18",
                                    rx: "2",
                                    ry: "2",
                                }
                                circle { cx: "8.5", cy: "8.5", r: "1.5" }
                                polyline { points: "21 15 16 10 5 21" }
                            }
                        }
                    }
                }
            } else {
                ToolbarButton {
                    index: 12usize,
                    on_click: move |_| on_format.call(MarkdownFormat::Image),
                    class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                    title: "Insert image",
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        rect {
                            x: "3",
                            y: "3",
                            width: "18",
                            height: "18",
                            rx: "2",
                            ry: "2",
                        }
                        circle { cx: "8.5", cy: "8.5", r: "1.5" }
                        polyline { points: "21 15 16 10 5 21" }
                    }
                }
            }
            if let Some(on_mention) = &props.on_mention_request {
                {
                    let on_mention = *on_mention;
                    rsx! {
                        ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
                        ToolbarButton {
                            index: 13usize,
                            on_click: move |_| on_mention.call(()),
                            class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                            title: "Insert Nostr mention",
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                circle { cx: "12", cy: "12", r: "4" }
                                path { d: "M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94" }
                            }
                        }
                    }
                }
            }
            ToolbarSeparator { class: "w-px h-6 bg-border mx-1" }
            ToolbarButton {
                index: 14usize,
                on_click: move |_| on_format.call(MarkdownFormat::HorizontalRule),
                class: "p-2 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition",
                title: "Horizontal rule",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    line {
                        x1: "3",
                        y1: "12",
                        x2: "21",
                        y2: "12",
                    }
                }
            }
        }
    }
}
/// Apply markdown formatting to content at the specified cursor position
///
/// # Arguments
/// * `content` - The current content
/// * `cursor_start` - Selection start position (UTF-8 byte index)
/// * `cursor_end` - Selection end position (UTF-8 byte index)
/// * `format` - The format to apply
///
/// # Returns
/// Tuple of (new content, new cursor position)
#[cfg_attr(not(any(feature = "web", test)), allow(dead_code))]
pub fn apply_markdown_format(
    content: &str,
    cursor_start: usize,
    cursor_end: usize,
    format: MarkdownFormat,
) -> (String, usize) {
    let cursor_start = cursor_start.min(content.len());
    let cursor_end = cursor_end.min(content.len());
    let (start, end) = if cursor_start <= cursor_end {
        (cursor_start, cursor_end)
    } else {
        (cursor_end, cursor_start)
    };
    let prefix = format.prefix();
    let suffix = format.suffix();
    let placeholder = format.placeholder();
    if format.is_line_prefix() {
        let line_start = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let before = &content[..line_start];
        let after = &content[line_start..];
        let new_content = format!("{}{}{}", before, prefix, after);
        let new_cursor = start + prefix.len();
        (new_content, new_cursor)
    } else if format.wraps_selection() {
        let before = &content[..start];
        let selected = if start == end {
            placeholder.to_string()
        } else {
            content[start..end].to_string()
        };
        let after = &content[end..];
        let new_content = format!("{}{}{}{}{}", before, prefix, selected, suffix, after);
        let new_cursor = match format {
            MarkdownFormat::Link | MarkdownFormat::Image => {
                start + prefix.len() + selected.len() + 2
            }
            _ => start + prefix.len() + selected.len() + suffix.len(),
        };
        (new_content, new_cursor)
    } else {
        let before = &content[..start];
        let after = &content[start..];
        let new_content = format!("{}{}{}", before, prefix, after);
        let new_cursor = start + prefix.len();
        (new_content, new_cursor)
    }
}
/// Set cursor position in a textarea element (WASM only)
#[cfg(feature = "web")]
pub fn set_textarea_cursor(textarea_id: &str, position: usize, content: &str) {
    use wasm_bindgen::JsCast;
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(textarea_id) {
                if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                    let utf16_pos = utf8_to_utf16_index(content, position) as u32;
                    let _ = textarea.set_selection_range(utf16_pos, utf16_pos);
                    let _ = textarea.focus();
                }
            }
        }
    }
}
#[cfg(not(feature = "web"))]
#[allow(dead_code)]
pub fn set_textarea_cursor(_textarea_id: &str, _position: usize, _content: &str) {}
/// Get the current UTF-8 cursor/selection range for a textarea when the platform exposes it.
#[cfg(feature = "web")]
pub fn get_textarea_cursor(textarea_id: &str, content: &str) -> Option<(usize, usize)> {
    use wasm_bindgen::JsCast;
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(textarea_id) {
                if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                    let start = textarea.selection_start().ok().flatten()? as usize;
                    let end = textarea.selection_end().ok().flatten()? as usize;
                    let start_utf8 = utf16_to_utf8_index(content, start);
                    let end_utf8 = utf16_to_utf8_index(content, end);
                    return Some((start_utf8, end_utf8));
                }
            }
        }
    }
    None
}
#[cfg(not(feature = "web"))]
pub fn get_textarea_cursor(_textarea_id: &str, _content: &str) -> Option<(usize, usize)> {
    None
}
/// Convert UTF-16 code unit index (from DOM) to UTF-8 byte index (for Rust string slicing)
#[cfg(feature = "web")]
fn utf16_to_utf8_index(text: &str, utf16_index: usize) -> usize {
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;
    for ch in text.chars() {
        if utf16_count >= utf16_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }
    utf8_byte_index.min(text.len())
}
/// Convert UTF-8 byte index (from Rust string) to UTF-16 code unit index (for DOM)
#[cfg(feature = "web")]
fn utf8_to_utf16_index(text: &str, utf8_index: usize) -> usize {
    let utf8_index = utf8_index.min(text.len());
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;
    for ch in text.chars() {
        if utf8_byte_index >= utf8_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }
    utf16_count
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bold_format_empty_selection() {
        let content = "hello world";
        let (result, cursor) = apply_markdown_format(content, 6, 6, MarkdownFormat::Bold);
        assert_eq!(result, "hello **text**world");
        assert_eq!(cursor, 14);
    }
    #[test]
    fn test_bold_format_with_selection() {
        let content = "hello world";
        let (result, cursor) = apply_markdown_format(content, 0, 5, MarkdownFormat::Bold);
        assert_eq!(result, "**hello** world");
        assert_eq!(cursor, 9);
    }
    #[test]
    fn test_heading_format() {
        let content = "hello world";
        let (result, cursor) = apply_markdown_format(content, 6, 6, MarkdownFormat::Heading1);
        assert_eq!(result, "# hello world");
        assert_eq!(cursor, 8);
    }
    #[test]
    fn test_heading_format_middle_of_line() {
        let content = "line one\nline two";
        let (result, cursor) = apply_markdown_format(content, 12, 12, MarkdownFormat::Heading2);
        assert_eq!(result, "line one\n## line two");
        assert_eq!(cursor, 15);
    }
    #[test]
    fn test_code_block_format() {
        let content = "hello world";
        let (result, cursor) = apply_markdown_format(content, 6, 11, MarkdownFormat::CodeBlock);
        assert_eq!(result, "hello ```\nworld\n```");
        assert_eq!(cursor, 19);
    }
    #[test]
    fn test_link_format() {
        let content = "check this out";
        let (result, cursor) = apply_markdown_format(content, 6, 10, MarkdownFormat::Link);
        assert_eq!(result, "check [this](url) out");
        assert_eq!(cursor, 13);
    }
}
