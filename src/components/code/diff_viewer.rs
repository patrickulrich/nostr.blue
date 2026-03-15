//! Unified & Side-by-Side Diff Viewer Component
//!
//! Renders diff content with colored line highlighting,
//! file headers, and hunk markers. Supports unified and side-by-side
//! view modes with collapsible hunks. Falls back to prose rendering
//! for cover letters or non-diff content.
//!
//! Supports line-level commenting: clickable "+" icons in the gutter
//! open an inline comment form, and existing line comments are
//! displayed between diff lines.
use std::collections::HashMap;

use crate::services::git_hosting::pull_requests::LineComment;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format::truncate_pubkey;
use crate::utils::format_relative_time_or;
use dioxus::prelude::*;

/// View mode for the diff viewer
#[derive(Clone, Copy, PartialEq)]
enum DiffViewMode {
    Unified,
    SideBySide,
}

/// A parsed diff hunk header
struct HunkHeader {
    old_start: usize,
    new_start: usize,
    #[allow(dead_code)]
    text: String,
}

/// Parse a hunk header like "@@ -1,5 +1,7 @@ fn main()"
fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
    let trimmed = line.trim_start_matches("@@ ");
    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let old_part = parts[0].trim_start_matches('-');
    let new_part = parts[1].trim_start_matches('+');
    let old_start = old_part.split(',').next()?.parse::<usize>().ok()?;
    let new_start = new_part
        .split(',')
        .next()?
        .trim_end_matches(" @@")
        .parse::<usize>()
        .ok()?;
    let text = if parts.len() > 2 {
        parts[2].trim_start_matches("@@ ").to_string()
    } else {
        String::new()
    };
    Some(HunkHeader {
        old_start,
        new_start,
        text,
    })
}

/// Check if content looks like a unified diff.
/// Only accept git-style diffs since `parse_diff_content` opens sections on
/// `diff --git` — plain unified patches would misparse without that boundary.
fn is_diff_content(content: &str) -> bool {
    content.lines().any(|line| line.starts_with("diff --git"))
}

/// Extract file path from a "diff --git a/path b/path" line.
/// Prefers the b/ side (destination) to handle renames correctly.
fn extract_file_path(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("diff --git ")?;
    // Handle quoted paths (e.g. "a/old name.rs" "b/new name.rs")
    if rest.contains('"') {
        // Extract the second quoted substring (b/ path)
        let mut in_quote = false;
        let mut quote_count = 0;
        let mut start = 0;
        for (i, c) in rest.char_indices() {
            if c == '"' {
                if in_quote {
                    quote_count += 1;
                    if quote_count == 2 {
                        let quoted = &rest[start..i];
                        return Some(quoted.strip_prefix("b/").unwrap_or(quoted));
                    }
                    in_quote = false;
                } else {
                    in_quote = true;
                    start = i + 1;
                }
            }
        }
    }
    // Try to get the b/ side path (handles renames)
    if let Some(b_path) = rest.split(" b/").nth(1) {
        Some(b_path)
    } else {
        // Fallback: try quoted paths (second token), strip a/ or b/ prefix
        rest.split(' ').nth(1).map(|p| {
            p.strip_prefix("b/")
                .or_else(|| p.strip_prefix("a/"))
                .unwrap_or(p)
        })
    }
}

/// A single row in side-by-side view
struct SideBySideRow {
    left_num: Option<usize>,
    left_content: Option<String>,
    left_kind: SideKind,
    right_num: Option<usize>,
    right_content: Option<String>,
    right_kind: SideKind,
}

#[derive(Clone, Copy, PartialEq)]
enum SideKind {
    Context,
    Remove,
    Add,
    Empty,
}

/// Build side-by-side rows from a slice of DiffLines (one hunk's worth)
fn build_side_by_side_rows(lines: &[DiffLine]) -> Vec<SideBySideRow> {
    let mut rows: Vec<SideBySideRow> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            LineKind::Context => {
                rows.push(SideBySideRow {
                    left_num: lines[i].old_num,
                    left_content: Some(lines[i].content.clone()),
                    left_kind: SideKind::Context,
                    right_num: lines[i].new_num,
                    right_content: Some(lines[i].content.clone()),
                    right_kind: SideKind::Context,
                });
                i += 1;
            }
            LineKind::Remove => {
                // Collect consecutive removes
                let remove_start = i;
                while i < lines.len() && matches!(lines[i].kind, LineKind::Remove) {
                    i += 1;
                }
                let removes = &lines[remove_start..i];

                // Collect consecutive adds that follow
                let add_start = i;
                while i < lines.len() && matches!(lines[i].kind, LineKind::Add) {
                    i += 1;
                }
                let adds = &lines[add_start..i];

                // Pair them up
                let max_len = removes.len().max(adds.len());
                for j in 0..max_len {
                    let left = removes.get(j);
                    let right = adds.get(j);
                    rows.push(SideBySideRow {
                        left_num: left.and_then(|l| l.old_num),
                        left_content: left.map(|l| l.content.clone()),
                        left_kind: if left.is_some() {
                            SideKind::Remove
                        } else {
                            SideKind::Empty
                        },
                        right_num: right.and_then(|l| l.new_num),
                        right_content: right.map(|l| l.content.clone()),
                        right_kind: if right.is_some() {
                            SideKind::Add
                        } else {
                            SideKind::Empty
                        },
                    });
                }
            }
            LineKind::Add => {
                // Standalone adds (not preceded by removes)
                rows.push(SideBySideRow {
                    left_num: None,
                    left_content: None,
                    left_kind: SideKind::Empty,
                    right_num: lines[i].new_num,
                    right_content: Some(lines[i].content.clone()),
                    right_kind: SideKind::Add,
                });
                i += 1;
            }
            LineKind::Info => {
                rows.push(SideBySideRow {
                    left_num: None,
                    left_content: Some(lines[i].content.clone()),
                    left_kind: SideKind::Context,
                    right_num: None,
                    right_content: None,
                    right_kind: SideKind::Empty,
                });
                i += 1;
            }
            LineKind::Hunk => {
                // Hunks are rendered separately, skip
                i += 1;
            }
        }
    }
    rows
}

/// Build a lookup from (file_path, line_number) to Vec<LineComment>
fn build_line_comment_map(comments: &[LineComment]) -> HashMap<(String, usize), Vec<LineComment>> {
    let mut map: HashMap<(String, usize), Vec<LineComment>> = HashMap::new();
    for c in comments {
        map.entry((c.file_path.clone(), c.line_number))
            .or_default()
            .push(c.clone());
    }
    map
}

/// Diff viewer component
///
/// Renders a unified or side-by-side diff with optional line-level
/// commenting support. When `pr_event_id` is provided, a "+" icon
/// appears in the gutter on hover to add inline comments.
#[component]
pub fn DiffViewer(
    content: String,
    is_cover_letter: bool,
    #[props(default)] pr_event_id: Option<String>,
    #[props(default)] line_comments: Vec<LineComment>,
    #[props(default)] on_line_comment: Option<EventHandler<(String, usize, String)>>,
) -> Element {
    let mut view_mode = use_signal(|| DiffViewMode::Unified);
    let mut collapsed_hunks: Signal<HashMap<(usize, usize), bool>> = use_signal(HashMap::new);
    // Track which file+line has the inline comment form open
    let mut active_comment_line: Signal<Option<(String, usize)>> = use_signal(|| None);
    // Text content of the inline comment form
    let mut comment_text = use_signal(String::new);

    // Reset comment and collapsed state when content prop changes
    use_effect(use_reactive(&content, move |_| {
        collapsed_hunks.set(HashMap::new());
        active_comment_line.set(None);
        comment_text.set(String::new());
    }));

    let has_commenting = pr_event_id.is_some() && on_line_comment.is_some();

    if is_cover_letter || !is_diff_content(&content) {
        return rsx! {
            div { class: "border border-border rounded-lg overflow-hidden",
                div { class: "px-4 py-2 bg-muted/50 border-b border-border",
                    span { class: "text-sm text-muted-foreground",
                        if is_cover_letter { "Cover Letter" } else { "Patch Content" }
                    }
                }
                div { class: "p-4 prose prose-sm max-w-none dark:prose-invert",
                    pre { class: "whitespace-pre-wrap text-sm font-mono", "{content}" }
                }
            }
        };
    }

    // Derive parsed diff directly from content prop
    let parsed = use_memo(use_reactive(&content, |c| parse_diff_content(&c)));
    let parsed_ref = parsed.read();
    let sections = &parsed_ref.sections;
    let section_hunks = &parsed_ref.section_hunks;

    let current_mode = *view_mode.read();

    // Build a lookup map for line comments by (file_path, line_number)
    let comment_map = use_memo(use_reactive(&line_comments, |lc| {
        build_line_comment_map(&lc)
    }));
    let comment_map = comment_map.read();

    // Column count for spanning rows (unified mode)
    // With commenting: comment-gutter + old-num + new-num + +/- indicator + content = 5
    // Without commenting: old-num + new-num + +/- indicator + content = 4
    let unified_colspan = if has_commenting { "5" } else { "4" };
    let side_by_side_colspan = "4";

    rsx! {
        div { class: "space-y-4",
            // View mode toggle
            div { class: "flex items-center gap-2 mb-4",
                div { class: "bg-muted rounded-lg overflow-hidden p-1",
                    button {
                        r#type: "button",
                        class: if current_mode == DiffViewMode::Unified { "px-3 py-1.5 text-xs bg-accent text-foreground font-medium" } else { "px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-accent/50 transition" },
                        onclick: move |_| {
                            active_comment_line.set(None);
                            comment_text.set(String::new());
                            view_mode.set(DiffViewMode::Unified);
                        },
                        "Unified"
                    }
                    button {
                        r#type: "button",
                        class: if current_mode == DiffViewMode::SideBySide { "px-3 py-1.5 text-xs bg-accent text-foreground font-medium" } else { "px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-accent/50 transition" },
                        onclick: move |_| {
                            active_comment_line.set(None);
                            comment_text.set(String::new());
                            view_mode.set(DiffViewMode::SideBySide);
                        },
                        "Side by Side"
                    }
                }
            }

            for (section_idx, section) in sections.iter().enumerate() {
                div {
                    key: "{section_idx}",
                    class: "border border-border rounded-lg overflow-hidden",
                    if !section.file_path.is_empty() {
                        div { class: "px-4 py-2 bg-muted/50 border-b border-border flex items-center gap-2",
                            svg {
                                class: "w-4 h-4 text-muted-foreground",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                polyline { points: "14 2 14 8 20 8" }
                            }
                            span { class: "text-sm font-mono font-medium", "{section.file_path}" }
                        }
                    }
                    div { class: "overflow-x-auto",
                        {
                            let file_key_for_comments = section.file_path.clone();
                            let file_comments: HashMap<usize, &Vec<LineComment>> = comment_map
                                .iter()
                                .filter(|((f, _), _)| f == &file_key_for_comments)
                                .map(|((_, ln), c)| (*ln, c))
                                .collect();
                            rsx! {
                        table { class: "w-full text-xs font-mono leading-5",
                            tbody {
                                for (hunk_idx, hunk) in section_hunks[section_idx].iter().enumerate() {
                                    // Hunk header row (clickable for collapse)
                                    if let Some(header) = &hunk.header {
                                        {
                                            let is_collapsed = collapsed_hunks.read().get(&(section_idx, hunk_idx)).copied().unwrap_or(false);
                                            let header_text = header.clone();
                                            rsx! {
                                                tr {
                                                    key: "hunk-{hunk_idx}",
                                                    class: "bg-blue-500/10 cursor-pointer hover:bg-blue-500/20 transition",
                                                    tabindex: "0",
                                                    role: "button",
                                                    aria_expanded: if is_collapsed { "false" } else { "true" },
                                                    onclick: move |_| {
                                                        let key = (section_idx, hunk_idx);
                                                        let mut map = collapsed_hunks.write();
                                                        let current = map.get(&key).copied().unwrap_or(false);
                                                        map.insert(key, !current);
                                                    },
                                                    onkeydown: move |e: KeyboardEvent| {
                                                        if matches!(e.key(), Key::Enter) || matches!(e.key(), Key::Character(ref c) if c == " ") {
                                                            e.prevent_default();
                                                            let key = (section_idx, hunk_idx);
                                                            let mut map = collapsed_hunks.write();
                                                            let current = map.get(&key).copied().unwrap_or(false);
                                                            map.insert(key, !current);
                                                        }
                                                    },
                                                    if current_mode == DiffViewMode::SideBySide {
                                                        td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50" }
                                                        td {
                                                            class: "px-2 whitespace-pre",
                                                            colspan: "3",
                                                            span { class: "text-blue-400",
                                                                if is_collapsed { "▶ " } else { "▼ " }
                                                                "{header_text}"
                                                            }
                                                        }
                                                    } else {
                                                        if has_commenting {
                                                            td { class: "w-5" }
                                                        }
                                                        td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50" }
                                                        td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50" }
                                                        td { class: "w-4 px-1 text-center select-none" }
                                                        td { class: "px-2 whitespace-pre",
                                                            span { class: "text-blue-400",
                                                                if is_collapsed { "▶ " } else { "▼ " }
                                                                "{header_text}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Hunk body (only if not collapsed)
                                    {
                                        let is_collapsed = collapsed_hunks.read().get(&(section_idx, hunk_idx)).copied().unwrap_or(false);
                                        if !is_collapsed {
                                            match current_mode {
                                                DiffViewMode::Unified => {
                                                    let cached_file_path = section.file_path.clone();
                                                    rsx! {
                                                    for (line_idx, diff_line) in hunk.lines.iter().enumerate() {
                                                        // The diff line row
                                                        tr {
                                                            key: "line-{hunk_idx}-{line_idx}",
                                                            class: {
                                                                let bg = match diff_line.kind {
                                                                    LineKind::Add => "bg-green-500/10",
                                                                    LineKind::Remove => "bg-red-500/10",
                                                                    LineKind::Hunk => "bg-blue-500/10",
                                                                    LineKind::Info => "bg-muted/50",
                                                                    LineKind::Context => "",
                                                                };
                                                                if has_commenting && !matches!(diff_line.kind, LineKind::Hunk | LineKind::Info) {
                                                                    format!("{bg} group")
                                                                } else {
                                                                    bg.to_string()
                                                                }
                                                            },
                                                            // Comment gutter "+" button (only in unified mode with commenting enabled)
                                                            if has_commenting {
                                                                td { class: "w-5 text-center select-none border-r border-border/50",
                                                                    if diff_line.new_num.is_some() {
                                                                        {
                                                                            let file_for_click = cached_file_path.clone();
                                                                            let line_num = diff_line.new_num.unwrap();
                                                                            rsx! {
                                                                                button {
                                                                                    r#type: "button",
                                                                                    class: "p-1 hover:bg-accent rounded transition text-primary opacity-0 group-hover:opacity-100 focus:opacity-100 focus-visible:ring-1 focus-visible:ring-primary text-[10px] font-bold flex items-center justify-center",
                                                                                    aria_label: "Add line comment",
                                                                                    title: "Add line comment",
                                                                                    onclick: move |e| {
                                                                                        e.stop_propagation();
                                                                                        active_comment_line.set(Some((file_for_click.clone(), line_num)));
                                                                                        comment_text.set(String::new());
                                                                                    },
                                                                                    "+"
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50",
                                                                if let Some(n) = diff_line.old_num {
                                                                    "{n}"
                                                                }
                                                            }
                                                            td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50",
                                                                if let Some(n) = diff_line.new_num {
                                                                    "{n}"
                                                                }
                                                            }
                                                            td { class: "w-4 px-1 text-center select-none",
                                                                match diff_line.kind {
                                                                    LineKind::Add => rsx! { span { class: "text-green-400", "+" } },
                                                                    LineKind::Remove => rsx! { span { class: "text-red-400", "-" } },
                                                                    _ => rsx! { span {} },
                                                                }
                                                            }
                                                            td { class: "px-2 whitespace-pre",
                                                                match diff_line.kind {
                                                                    LineKind::Hunk => rsx! {
                                                                        span { class: "text-blue-400", "{diff_line.content}" }
                                                                    },
                                                                    LineKind::Add => rsx! {
                                                                        span { class: "text-green-400", "{diff_line.content}" }
                                                                    },
                                                                    LineKind::Remove => rsx! {
                                                                        span { class: "text-red-400", "{diff_line.content}" }
                                                                    },
                                                                    LineKind::Info => rsx! {
                                                                        span { class: "text-muted-foreground italic", "{diff_line.content}" }
                                                                    },
                                                                    LineKind::Context => rsx! {
                                                                        span { "{diff_line.content}" }
                                                                    },
                                                                }
                                                            }
                                                        }

                                                        // Display existing line comments for this file + line
                                                        if !matches!(diff_line.kind, LineKind::Hunk | LineKind::Info) {
                                                            if let Some(line_num) = diff_line.new_num {
                                                                if let Some(comments) = file_comments.get(&line_num) {
                                                                    for lc in comments.iter() {
                                                                        tr {
                                                                            key: "lc-{lc.event_id}",
                                                                            class: "bg-accent/30",
                                                                            td {
                                                                                colspan: unified_colspan,
                                                                                class: "px-4 py-2",
                                                                                InlineLineComment { comment: lc.clone() }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        // Inline comment form (if this line is selected)
                                                        if has_commenting {
                                                            if let Some(line_num) = diff_line.new_num {
                                                                {
                                                                    let is_active = active_comment_line.read().as_ref()
                                                                        .map(|(f, l)| f == &cached_file_path && *l == line_num)
                                                                        .unwrap_or(false);
                                                                    if is_active {
                                                                        let file_for_submit = cached_file_path.clone();
                                                                        rsx! {
                                                                            tr {
                                                                                key: "comment-form-{line_num}",
                                                                                class: "bg-accent/20",
                                                                                td {
                                                                                    colspan: unified_colspan,
                                                                                    class: "p-3",
                                                                                    div { class: "space-y-2",
                                                                                        textarea {
                                                                                            class: "w-full px-3 py-2 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-primary resize-y font-sans",
                                                                                            placeholder: "Write a line comment...",
                                                                                            rows: 3,
                                                                                            value: "{comment_text}",
                                                                                            oninput: move |e| comment_text.set(e.value()),
                                                                                        }
                                                                                        div { class: "flex justify-end gap-2",
                                                                                            button {
                                                                                                r#type: "button",
                                                                                                class: "px-3 py-1 text-xs text-muted-foreground hover:text-foreground transition",
                                                                                                onclick: move |_| {
                                                                                                    active_comment_line.set(None);
                                                                                                    comment_text.set(String::new());
                                                                                                },
                                                                                                "Cancel"
                                                                                            }
                                                                                            button {
                                                                                                r#type: "button",
                                                                                                class: "px-3 py-1 text-xs bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                                                                                                disabled: comment_text.read().trim().is_empty(),
                                                                                                onclick: {
                                                                                                    let file_path = file_for_submit.clone();
                                                                                                    move |_| {
                                                                                                        let text = comment_text.read().clone();
                                                                                                        if !text.trim().is_empty() {
                                                                                                            if let Some(ref handler) = on_line_comment {
                                                                                                                handler.call((file_path.clone(), line_num, text));
                                                                                                            }
                                                                                                            active_comment_line.set(None);
                                                                                                            comment_text.set(String::new());
                                                                                                        }
                                                                                                    }
                                                                                                },
                                                                                                "Comment"
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    } else {
                                                                        rsx! {}
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }},
                                                DiffViewMode::SideBySide => {
                                                    let cached_file_path = section.file_path.clone();
                                                    let rows = build_side_by_side_rows(&hunk.lines);
                                                    rsx! {
                                                        for (row_idx, row) in rows.into_iter().enumerate() {
                                                            {
                                                                let is_active = row.right_num
                                                                    .and_then(|line_num| {
                                                                        active_comment_line.read().as_ref().map(|(f, l)| {
                                                                            f == &cached_file_path && *l == line_num
                                                                        })
                                                                    })
                                                                    .unwrap_or(false);
                                                                let file_for_click = cached_file_path.clone();
                                                                let file_for_click_keydown = file_for_click.clone();
                                                                rsx! {
                                                                    tr {
                                                                        key: "sbs-{hunk_idx}-{row_idx}",
                                                                        td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50",
                                                                            if let Some(n) = row.left_num {
                                                                                "{n}"
                                                                            }
                                                                        }
                                                                        td {
                                                                            class: match row.left_kind {
                                                                                SideKind::Remove => "w-1/2 px-2 whitespace-pre bg-red-500/10 border-r border-border/50",
                                                                                SideKind::Empty => "w-1/2 px-2 whitespace-pre bg-muted/30 border-r border-border/50",
                                                                                _ => "w-1/2 px-2 whitespace-pre border-r border-border/50",
                                                                            },
                                                                            if let Some(ref text) = row.left_content {
                                                                                span {
                                                                                    class: if row.left_kind == SideKind::Remove { "text-red-400" } else { "" },
                                                                                    "{text}"
                                                                                }
                                                                            }
                                                                        }
                                                                        td { class: "w-10 px-2 text-right text-muted-foreground select-none border-r border-border/50",
                                                                            if let Some(n) = row.right_num {
                                                                                "{n}"
                                                                            }
                                                                        }
                                                                        td {
                                                                            class: match row.right_kind {
                                                                                SideKind::Add => "w-1/2 px-2 whitespace-pre bg-green-500/10",
                                                                                SideKind::Empty => "w-1/2 px-2 whitespace-pre bg-muted/30",
                                                                                _ => "w-1/2 px-2 whitespace-pre",
                                                                            },
                                                                            if has_commenting && row.right_num.is_some() {
                                                                                div { class: "flex items-start gap-2",
                                                                                    button {
                                                                                        r#type: "button",
                                                                                        aria_pressed: is_active.to_string(),
                                                                                        class: match (row.right_kind, is_active) {
                                                                                            (SideKind::Add, true) => "mt-0.5 h-5 w-5 shrink-0 rounded-sm bg-green-500/15 ring-1 ring-inset ring-primary hover:bg-green-500/20 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary",
                                                                                            (SideKind::Add, false) => "mt-0.5 h-5 w-5 shrink-0 rounded-sm bg-green-500/10 hover:bg-green-500/20 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary",
                                                                                            (_, true) => "mt-0.5 h-5 w-5 shrink-0 rounded-sm bg-accent/10 ring-1 ring-inset ring-primary hover:bg-accent/20 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary",
                                                                                            (_, false) => "mt-0.5 h-5 w-5 shrink-0 rounded-sm hover:bg-accent/10 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary",
                                                                                        },
                                                                                        onclick: move |_| {
                                                                                            active_comment_line.set(Some((file_for_click.clone(), row.right_num.unwrap())));
                                                                                            comment_text.set(String::new());
                                                                                        },
                                                                                        onkeydown: move |e: KeyboardEvent| {
                                                                                            if matches!(e.key(), Key::Enter)
                                                                                                || matches!(e.key(), Key::Character(ref c) if c == " ")
                                                                                            {
                                                                                                e.prevent_default();
                                                                                                active_comment_line.set(Some((file_for_click_keydown.clone(), row.right_num.unwrap())));
                                                                                                comment_text.set(String::new());
                                                                                            }
                                                                                        },
                                                                                        aria_label: "Add line comment",
                                                                                        title: "Add line comment",
                                                                                        "+"
                                                                                    }
                                                                                    if let Some(ref text) = row.right_content {
                                                                                        span {
                                                                                            class: if row.right_kind == SideKind::Add { "text-green-400" } else { "" },
                                                                                            "{text}"
                                                                                        }
                                                                                    }
                                                                                }
                                                                            } else if let Some(ref text) = row.right_content {
                                                                                span {
                                                                                    class: if row.right_kind == SideKind::Add { "text-green-400" } else { "" },
                                                                                    "{text}"
                                                                                }
                                                                            }
                                                                        }
                                                                    }

                                                                    if let Some(line_num) = row.right_num {
                                                                        if let Some(comments) = file_comments.get(&line_num) {
                                                                            for lc in comments.iter() {
                                                                                tr {
                                                                                    key: "lc-sbs-{lc.event_id}",
                                                                                    class: "bg-accent/30",
                                                                                    td {
                                                                                        colspan: side_by_side_colspan,
                                                                                        class: "px-4 py-2",
                                                                                        InlineLineComment { comment: lc.clone() }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }

                                                                    if has_commenting {
                                                                        if let Some(line_num) = row.right_num {
                                                                            if is_active {
                                                                                {
                                                                                    let file_for_submit = cached_file_path.clone();
                                                                                    rsx! {
                                                                                        tr {
                                                                                            key: "comment-form-sbs-{line_num}",
                                                                                            class: "bg-accent/20",
                                                                                            td {
                                                                                                colspan: side_by_side_colspan,
                                                                                                class: "p-3",
                                                                                                div { class: "space-y-2",
                                                                                                    textarea {
                                                                                                        class: "w-full px-3 py-2 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-primary resize-y font-sans",
                                                                                                        placeholder: "Write a line comment...",
                                                                                                        rows: 3,
                                                                                                        value: "{comment_text}",
                                                                                                        oninput: move |e| comment_text.set(e.value()),
                                                                                                    }
                                                                                                    div { class: "flex justify-end gap-2",
                                                                                                        button {
                                                                                                            r#type: "button",
                                                                                                            class: "px-3 py-1 text-xs text-muted-foreground hover:text-foreground transition",
                                                                                                            onclick: move |_| {
                                                                                                                active_comment_line.set(None);
                                                                                                                comment_text.set(String::new());
                                                                                                            },
                                                                                                            "Cancel"
                                                                                                        }
                                                                                                        button {
                                                                                                            r#type: "button",
                                                                                                            class: "px-3 py-1 text-xs bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                                                                                                            disabled: comment_text.read().trim().is_empty(),
                                                                                                            onclick: {
                                                                                                                let file_path = file_for_submit.clone();
                                                                                                                move |_| {
                                                                                                                    let text = comment_text.read().clone();
                                                                                                                    if !text.trim().is_empty() {
                                                                                                                        if let Some(ref handler) = on_line_comment {
                                                                                                                            handler.call((file_path.clone(), line_num, text));
                                                                                                                        }
                                                                                                                        active_comment_line.set(None);
                                                                                                                        comment_text.set(String::new());
                                                                                                                    }
                                                                                                                }
                                                                                                            },
                                                                                                            "Comment"
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
                                                            }
                                                        }
                                                    }
                                                },
                                            }
                                        } else {
                                            rsx! {}
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
    }
}

/// Inline display of a single line comment within the diff
#[component]
fn InlineLineComment(comment: LineComment) -> Element {
    let author_profile = PROFILE_CACHE.read().peek(&comment.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&comment.pubkey));
    rsx! {
        div { class: "flex items-start gap-2 text-xs font-sans",
            div { class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center shrink-0 overflow-hidden",
                if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                    img {
                        class: "w-full h-full object-cover",
                        src: "{picture}",
                        alt: "Author",
                    }
                } else {
                    span { class: "text-[10px]", "{author_name.chars().next().unwrap_or('?')}" }
                }
            }
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2",
                    span { class: "font-medium text-foreground", "{author_name}" }
                    span { class: "text-muted-foreground",
                        {format_relative_time_or(comment.created_at, "Unknown")}
                    }
                }
                p { class: "text-sm text-foreground mt-0.5 whitespace-pre-wrap", "{comment.content}" }
            }
        }
    }
}

#[derive(PartialEq)]
struct DiffSection {
    file_path: String,
    lines: Vec<DiffLine>,
}

#[derive(PartialEq)]
struct DiffLine {
    kind: LineKind,
    old_num: Option<usize>,
    new_num: Option<usize>,
    content: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Context,
    Add,
    Remove,
    Hunk,
    Info,
}

#[derive(PartialEq)]
struct Hunk {
    header: Option<String>,
    lines: Vec<DiffLine>,
}

/// Parsed diff result containing sections and their hunks
#[derive(PartialEq)]
struct ParsedDiff {
    sections: Vec<DiffSection>,
    section_hunks: Vec<Vec<Hunk>>,
}

/// Parse diff content into sections and hunks
fn parse_diff_content(content: &str) -> ParsedDiff {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections: Vec<DiffSection> = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_lines: Vec<DiffLine> = Vec::new();
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;
    let mut in_header = false;

    let mut binary_hint: Option<String> = None;
    let mut header_summary: Vec<String> = Vec::new();

    for line in &lines {
        if line.starts_with("diff --git") {
            // Flush previous section — even if current_lines is empty (header-only/binary diff)
            if let Some(ref file) = current_file {
                if current_lines.is_empty() {
                    let info_text = binary_hint
                        .take()
                        .or_else(|| {
                            (!header_summary.is_empty()).then(|| header_summary.join(" • "))
                        })
                        .unwrap_or_else(|| "(Binary file or file metadata changed)".to_string());
                    current_lines.push(DiffLine {
                        kind: LineKind::Info,
                        old_num: None,
                        new_num: None,
                        content: info_text,
                    });
                }
                sections.push(DiffSection {
                    file_path: file.clone(),
                    lines: std::mem::take(&mut current_lines),
                });
            } else if !current_lines.is_empty() {
                sections.push(DiffSection {
                    file_path: String::new(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            binary_hint = None;
            header_summary.clear();
            current_file = extract_file_path(line).map(|s| s.to_string());
            in_header = true;
            continue;
        }

        if in_header
            && (line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("index "))
        {
            continue;
        }

        if in_header
            && (line.starts_with("old mode")
                || line.starts_with("new mode")
                || line.starts_with("new file")
                || line.starts_with("deleted file")
                || line.starts_with("similarity")
                || line.starts_with("rename"))
        {
            header_summary.push(line.to_string());
            continue;
        }

        if in_header && line.starts_with("Binary") {
            binary_hint = Some(line.to_string());
            continue;
        }

        if line.starts_with("@@ ") {
            if let Some(hunk) = parse_hunk_header(line) {
                in_header = false;
                old_line = hunk.old_start;
                new_line = hunk.new_start;
                current_lines.push(DiffLine {
                    kind: LineKind::Hunk,
                    old_num: None,
                    new_num: None,
                    content: line.to_string(),
                });
            } else {
                // Malformed hunk header — treat as info line, don't change counters
                in_header = false;
                current_lines.push(DiffLine {
                    kind: LineKind::Info,
                    old_num: None,
                    new_num: None,
                    content: line.to_string(),
                });
            }
            continue;
        }

        if in_header {
            continue;
        }

        if let Some(added) = line.strip_prefix('+') {
            current_lines.push(DiffLine {
                kind: LineKind::Add,
                old_num: None,
                new_num: Some(new_line),
                content: added.to_string(),
            });
            new_line += 1;
        } else if let Some(removed) = line.strip_prefix('-') {
            current_lines.push(DiffLine {
                kind: LineKind::Remove,
                old_num: Some(old_line),
                new_num: None,
                content: removed.to_string(),
            });
            old_line += 1;
        } else if line.starts_with('\\') {
            current_lines.push(DiffLine {
                kind: LineKind::Info,
                old_num: None,
                new_num: None,
                content: line.to_string(),
            });
        } else {
            let display = if let Some(stripped) = line.strip_prefix(' ') {
                stripped.to_string()
            } else {
                line.to_string()
            };
            current_lines.push(DiffLine {
                kind: LineKind::Context,
                old_num: Some(old_line),
                new_num: Some(new_line),
                content: display,
            });
            old_line += 1;
            new_line += 1;
        }
    }

    // Flush final section — handle header-only/binary diffs
    if let Some(ref file) = current_file {
        if current_lines.is_empty() {
            let info_text = binary_hint
                .take()
                .or_else(|| (!header_summary.is_empty()).then(|| header_summary.join(" • ")))
                .unwrap_or_else(|| "(Binary file or file metadata changed)".to_string());
            current_lines.push(DiffLine {
                kind: LineKind::Info,
                old_num: None,
                new_num: None,
                content: info_text,
            });
        }
        sections.push(DiffSection {
            file_path: file.clone(),
            lines: std::mem::take(&mut current_lines),
        });
    } else if !current_lines.is_empty() {
        sections.push(DiffSection {
            file_path: String::new(),
            lines: std::mem::take(&mut current_lines),
        });
    }

    // Split each section's lines into hunks for collapse support
    let section_hunks: Vec<Vec<Hunk>> = sections
        .iter_mut()
        .map(|section| {
            let mut hunks: Vec<Hunk> = Vec::new();
            let mut current_hunk_header: Option<String> = None;
            let mut current_hunk_lines: Vec<DiffLine> = Vec::new();

            for dl in std::mem::take(&mut section.lines) {
                if matches!(dl.kind, LineKind::Hunk) {
                    if current_hunk_header.is_some() || !current_hunk_lines.is_empty() {
                        hunks.push(Hunk {
                            header: current_hunk_header.take(),
                            lines: std::mem::take(&mut current_hunk_lines),
                        });
                    }
                    current_hunk_header = Some(dl.content);
                } else {
                    current_hunk_lines.push(dl);
                }
            }
            if current_hunk_header.is_some() || !current_hunk_lines.is_empty() {
                hunks.push(Hunk {
                    header: current_hunk_header,
                    lines: current_hunk_lines,
                });
            }
            hunks
        })
        .collect();

    ParsedDiff {
        sections,
        section_hunks,
    }
}
