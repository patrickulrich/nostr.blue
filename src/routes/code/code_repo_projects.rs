//! Project Board
//!
//! Kanban-style view of issues and PRs organized by status.
//! Features: Kanban board, click-to-move status, custom columns (NIP-78), roadmap view.
#![allow(dead_code)]
use crate::components::code::RepoTabNav;
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_repo_issues, fetch_repo_prs, fetch_repository, update_issue_status_by_id,
};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::{Issue, IssueStatus, PullRequest, Repository};
use crate::utils::time::format_time_ago;
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, Kind, PublicKey, Tag};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ─── Board Item ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum BoardItem {
    Issue(Issue),
    PullRequest(PullRequest),
}

impl BoardItem {
    fn status(&self) -> IssueStatus {
        match self {
            Self::Issue(i) => i.status,
            Self::PullRequest(p) => p.status,
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Issue(i) => i.subject.as_deref().unwrap_or("Untitled"),
            Self::PullRequest(p) => p.content.lines().next().unwrap_or("Untitled"),
        }
    }

    fn event_id(&self) -> &str {
        match self {
            Self::Issue(i) => &i.event_id,
            Self::PullRequest(p) => &p.event_id,
        }
    }

    fn pubkey(&self) -> &str {
        match self {
            Self::Issue(i) => &i.pubkey,
            Self::PullRequest(p) => &p.pubkey,
        }
    }

    fn created_at(&self) -> u64 {
        match self {
            Self::Issue(i) => i.created_at,
            Self::PullRequest(p) => p.created_at,
        }
    }

    fn is_pr(&self) -> bool {
        matches!(self, Self::PullRequest(_))
    }

    fn detail_route(&self) -> Route {
        match self {
            Self::Issue(i) => Route::CodeIssueDetail {
                note_id: i.event_id.clone(),
            },
            Self::PullRequest(p) => Route::CodePullDetail {
                note_id: p.event_id.clone(),
            },
        }
    }

    fn labels(&self) -> &[String] {
        match self {
            Self::Issue(i) => &i.labels,
            Self::PullRequest(p) => &p.labels,
        }
    }
}

// ─── Custom Columns (NIP-78) ─────────────────────────────────────────────────

const APP_DATA_KIND: u16 = 30078;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BoardColumn {
    id: String,
    name: String,
    order: u32,
    /// Maps to a standard status, if any
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoardLayout {
    columns: Vec<BoardColumn>,
}

fn default_columns() -> Vec<BoardColumn> {
    vec![
        BoardColumn {
            id: "draft".to_string(),
            name: "Draft".to_string(),
            order: 0,
            status: Some("draft".to_string()),
        },
        BoardColumn {
            id: "open".to_string(),
            name: "Open".to_string(),
            order: 1,
            status: Some("open".to_string()),
        },
        BoardColumn {
            id: "applied".to_string(),
            name: "Applied".to_string(),
            order: 2,
            status: Some("applied".to_string()),
        },
        BoardColumn {
            id: "closed".to_string(),
            name: "Closed".to_string(),
            order: 3,
            status: Some("closed".to_string()),
        },
    ]
}

fn column_color(status: Option<&str>) -> &'static str {
    match status {
        Some("draft") => "bg-muted-foreground",
        Some("open") => "bg-blue-500",
        Some("applied") => "bg-green-500",
        Some("closed") => "bg-red-500",
        _ => "bg-purple-500",
    }
}

fn status_from_string(s: &str) -> Option<IssueStatus> {
    match s {
        "draft" => Some(IssueStatus::Draft),
        "open" => Some(IssueStatus::Open),
        "applied" => Some(IssueStatus::Applied),
        "closed" => Some(IssueStatus::Closed),
        _ => None,
    }
}

fn status_to_string(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Draft => "draft",
        IssueStatus::Open => "open",
        IssueStatus::Applied => "applied",
        IssueStatus::Closed => "closed",
    }
}

async fn fetch_board_layout(naddr: &str) -> Option<Vec<BoardColumn>> {
    let client = nostr_client::get_client()?;
    let pubkey_str = auth_store::get_pubkey()?;
    let pubkey = PublicKey::from_hex(&pubkey_str).ok()?;
    let d_tag = format!("board-layout-{}", naddr);
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(&d_tag)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(8)).await {
        if let Some(event) = events.into_iter().next() {
            if let Ok(layout) = serde_json::from_str::<BoardLayout>(&event.content) {
                if !layout.columns.is_empty() {
                    return Some(layout.columns);
                }
            }
        }
    }
    None
}

async fn save_board_layout(naddr: &str, columns: &[BoardColumn]) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    let d_tag = format!("board-layout-{}", naddr);
    let layout = BoardLayout {
        columns: columns.to_vec(),
    };
    let content =
        serde_json::to_string(&layout).map_err(|e| format!("Failed to serialize: {}", e))?;
    nostr_client::ensure_relays_ready(&client).await;
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), content)
        .tag(Tag::identifier(&d_tag));
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish layout: {}", e))?;
    Ok(())
}

fn is_repo_owner_or_maintainer(repo: &Repository) -> bool {
    if let Some(my_pubkey) = auth_store::get_pubkey() {
        if repo.pubkey == my_pubkey {
            return true;
        }
        if repo.maintainers.contains(&my_pubkey) {
            return true;
        }
    }
    false
}

// ─── View Toggle ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum BoardView {
    Board,
    Roadmap,
}

// ─── Main Component ──────────────────────────────────────────────────────────

#[component]
pub fn CodeRepoProjects(naddr: String) -> Element {
    let mut items = use_signal(Vec::<BoardItem>::new);
    let mut loading = use_signal(|| true);
    let mut repo = use_signal(|| None::<Repository>);
    let mut columns = use_signal(default_columns);
    let mut view_mode = use_signal(|| BoardView::Board);
    let mut show_customize = use_signal(|| false);

    // Fetch repo + issues + PRs + custom layout
    let naddr_for_effect = naddr.clone();
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);

            // Fetch repo metadata
            if let Ok(r) = fetch_repository(&n).await {
                repo.set(Some(r));
            }

            // Fetch custom board layout
            if let Some(custom_cols) = fetch_board_layout(&n).await {
                columns.set(custom_cols);
            }

            // Fetch issues and PRs
            let mut all_items = Vec::new();
            if let Ok(issues) = fetch_repo_issues(&n).await {
                all_items.extend(issues.into_iter().map(BoardItem::Issue));
            }
            if let Ok(prs) = fetch_repo_prs(&n).await {
                all_items.extend(prs.into_iter().map(BoardItem::PullRequest));
            }
            items.set(all_items);
            loading.set(false);
        });
    });

    let items_read = items.read();
    let columns_read = columns.read();
    let can_customize = repo
        .read()
        .as_ref()
        .map(is_repo_owner_or_maintainer)
        .unwrap_or(false);

    rsx! {
        div { class: "min-h-screen",
            if let Some(r) = repo.read().as_ref() {
                RepoTabNav {
                    naddr: naddr.clone(),
                    active_tab: "projects".to_string(),
                    issue_count: Some(r.issue_count),
                    pr_count: Some(r.pr_count),
                }
            }

            div { class: "p-4",
                // Header with view toggle and customize button
                div { class: "flex items-center gap-2 mb-4",
                    svg {
                        class: "w-5 h-5",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        rect { x: "3", y: "3", width: "7", height: "18", rx: "1" }
                        rect { x: "14", y: "3", width: "7", height: "10", rx: "1" }
                    }
                    h2 { class: "text-lg font-bold", "Project Board" }

                    // View toggle
                    div { class: "ml-auto flex items-center gap-1 bg-muted rounded-lg p-0.5",
                        button {
                            class: if *view_mode.read() == BoardView::Board {
                                "px-3 py-1 text-xs rounded-md bg-background text-foreground font-medium shadow-sm"
                            } else {
                                "px-3 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| view_mode.set(BoardView::Board),
                            "Board"
                        }
                        button {
                            class: if *view_mode.read() == BoardView::Roadmap {
                                "px-3 py-1 text-xs rounded-md bg-background text-foreground font-medium shadow-sm"
                            } else {
                                "px-3 py-1 text-xs rounded-md text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| view_mode.set(BoardView::Roadmap),
                            "Roadmap"
                        }
                    }

                    // Customize button (only for repo owner/maintainers)
                    if can_customize {
                        button {
                            class: "px-3 py-1.5 text-xs rounded-lg border border-border text-muted-foreground hover:bg-accent transition",
                            onclick: move |_| show_customize.set(true),
                            "Customize"
                        }
                    }
                }

                if *loading.read() {
                    LoadingSkeleton {}
                } else if items_read.is_empty() {
                    EmptyBoard {}
                } else {
                    match *view_mode.read() {
                        BoardView::Board => rsx! {
                            div { class: "flex gap-4 overflow-x-auto pb-4",
                                for col in columns_read.iter() {
                                    {
                                        let col_items: Vec<_> = if let Some(ref status_str) = col.status {
                                            if let Some(status) = status_from_string(status_str) {
                                                items_read.iter().filter(|i| i.status() == status).cloned().collect()
                                            } else {
                                                Vec::new()
                                            }
                                        } else {
                                            Vec::new()
                                        };
                                        let color = column_color(col.status.as_deref());
                                        rsx! {
                                            BoardColumnView {
                                                key: "{col.id}",
                                                title: col.name.clone(),
                                                color: color,
                                                count: col_items.len(),
                                                items: col_items,
                                                items_signal: items,
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        BoardView::Roadmap => rsx! {
                            RoadmapView { items: items_read.clone() }
                        },
                    }
                }
            }
        }

        // Customize columns modal
        if *show_customize.read() {
            CustomizeColumnsModal {
                columns: columns.read().clone(),
                naddr: naddr.clone(),
                on_save: move |new_cols: Vec<BoardColumn>| {
                    columns.set(new_cols);
                    show_customize.set(false);
                },
                on_close: move |_| show_customize.set(false),
            }
        }
    }
}

// ─── Board Column View ───────────────────────────────────────────────────────

#[component]
fn BoardColumnView(
    title: String,
    color: &'static str,
    count: usize,
    items: Vec<BoardItem>,
    items_signal: Signal<Vec<BoardItem>>,
) -> Element {
    rsx! {
        div { class: "min-w-72 flex-shrink-0 bg-muted/30 rounded-lg",
            // Column header
            div { class: "flex items-center gap-2 p-3 border-b border-border",
                span { class: "w-3 h-3 rounded-full {color}" }
                span { class: "font-semibold text-sm", "{title}" }
                span { class: "ml-auto px-2 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                    "{count}"
                }
            }

            // Cards
            div { class: "p-2 space-y-2 max-h-[70vh] overflow-y-auto",
                if items.is_empty() {
                    div { class: "text-center py-8 text-sm text-muted-foreground",
                        "No items"
                    }
                } else {
                    for item in items.iter() {
                        BoardCard {
                            key: "{item.event_id()}",
                            item: item.clone(),
                            items_signal: items_signal,
                        }
                    }
                }
            }
        }
    }
}

// ─── Board Card ──────────────────────────────────────────────────────────────

#[component]
fn BoardCard(item: BoardItem, items_signal: Signal<Vec<BoardItem>>) -> Element {
    let time_ago = format_time_ago(item.created_at());
    let truncated_author = {
        let pk = item.pubkey();
        if pk.len() > 12 {
            format!("{}...{}", &pk[..6], &pk[pk.len() - 4..])
        } else {
            pk.to_string()
        }
    };
    let is_authenticated = auth_store::is_authenticated();
    let current_status = item.status();
    let event_id_str = item.event_id().to_string();

    rsx! {
        Link {
            to: item.detail_route(),
            class: "block bg-card border border-border rounded-lg p-3 hover:bg-accent/50 transition cursor-pointer",
            // Title
            div { class: "font-medium text-sm mb-2 line-clamp-2", "{item.title()}" }

            // Labels
            if !item.labels().is_empty() {
                div { class: "flex flex-wrap gap-1 mb-2",
                    for label in item.labels().iter().take(3) {
                        span { class: "px-1.5 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20",
                            "{label}"
                        }
                    }
                }
            }

            // Footer: type badge + author + time + status dropdown
            div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                // Type badge
                if item.is_pr() {
                    span { class: "flex items-center gap-1 px-1.5 py-0.5 rounded bg-purple-500/10 text-purple-400",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "18", cy: "18", r: "3" }
                            circle { cx: "6", cy: "6", r: "3" }
                            path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                            line { x1: "6", y1: "9", x2: "6", y2: "21" }
                        }
                        "PR"
                    }
                } else {
                    span { class: "flex items-center gap-1 px-1.5 py-0.5 rounded bg-green-500/10 text-green-400",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "10" }
                            circle { cx: "12", cy: "12", r: "1" }
                        }
                        "Issue"
                    }
                }
                span { class: "truncate", "{truncated_author}" }
                span { class: "ml-auto whitespace-nowrap", "{time_ago}" }
            }

            // Status dropdown (only for authenticated users)
            if is_authenticated {
                div { class: "mt-2 pt-2 border-t border-border",
                    StatusDropdown {
                        current_status: current_status,
                        event_id: event_id_str,
                        items_signal: items_signal,
                    }
                }
            }
        }
    }
}

// ─── Status Dropdown ─────────────────────────────────────────────────────────

#[component]
fn StatusDropdown(
    current_status: IssueStatus,
    event_id: String,
    items_signal: Signal<Vec<BoardItem>>,
) -> Element {
    let mut updating = use_signal(|| false);

    let status_value = status_to_string(current_status).to_string();

    rsx! {
        select {
            class: "w-full px-2 py-1 text-xs rounded bg-muted border border-border text-foreground cursor-pointer",
            disabled: *updating.read(),
            value: "{status_value}",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
            },
            onchange: {
                let event_id = event_id.clone();
                move |e: Event<FormData>| {
                    e.stop_propagation();
                    let new_val = e.value().clone();
                    let eid = event_id.clone();
                    if let Some(new_status) = status_from_string(&new_val) {
                        spawn(async move {
                            updating.set(true);
                            match update_issue_status_by_id(&eid, new_status).await {
                                Ok(_) => {
                                    // Update local items signal to reflect new status
                                    let mut current = items_signal.read().clone();
                                    for item in current.iter_mut() {
                                        if item.event_id() == eid {
                                            match item {
                                                BoardItem::Issue(ref mut i) => i.status = new_status,
                                                BoardItem::PullRequest(ref mut p) => p.status = new_status,
                                            }
                                        }
                                    }
                                    items_signal.set(current);
                                }
                                Err(e) => {
                                    log::error!("Failed to update status: {}", e);
                                }
                            }
                            updating.set(false);
                        });
                    }
                }
            },
            option { value: "draft", selected: current_status == IssueStatus::Draft, "Draft" }
            option { value: "open", selected: current_status == IssueStatus::Open, "Open" }
            option { value: "applied", selected: current_status == IssueStatus::Applied, "Applied" }
            option { value: "closed", selected: current_status == IssueStatus::Closed, "Closed" }
        }
    }
}

// ─── Customize Columns Modal ─────────────────────────────────────────────────

#[component]
fn CustomizeColumnsModal(
    columns: Vec<BoardColumn>,
    naddr: String,
    on_save: EventHandler<Vec<BoardColumn>>,
    on_close: EventHandler<()>,
) -> Element {
    let mut editing = use_signal(|| columns.clone());
    let mut new_col_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        div {
            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm flex items-center justify-center",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-xl p-6 w-full max-w-md mx-4 shadow-xl",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                h3 { class: "text-lg font-bold mb-4", "Customize Board Columns" }

                // Current columns list
                div { class: "space-y-2 mb-4 max-h-64 overflow-y-auto",
                    for (idx, col) in editing.read().iter().enumerate() {
                        div { class: "flex items-center gap-2 p-2 bg-muted/30 rounded-lg",
                            // Color indicator
                            span { class: "w-3 h-3 rounded-full {column_color(col.status.as_deref())}" }

                            // Column name (editable)
                            input {
                                r#type: "text",
                                class: "flex-1 px-2 py-1 text-sm bg-background border border-border rounded",
                                value: "{col.name}",
                                onchange: {
                                    move |e: Event<FormData>| {
                                        let mut cols = editing.read().clone();
                                        if let Some(c) = cols.get_mut(idx) {
                                            c.name = e.value().clone();
                                        }
                                        editing.set(cols);
                                    }
                                },
                            }

                            // Status mapping
                            if col.status.is_some() {
                                span { class: "text-xs text-muted-foreground px-1",
                                    "{col.status.as_deref().unwrap_or(\"\")}"
                                }
                            }

                            // Move up
                            if idx > 0 {
                                button {
                                    class: "p-1 hover:bg-accent rounded transition text-muted-foreground",
                                    onclick: {
                                        move |_| {
                                            let mut cols = editing.read().clone();
                                            if idx > 0 {
                                                cols.swap(idx, idx - 1);
                                                for (i, c) in cols.iter_mut().enumerate() {
                                                    c.order = i as u32;
                                                }
                                                editing.set(cols);
                                            }
                                        }
                                    },
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M18 15l-6-6-6 6" }
                                    }
                                }
                            }

                            // Move down
                            if idx < editing.read().len() - 1 {
                                button {
                                    class: "p-1 hover:bg-accent rounded transition text-muted-foreground",
                                    onclick: {
                                        let len = editing.read().len();
                                        move |_| {
                                            let mut cols = editing.read().clone();
                                            if idx < len - 1 {
                                                cols.swap(idx, idx + 1);
                                                for (i, c) in cols.iter_mut().enumerate() {
                                                    c.order = i as u32;
                                                }
                                                editing.set(cols);
                                            }
                                        }
                                    },
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M6 9l6 6 6-6" }
                                    }
                                }
                            }

                            // Delete (only custom columns without standard status)
                            if col.status.is_none() {
                                button {
                                    class: "p-1 hover:bg-red-500/10 text-red-400 rounded transition",
                                    onclick: {
                                        move |_| {
                                            let mut cols = editing.read().clone();
                                            cols.remove(idx);
                                            for (i, c) in cols.iter_mut().enumerate() {
                                                c.order = i as u32;
                                            }
                                            editing.set(cols);
                                        }
                                    },
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path { d: "M18 6L6 18M6 6l12 12" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Add new column
                div { class: "flex gap-2 mb-4",
                    input {
                        r#type: "text",
                        class: "flex-1 px-3 py-2 text-sm bg-background border border-border rounded-lg",
                        placeholder: "New column name...",
                        value: "{new_col_name.read()}",
                        oninput: move |e: Event<FormData>| new_col_name.set(e.value().clone()),
                    }
                    button {
                        class: "px-3 py-2 text-sm rounded-lg bg-blue-500 text-white hover:bg-blue-600 transition disabled:opacity-50",
                        disabled: new_col_name.read().trim().is_empty(),
                        onclick: move |_| {
                            let name = new_col_name.read().trim().to_string();
                            if !name.is_empty() {
                                let mut cols = editing.read().clone();
                                let id = name.to_lowercase().replace(' ', "-");
                                let order = cols.len() as u32;
                                cols.push(BoardColumn {
                                    id,
                                    name,
                                    order,
                                    status: None,
                                });
                                editing.set(cols);
                                new_col_name.set(String::new());
                            }
                        },
                        "Add"
                    }
                }

                if let Some(err) = error_msg.read().as_ref() {
                    div { class: "text-sm text-red-400 mb-3", "{err}" }
                }

                // Actions
                div { class: "flex items-center gap-2 justify-end",
                    button {
                        class: "px-4 py-2 text-sm rounded-lg border border-border text-muted-foreground hover:bg-accent transition",
                        onclick: move |_| {
                            editing.set(default_columns());
                        },
                        "Reset to Default"
                    }
                    button {
                        class: "px-4 py-2 text-sm rounded-lg border border-border text-muted-foreground hover:bg-accent transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 text-sm rounded-lg bg-blue-500 text-white hover:bg-blue-600 transition disabled:opacity-50",
                        disabled: *saving.read(),
                        onclick: {
                            let naddr = naddr.clone();
                            move |_| {
                                let cols = editing.read().clone();
                                let n = naddr.clone();
                                spawn(async move {
                                    saving.set(true);
                                    error_msg.set(None);
                                    match save_board_layout(&n, &cols).await {
                                        Ok(()) => {
                                            on_save.call(cols);
                                        }
                                        Err(e) => {
                                            error_msg.set(Some(e));
                                        }
                                    }
                                    saving.set(false);
                                });
                            }
                        },
                        if *saving.read() { "Saving..." } else { "Save" }
                    }
                }
            }
        }
    }
}

// ─── Roadmap View ────────────────────────────────────────────────────────────

#[component]
fn RoadmapView(items: Vec<BoardItem>) -> Element {
    // Group items by month (YYYY-MM)
    let mut months: std::collections::BTreeMap<String, Vec<BoardItem>> =
        std::collections::BTreeMap::new();
    for item in items.iter() {
        let ts = item.created_at();
        let month_key = timestamp_to_month(ts);
        months
            .entry(month_key)
            .or_default()
            .push(item.clone());
    }

    if months.is_empty() {
        return rsx! {
            div { class: "text-center py-12 text-muted-foreground text-sm",
                "No items to display on the roadmap."
            }
        };
    }

    rsx! {
        div { class: "overflow-x-auto pb-4",
            div { class: "flex gap-4 min-w-max",
                for (month, month_items) in months.iter() {
                    div { key: "{month}", class: "min-w-64 flex-shrink-0",
                        // Month header
                        div { class: "flex items-center gap-2 mb-3 pb-2 border-b border-border",
                            span { class: "font-semibold text-sm", "{format_month_label(month)}" }
                            span { class: "px-2 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                                "{month_items.len()}"
                            }
                        }

                        // Items in this month
                        div { class: "space-y-2",
                            for item in month_items.iter() {
                                RoadmapItem { key: "{item.event_id()}", item: item.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RoadmapItem(item: BoardItem) -> Element {
    let status = item.status();
    let status_color = match status {
        IssueStatus::Draft => "bg-muted-foreground",
        IssueStatus::Open => "bg-blue-500",
        IssueStatus::Applied => "bg-green-500",
        IssueStatus::Closed => "bg-red-500",
    };

    rsx! {
        Link {
            to: item.detail_route(),
            class: "flex items-start gap-2 p-2 bg-card border border-border rounded-lg hover:bg-accent/50 transition cursor-pointer",

            // Status indicator
            span { class: "w-2.5 h-2.5 rounded-full mt-1 flex-shrink-0 {status_color}" }

            div { class: "min-w-0 flex-1",
                div { class: "text-sm font-medium line-clamp-1", "{item.title()}" }
                div { class: "flex items-center gap-2 text-xs text-muted-foreground mt-0.5",
                    if item.is_pr() {
                        span { class: "text-purple-400", "PR" }
                    } else {
                        span { class: "text-green-400", "Issue" }
                    }
                    span { "{status.label()}" }
                }
            }
        }
    }
}

fn timestamp_to_month(ts: u64) -> String {
    // Convert unix timestamp to YYYY-MM
    let secs = ts as i64;
    let days_since_epoch = secs / 86400;
    // Approximate year/month calculation
    let mut year = 1970i32;
    let mut remaining_days = days_since_epoch;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }
    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &dim in &days_in_months {
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }
    format!("{:04}-{:02}", year, month)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn format_month_label(key: &str) -> String {
    let parts: Vec<&str> = key.split('-').collect();
    if parts.len() == 2 {
        let month_name = match parts[1] {
            "01" => "January",
            "02" => "February",
            "03" => "March",
            "04" => "April",
            "05" => "May",
            "06" => "June",
            "07" => "July",
            "08" => "August",
            "09" => "September",
            "10" => "October",
            "11" => "November",
            "12" => "December",
            _ => parts[1],
        };
        format!("{} {}", month_name, parts[0])
    } else {
        key.to_string()
    }
}

// ─── Empty & Loading States ──────────────────────────────────────────────────

#[component]
fn EmptyBoard() -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect { x: "3", y: "3", width: "7", height: "18", rx: "1" }
                    rect { x: "14", y: "3", width: "7", height: "10", rx: "1" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Items Yet" }
            p { class: "text-muted-foreground text-sm",
                "Create issues or pull requests to see them organized on this board."
            }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "flex gap-4 overflow-x-auto pb-4 animate-pulse",
            for i in 0..4 {
                div { key: "{i}", class: "min-w-72 flex-shrink-0 bg-muted/30 rounded-lg",
                    div { class: "p-3 border-b border-border",
                        div { class: "h-4 bg-muted rounded w-20" }
                    }
                    div { class: "p-2 space-y-2",
                        for j in 0..3 {
                            div { key: "{j}", class: "bg-card border border-border rounded-lg p-3",
                                div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                                div { class: "h-3 bg-muted rounded w-1/2" }
                            }
                        }
                    }
                }
            }
        }
    }
}
