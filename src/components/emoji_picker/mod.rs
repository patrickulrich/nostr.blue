mod data;
mod subdivisions;

use self::data::{
    NativeEmojiEntry, ALL_NATIVE_EMOJIS, CANONICAL_SUBDIVISIONS, NATIVE_EMOJI_CATEGORIES,
    SUBDIVISION_INDEX,
};

use crate::components::EmojiPackManagerModal;
use crate::hooks::{use_infinite_scroll_with_generation, DEFAULT_LONG_PRESS_MS};
use crate::platform::timer::sleep_ms;
use crate::stores::emoji_store::{
    save_recent_emoji, CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt, CUSTOM_EMOJIS,
    EMOJI_SETS, RECENT_EMOJIS,
};
use crate::utils::custom_emoji::EmojiSelection;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Grid cells rendered per incremental chunk in the All/Flags tabs.
const GRID_CHUNK_SIZE: usize = 240;

#[derive(Props, Clone, PartialEq)]
pub struct EmojiPickerProps {
    pub on_emoji_selected: EventHandler<EmojiSelection>,
    #[props(default = false)]
    pub icon_only: bool,
    #[props(default = "Add emoji".to_string())]
    pub aria_label: String,
    #[props(default = false)]
    pub disabled: bool,
}

#[derive(Clone, PartialEq)]
enum EmojiCategory {
    Recent,
    Custom,
    Set(String),
    All,
    Standard(usize),
}

#[derive(Clone, PartialEq)]
enum SearchResult {
    Native(NativeEmojiEntry),
    Custom {
        shortcode: String,
        url: String,
        pack_coordinate: Option<String>,
    },
}

/// Builds the per-flag renderability probe script. Every subdivision glyph is
/// rendered into one offscreen canvas grid alongside two fallback reference
/// cells (the bare black flag 🏴 and a synthesized invalid-code tag sequence
/// that no font has artwork for). A glyph counts as renderable when its pixels
/// match neither reference, the cell is not blank, and its measured width
/// stays within the black-flag width (Windows renders tag characters as
/// letters/tofu that overflow). Returns a compact "1"/"0" string aligned with
/// `CANONICAL_SUBDIVISIONS`; any canvas failure inside the JS fails open
/// ("1" for every glyph: no demotion, no labels).
fn subdivision_render_probe_script(glyphs: &[&str]) -> String {
    let payload = serde_json::to_string(glyphs).unwrap_or_else(|_| "[]".to_string());
    format!(
        r#"return (() => {{
    const GLYPHS = {payload};
    const failOpen = '1'.repeat(GLYPHS.length);
    const CELL = 48, COLS = 85, FONT = '32px sans-serif';
    const BLACK_FLAG = "\u{{1F3F4}}";
    const UNKNOWN_REF = "\u{{1F3F4}}\u{{E007A}}\u{{E007A}}\u{{E007A}}\u{{E007A}}\u{{E007F}}";
    try {{
        const canvas = document.createElement('canvas');
        const rows = Math.ceil((GLYPHS.length + 2) / COLS);
        canvas.width = COLS * CELL;
        canvas.height = rows * CELL;
        const ctx = canvas.getContext('2d', {{ willReadFrequently: true }});
        if (!ctx || canvas.width === 0 || canvas.height === 0) return failOpen;
        ctx.font = FONT;
        ctx.textBaseline = 'top';
        ctx.fillStyle = '#000';
        const cellX = (i) => (i % COLS) * CELL;
        const cellY = (i) => Math.floor(i / COLS) * CELL;
        ctx.fillText(BLACK_FLAG, cellX(0), cellY(0));
        ctx.fillText(UNKNOWN_REF, cellX(1), cellY(1));
        const cellData = (i) => ctx.getImageData(cellX(i), cellY(i), CELL, CELL).data;
        const flagRef = cellData(0);
        const unknownRef = cellData(1);
        const flagWidth = ctx.measureText(BLACK_FLAG).width;
        const equals = (a, b) => {{
            for (let i = 0; i < a.length; i++) {{ if (a[i] !== b[i]) return false; }}
            return true;
        }};
        const isBlank = (a) => {{
            for (let i = 3; i < a.length; i += 4) {{ if (a[i] !== 0) return false; }}
            return true;
        }};
        let out = '';
        for (let i = 0; i < GLYPHS.length; i++) {{
            ctx.fillText(GLYPHS[i], cellX(i + 2), cellY(i + 2));
            const data = cellData(i + 2);
            const widthOk = ctx.measureText(GLYPHS[i]).width <= flagWidth + 1.5
                && ctx.measureText(GLYPHS[i]).width > 0;
            const supported = !equals(data, flagRef)
                && !equals(data, unknownRef)
                && !isBlank(data)
                && widthOk;
            out += supported ? '1' : '0';
        }}
        return out;
    }} catch (err) {{
        return failOpen;
    }}
}})()"#
    )
}

/// Parse the probe's compact "1"/"0" response. `None` on length mismatch or
/// unexpected characters (callers fail open).
fn parse_probe_result(value: &serde_json::Value, expected: usize) -> Option<Vec<bool>> {
    let raw = value.as_str()?;
    if raw.len() != expected {
        return None;
    }
    raw.bytes()
        .map(|byte| match byte {
            b'1' => Some(true),
            b'0' => Some(false),
            _ => None,
        })
        .collect()
}

/// Flags-category ordering with per-flag renderability applied: entries whose
/// glyphs render keep the canonical interleaved order; probe-verified
/// unrenderable subdivisions are demoted to the end (blocks keep their
/// internal order). `None` (probe pending/failed) yields the canonical order.
fn partition_flags_by_support(
    items: &[NativeEmojiEntry],
    support: Option<&[bool]>,
) -> Vec<NativeEmojiEntry> {
    let Some(support) = support else {
        return items.to_vec();
    };
    let mut next_subdivision = 0usize;
    let mut renderable = Vec::with_capacity(items.len());
    let mut demoted = Vec::new();
    for entry in items {
        let renders = if entry.is_subdivision {
            let renders = support.get(next_subdivision).copied().unwrap_or(true);
            next_subdivision += 1;
            renders
        } else {
            true
        };
        if renders {
            renderable.push(entry.clone());
        } else {
            demoted.push(entry.clone());
        }
    }
    renderable.extend(demoted);
    renderable
}

/// Whether this entry's glyph was verified unrenderable by the probe. Pending
/// or failed probes (and non-subdivisions) are never flagged.
fn entry_unrenderable(entry: &NativeEmojiEntry, support: Option<&[bool]>) -> bool {
    if !entry.is_subdivision {
        return false;
    }
    let Some(support) = support else {
        return false;
    };
    let index = SUBDIVISION_INDEX
        .get(&entry.glyph)
        .copied()
        .unwrap_or(usize::MAX);
    !support.get(index).copied().unwrap_or(true)
}


fn installed_custom_search_items() -> Vec<(String, String, Option<String>)> {
    let custom_store = CUSTOM_EMOJIS.read();
    let custom_data = custom_store.data();
    let custom_list = custom_data.read();

    let sets_store = EMOJI_SETS.read();
    let sets_data = sets_store.data();
    let sets_list = sets_data.read();

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for emoji in custom_list.iter() {
        if seen.insert((emoji.shortcode.clone(), emoji.image_url.clone(), None)) {
            items.push((emoji.shortcode.clone(), emoji.image_url.clone(), None));
        }
    }

    for set in sets_list.iter() {
        let coordinate = format!("30030:{}:{}", set.author, set.identifier);
        for emoji in &set.emojis {
            if seen.insert((
                emoji.shortcode.clone(),
                emoji.image_url.clone(),
                Some(coordinate.clone()),
            )) {
                items.push((
                    emoji.shortcode.clone(),
                    emoji.image_url.clone(),
                    Some(coordinate.clone()),
                ));
            }
        }
    }

    items
}

fn resolve_recent_selection(
    entry: &str,
    installed_custom_map: &HashMap<String, (String, Option<String>)>,
) -> Option<EmojiSelection> {
    if !is_valid_http_url(entry) {
        return Some(EmojiSelection::Native {
            emoji: entry.to_string(),
        });
    }

    if let Some((shortcode, pack_coordinate)) = installed_custom_map.get(entry) {
        return Some(EmojiSelection::Custom {
            shortcode: shortcode.clone(),
            url: entry.to_string(),
            pack_coordinate: pack_coordinate.clone(),
        });
    }

    None
}

fn emoji_selection_shortcode(selection: &EmojiSelection) -> String {
    match selection {
        EmojiSelection::Native { emoji } => emoji.clone(),
        EmojiSelection::Custom { shortcode, .. } => shortcode.clone(),
    }
}

fn custom_result_rank(shortcode: &str, query: &str) -> i32 {
    if shortcode == query {
        0
    } else if shortcode.starts_with(query) {
        1
    } else if shortcode.contains(query) {
        2
    } else {
        3
    }
}

/// Relevance tier for a native emoji against a lowercase query. Lower ranks
/// first: (0) exact shortcode or exact `flag:`-stripped name, (1) any prefix
/// (shortcode or name word), (2) substring anywhere. `None` = no match.
fn native_search_tier(entry: &NativeEmojiEntry, query: &str) -> Option<u8> {
    let name_lower = entry.name.to_lowercase();
    let bare = name_lower.strip_prefix("flag: ").unwrap_or(&name_lower);
    if bare == query || entry.shortcodes.iter().any(|code| code == query) {
        return Some(0);
    }
    if entry.shortcodes.iter().any(|code| code.starts_with(query))
        || bare.split_whitespace().any(|word| word.starts_with(query))
    {
        return Some(1);
    }
    if name_lower.contains(query) || entry.shortcodes.iter().any(|code| code.contains(query)) {
        return Some(2);
    }
    None
}

/// All native emojis matching `query`, best first. Tie-break chain after the
/// relevance tier: countries before subdivisions, shallower containment
/// depth first (states above counties), then alphabetical by name.
fn ranked_native_matches(query: &str, support: Option<&[bool]>) -> Vec<NativeEmojiEntry> {
    let mut matches: Vec<(u8, &NativeEmojiEntry)> = ALL_NATIVE_EMOJIS
        .iter()
        .filter_map(|entry| {
            native_search_tier(entry, query).map(|tier| (tier, entry))
        })
        .collect();
    matches.sort_by(|a, b| {
        a.0.cmp(&b.0)
            // Same demotion as the grids: within a relevance tier, flags the
            // probe verified as unrenderable sink below the renderable ones
            // ("ky" -> Cayman Islands and Kentucky above Kayanza on a font
            // without Burundi artwork). Pending/failed probes demote nothing.
            .then(entry_unrenderable(a.1, support).cmp(&entry_unrenderable(b.1, support)))
            .then(a.1.is_subdivision.cmp(&b.1.is_subdivision))
            .then(a.1.depth.cmp(&b.1.depth))
            // Shorter names first within a tier: closer to the query ("ky" ->
            // Kentucky before Guna Yala, both US-KY-style exact segments).
            .then(a.1.name.len().cmp(&b.1.name.len()))
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    matches.into_iter().map(|(_, entry)| entry.clone()).collect()
}

/// Tooltip text for a native emoji cell: name (plus ISO code for subdivision
/// flags) and the skin-tone hint when applicable.
fn emoji_tooltip(entry: &NativeEmojiEntry, tone_hint: &str) -> String {
    let mut title = if entry.is_subdivision {
        format!(
            "{} ({})",
            entry.name,
            entry.label.as_deref().unwrap_or_default()
        )
    } else {
        entry.name.clone()
    };
    if entry.has_tones {
        title.push_str(tone_hint);
    }
    title
}

/// All skin-tone variants for a base emoji glyph (base first), or an empty vec
/// when the emoji has none. Handles the 6-variant single-person case and the
/// up-to-26 two-person combinations.
fn native_tone_variants(glyph: &str) -> Vec<String> {
    emojis::get(glyph)
        .and_then(|emoji| emoji.skin_tones())
        .map(|tones| tones.map(|t| t.as_str().to_string()).collect())
        .unwrap_or_default()
}

/// Long-press completion: open the tone menu for the pressed glyph (if it has
/// variants) and mark the press as consumed so the synthesized `click` on
/// finger-lift is suppressed.
fn complete_tone_long_press(
    glyph: Option<String>,
    mut tone_menu: Signal<Option<Vec<String>>>,
    mut tone_press_consumed: Signal<bool>,
) {
    let Some(glyph) = glyph else { return };
    let tones = native_tone_variants(&glyph);
    if !tones.is_empty() {
        tone_menu.set(Some(tones));
        tone_press_consumed.set(true);
    }
}

/// Cancel a pending long-press (touchmove / touchend / touchcancel).
fn cancel_tone_press(mut generation: Signal<u32>) {
    generation.with_mut(|g| *g = g.wrapping_add(1));
}

/// End a touch press; if a long-press already opened the tone menu, suppress
/// the follow-up click event the browser would otherwise synthesize.
fn end_tone_press(
    evt: TouchEvent,
    generation: Signal<u32>,
    mut consumed: Signal<bool>,
) {
    if *consumed.read() {
        evt.prevent_default();
        consumed.set(false);
    }
    cancel_tone_press(generation);
}

#[component]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut show_manage_modal = use_signal(|| false);
    let mut selected_category = use_signal(|| EmojiCategory::Recent);
    let mut search_query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut position_below = use_signal(|| false);
    let button_id = use_signal(|| format!("emoji-picker-{}", uuid::Uuid::new_v4()));
    #[allow(unused_mut)]
    let mut picker_top = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_bottom = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_left = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut is_mobile = use_signal(|| false);
    let mut failed_images: Signal<HashSet<String>> = use_signal(HashSet::new);
    let mut tone_menu: Signal<Option<Vec<String>>> = use_signal(|| None);
    let mut tone_press_generation = use_signal(|| 0u32);
    // `None` until the canvas probe resolves (or its 100ms timeout fires).
    // `Some(false)` enables the "US-KY" fallback labels under subdivision
    // glyphs on platforms whose emoji font cannot render tag sequences.
    // Per-flag renderability verdicts from the font probe, aligned with
    // CANONICAL_SUBDIVISIONS. `None` until the probe resolves (or its 500ms
    // timeout fails open). The Flags/All grids hold their first paint until
    // this is `Some`, so the demotion ordering lands without a reflow.
    let subdivision_render_support: Signal<Option<Vec<bool>>> = use_signal(|| None);
    // Incremental grid rendering for the two large tabs (All / Flags): DOM
    // stays bounded while the user scrolls through thousands of cells.
    let mut grid_chunk = use_signal(|| GRID_CHUNK_SIZE);
    let grid_loading = use_signal(|| false);
    let mut grid_has_more = use_signal(|| false);
    let mut grid_generation = use_signal(|| 0u64);
    let sentinel_id = use_infinite_scroll_with_generation(
        move || {
            grid_chunk.with_mut(|chunk| *chunk = chunk.saturating_add(GRID_CHUNK_SIZE));
        },
        grid_has_more,
        grid_loading,
        grid_generation,
    );
    let mut tone_pending_glyph: Signal<Option<String>> = use_signal(|| None);
    let tone_press_consumed = use_signal(|| false);
    let tone_hint = if cfg!(feature = "mobile_platform") {
        " — long-press for skin tones"
    } else {
        " — right-click or long-press for skin tones"
    };

    let custom_emojis = CUSTOM_EMOJIS.read();
    let emoji_sets = EMOJI_SETS.read();
    let recent_emojis = RECENT_EMOJIS.read();

    let installed_custom_items = use_memo(installed_custom_search_items);
    let installed_custom_map = use_memo(move || {
        installed_custom_items
            .read()
            .iter()
            .map(|(shortcode, url, pack_coordinate)| {
                (url.clone(), (shortcode.clone(), pack_coordinate.clone()))
            })
            .collect::<HashMap<_, _>>()
    });
    let search_lower = use_memo(move || search_query.read().trim().to_lowercase());
    let is_searching = !search_lower.read().is_empty();

    let search_results = use_memo(move || {
        let query = search_lower.read();
        // Subscribe to the probe so results re-rank when renderability
        // verdicts land (demotion inside relevance tiers).
        let support = subdivision_render_support.read();
        if query.is_empty() {
            return Vec::<SearchResult>::new();
        }

        let mut results = installed_custom_items
            .read()
            .iter()
            .filter(|(shortcode, _, _)| shortcode.to_lowercase().contains(query.as_str()))
            .map(|(shortcode, url, pack_coordinate)| SearchResult::Custom {
                shortcode: shortcode.clone(),
                url: url.clone(),
                pack_coordinate: pack_coordinate.clone(),
            })
            .collect::<Vec<_>>();

        results.extend(
            ranked_native_matches(query.as_str(), support.as_deref())
                .into_iter()
                .map(SearchResult::Native),
        );
        results.sort_by(|a, b| match (a, b) {
            (
                SearchResult::Custom { shortcode: a, .. },
                SearchResult::Custom { shortcode: b, .. },
            ) => custom_result_rank(a, query.as_str()).cmp(&custom_result_rank(b, query.as_str())),
            (SearchResult::Custom { .. }, SearchResult::Native(_)) => Ordering::Less,
            (SearchResult::Native(_), SearchResult::Custom { .. }) => Ordering::Greater,
            // Natives are pre-ranked by `ranked_native_matches`; the stable
            // sort preserves that order.
            (SearchResult::Native(_), SearchResult::Native(_)) => Ordering::Equal,
        });
        results.truncate(80);
        results
    });

    // Keep the active category tab scrolled into view in the strip (#378) —
    // with a dozen default tabs the strip always overflows, so selection would
    // otherwise land off-screen.
    use_effect(move || {
        let _ = selected_category.read();
        let _ = document::eval(
            r#"document.getElementById('emoji-category-strip')?.querySelector('.emoji-tab-active')?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' })"#,
        );
    });

    // Reset the visible chunk when the picker (re)opens, the category
    // changes, or the font probe lands (the grid order changes): the popup
    // DOM (sentinel included) is re-created each open, so the infinite-scroll
    // hook must re-attach to the fresh element.
    use_effect(move || {
        let _ = show_picker.read();
        let _ = selected_category.read();
        let _ = subdivision_render_support.read();
        grid_chunk.set(GRID_CHUNK_SIZE);
        grid_generation.with_mut(|generation| *generation = generation.wrapping_add(1));
    });

    // Keep `has_more` in sync with the active tab's total item count (the
    // demotion partition preserves counts, so the static totals stay valid).
    use_effect(move || {
        let category = selected_category.read().clone();
        let chunk = *grid_chunk.read();
        let total = match &category {
            EmojiCategory::Standard(idx) => NATIVE_EMOJI_CATEGORIES
                .get(*idx)
                .map(|(_, items)| items.len())
                .unwrap_or(0),
            EmojiCategory::All => ALL_NATIVE_EMOJIS.len(),
            _ => 0,
        };
        grid_has_more.set(total > chunk);
    });

    // Demotion ordering: renderable entries keep the canonical interleave;
    // probe-verified unrenderable subdivisions move to the end.
    let visible_flags = use_memo(move || {
        let support = subdivision_render_support.read();
        partition_flags_by_support(&NATIVE_EMOJI_CATEGORIES[8].1, support.as_deref())
    });
    let visible_all = use_memo(move || {
        let mut items: Vec<NativeEmojiEntry> = NATIVE_EMOJI_CATEGORIES
            .iter()
            .take(8)
            .flat_map(|(_, items)| items.iter().cloned())
            .collect();
        items.extend(visible_flags.read().iter().cloned());
        items
    });

    // Per-flag fallback labels: only entries the probe verified unrenderable
    // (pending/failed probes label nothing).
    let render_support = subdivision_render_support.read().clone();

    let mut emit_selection = move |selection: EmojiSelection| {
        match &selection {
            EmojiSelection::Native { emoji } => save_recent_emoji(emoji.clone()),
            EmojiSelection::Custom { url, .. } => save_recent_emoji(url.clone()),
        }
        props.on_emoji_selected.call(selection);
        show_picker.set(false);
        tone_menu.set(None);
        search_query.set(String::new());
    };

    rsx! {
        div { class: "relative",
            button {
                id: "{button_id}",
                class: if props.disabled {
                    if props.icon_only { "p-2 rounded-full opacity-50 cursor-not-allowed" } else { "px-3 py-2 bg-muted text-foreground rounded-lg text-sm font-medium opacity-50 cursor-not-allowed" }
                } else {
                    if props.icon_only { "p-2 rounded-full hover:bg-accent transition" } else { "px-3 py-2 bg-muted text-foreground hover:bg-accent rounded-lg text-sm font-medium transition" }
                },
                title: if props.icon_only { "Add emoji" } else { "" },
                aria_label: if props.icon_only { "{props.aria_label}" } else { "" },
                disabled: props.disabled,
                onclick: move |_| {
                    if props.disabled {
                        return;
                    }
                    let current = *show_picker.read();
                    show_picker.set(!current);
                    if !current {
                        // One-time per-flag renderability probe: drives the
                        // demotion ordering and fallback labels.
                        if subdivision_render_support.read().is_none() {
                            let expected = CANONICAL_SUBDIVISIONS.len();
                            let script = subdivision_render_probe_script(
                                &CANONICAL_SUBDIVISIONS
                                    .iter()
                                    .map(|entry| entry.glyph.as_str())
                                    .collect::<Vec<_>>(),
                            );
                            let mut probe_result = subdivision_render_support;
                            spawn(async move {
                                let verdict = match document::eval(&script).await {
                                    Ok(value) => parse_probe_result(&value, expected)
                                        .unwrap_or_else(|| vec![true; expected]),
                                    // Fail open: assume renderable, no labels.
                                    Err(_) => vec![true; expected],
                                };
                                probe_result.set(Some(verdict));
                            });
                            spawn(async move {
                                sleep_ms(500).await;
                                if probe_result.read().is_none() {
                                    // Fail open so the held grids paint.
                                    probe_result.set(Some(vec![true; expected]));
                                }
                            });
                        }
                        #[cfg(feature = "web")]
                        {
                            let btn_id = button_id.read().clone();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(element) = document.get_element_by_id(&btn_id) {
                                        let rect = element.get_bounding_client_rect();
                                        let viewport_width = window.inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(1024.0);
                                        let viewport_height = window.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(800.0);
                                        let is_mobile_view = viewport_width < 640.0;
                                        is_mobile.set(is_mobile_view);
                                        if is_mobile_view {
                                            picker_top.set(16.0);
                                            position_below.set(true);
                                        } else {
                                            let button_center_y = rect.top() + (rect.height() / 2.0);
                                            let is_in_top_half = button_center_y < (viewport_height / 2.0);
                                            picker_left.set(rect.left());
                                            if is_in_top_half {
                                                picker_top.set(rect.bottom() + 8.0);
                                                position_below.set(true);
                                            } else {
                                                picker_bottom.set(viewport_height - rect.top() + 8.0);
                                                position_below.set(false);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        {
                            is_mobile.set(false);
                            position_below.set(true);
                            picker_top.set(16.0);
                            picker_left.set(16.0);
                            picker_bottom.set(0.0);
                        }
                    }
                },
                if props.icon_only {
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "10" }
                        path { d: "M8 14s1.5 2 4 2 4-2 4-2" }
                        line { x1: "9", y1: "9", x2: "9.01", y2: "9" }
                        line { x1: "15", y1: "9", x2: "15.01", y2: "9" }
                    }
                } else {
                    "😀 Emoji"
                }
            }
            if *show_picker.read() {
                div {
                    class: "fixed inset-0 z-[60]",
                    onclick: move |_| {
                        tone_menu.set(None);
                        show_picker.set(false);
                    },
                    div {
                        class: "fixed bg-background border border-border rounded-lg shadow-xl flex flex-col",
                        class: "w-[calc(100vw-2rem)] sm:w-[25rem]",
                        style: if *is_mobile.read() {
                            "top: 1rem; left: 1rem; right: 1rem; height: min(28rem, calc(100vh - 2rem)); max-height: calc(100vh - 2rem);".to_string()
                        } else if *position_below.read() {
                            format!("top: {}px; left: {}px; height: min(28rem, calc(100vh - {}px - 1rem)); max-height: min(36rem, calc(100vh - {}px - 1rem));", *picker_top.read(), *picker_left.read(), *picker_top.read(), *picker_top.read())
                        } else {
                            format!("bottom: {}px; left: {}px; height: min(28rem, calc(100vh - {}px - 1rem)); max-height: min(36rem, calc(100vh - {}px - 1rem));", *picker_bottom.read(), *picker_left.read(), *picker_bottom.read(), *picker_bottom.read())
                        },
                        onclick: move |e| e.stop_propagation(),
                        div { class: "flex items-center justify-between p-3 border-b border-border",
                            h3 { class: "text-sm font-semibold", "Select Emoji" }
                            button {
                                class: "text-muted-foreground hover:text-foreground",
                                onclick: move |_| {
                                    tone_menu.set(None);
                                    show_picker.set(false);
                                },
                                "✕"
                            }
                        }
                        div { class: "p-2 border-b border-border",
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 text-sm bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "Search emojis and packs...",
                                value: "{search_query}",
                                oninput: move |evt| search_query.set(evt.value()),
                            }
                        }
                        if !is_searching {
                            div {
                                id: "emoji-category-strip",
                                class: "flex gap-1 p-2 border-b border-border overflow-x-auto overscroll-contain",
                                button {
                                    class: if *selected_category.read() == EmojiCategory::Recent { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap shrink-0 emoji-tab-active" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs font-medium whitespace-nowrap shrink-0" },
                                    onclick: move |_| selected_category.set(EmojiCategory::Recent),
                                    "🕐 Recent"
                                }
                                if !custom_emojis.data().read().is_empty() || !emoji_sets.data().read().is_empty() {
                                    button {
                                        class: if *selected_category.read() == EmojiCategory::Custom { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap shrink-0 emoji-tab-active" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs font-medium whitespace-nowrap shrink-0" },
                                        onclick: move |_| selected_category.set(EmojiCategory::Custom),
                                        "⭐ Custom"
                                    }
                                }
                                for set in emoji_sets.data().read().iter() {
                                    {
                                        let coordinate = format!("30030:{}:{}", set.author, set.identifier);
                                        let display_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                        rsx! {
                                            button {
                                                key: "set-{coordinate}",
                                                class: if *selected_category.read() == EmojiCategory::Set(coordinate.clone()) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium truncate max-w-[8rem] shrink-0 emoji-tab-active" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs font-medium truncate max-w-[8rem] shrink-0" },
                                                onclick: move |_| selected_category.set(EmojiCategory::Set(coordinate.clone())),
                                                "📦 {display_name}"
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: if *selected_category.read() == EmojiCategory::All { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap shrink-0 emoji-tab-active" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs font-medium whitespace-nowrap shrink-0" },
                                    onclick: move |_| selected_category.set(EmojiCategory::All),
                                    "🌐 All"
                                }
                                for (idx, (category_name, _)) in NATIVE_EMOJI_CATEGORIES.iter().enumerate() {
                                    button {
                                        key: "std-{idx}",
                                        class: if *selected_category.read() == EmojiCategory::Standard(idx) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap shrink-0 emoji-tab-active" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs font-medium whitespace-nowrap shrink-0" },
                                        onclick: move |_| selected_category.set(EmojiCategory::Standard(idx)),
                                        "{category_name}"
                                    }
                                }
                            }
                        }
                        div { class: "flex-1 overflow-y-auto p-3",
                            if is_searching {
                                if search_results.read().is_empty() {
                                    div { class: "py-8 text-center text-sm text-muted-foreground",
                                        "No emoji matches found."
                                    }
                                } else {
                                    div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                        for (index, result) in search_results.read().iter().enumerate() {
                                            match result {
                                                SearchResult::Native(emoji) => {
                                                    let glyph = emoji.glyph.clone();
                                                    let title = emoji_tooltip(emoji, tone_hint);
                                                    let sub_label = if entry_unrenderable(emoji, render_support.as_deref()) {
                                                        emoji.label.clone()
                                                    } else {
                                                        None
                                                    };
                                                    let click_glyph = glyph.clone();
                                                    let touch_glyph = glyph.clone();
                                                    let context_glyph = glyph.clone();
                                                    rsx! {
                                                        button {
                                                            key: "search-native-{index}",
                                                            class: "rounded-lg p-2 text-2xl hover:bg-accent transition",
                                                            title: "{title}",
                                                            onclick: move |_| {
                                                                emit_selection(EmojiSelection::Native { emoji: click_glyph.clone() });
                                                            },
                                                            ontouchstart: {
                                                                let tone_glyph = touch_glyph.clone();
                                                                move |_| {
                                                                    tone_pending_glyph.set(Some(tone_glyph.clone()));
                                                                    let token = tone_press_generation.peek().wrapping_add(1);
                                                                    tone_press_generation.set(token);
                                                                    spawn(async move {
                                                                        sleep_ms(DEFAULT_LONG_PRESS_MS).await;
                                                                        if *tone_press_generation.peek() == token {
                                                                            complete_tone_long_press(
                                                                                tone_pending_glyph.peek().clone(),
                                                                                tone_menu,
                                                                                tone_press_consumed,
                                                                            );
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            ontouchmove: move |_| cancel_tone_press(tone_press_generation),
                                                            ontouchend: move |evt: TouchEvent| end_tone_press(evt, tone_press_generation, tone_press_consumed),
                                                            ontouchcancel: move |_| cancel_tone_press(tone_press_generation),
                                                            oncontextmenu: {
                                                                let tone_glyph = context_glyph.clone();
                                                                move |e: MouseEvent| {
                                                                    if cfg!(feature = "mobile_platform") {
                                                                        return;
                                                                    }
                                                                    e.prevent_default();
                                                                    e.stop_propagation();
                                                                    let tones = native_tone_variants(&tone_glyph);
                                                                    if !tones.is_empty() {
                                                                        tone_menu.set(Some(tones));
                                                                    }
                                                                }
                                                            },
                                                            "{glyph}"
                                                            if let Some(label_text) = &sub_label {
                                                                span { class: "block text-[9px] leading-none text-muted-foreground text-center mt-0.5 truncate",
                                                                    "{label_text}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                SearchResult::Custom { shortcode, url, pack_coordinate } => {
                                                    let shortcode_value = shortcode.clone();
                                                    let url_value = url.clone();
                                                    let title = format!(":{shortcode_value}:");
                                                    let invalid_url = !is_valid_http_url(url);
                                                    let transient_failed = failed_images.read().contains(url);
                                                    let has_error = invalid_url || transient_failed;
                                                    let pack_ref = pack_coordinate.clone();
                                                    rsx! {
                                                        button {
                                                            key: "search-custom-{index}",
                                                            class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                            title: "{title}",
                                                            onclick: move |_| {
                                                                if invalid_url {
                                                                    return;
                                                                }
                                                                emit_selection(EmojiSelection::Custom {
                                                                    shortcode: shortcode_value.clone(),
                                                                    url: url_value.clone(),
                                                                    pack_coordinate: pack_ref.clone(),
                                                                });
                                                            },
                                                            if has_error {
                                                                span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                            } else {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: ":{shortcode}:",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: {
                                                                        let url = url.clone();
                                                                        move |_| {
                                                                            failed_images.write().insert(url.clone());
                                                                        }
                                                                    },
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                match selected_category.read().clone() {
                                    EmojiCategory::Recent => rsx! {
                                        div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                            for (emoji_idx, (entry, selection)) in recent_emojis
                                                .iter()
                                                .filter_map(|emoji| resolve_recent_selection(emoji, &installed_custom_map.read()).map(|selection| (emoji.clone(), selection)))
                                                .enumerate()
                                            {
                                                {
                                                    {
                                                        let selection_for_click = selection.clone();
                                                        let has_error = is_valid_http_url(&entry) && failed_images.read().contains(&entry);
                                                        let fallback_text = emoji_selection_shortcode(&selection);
                                                        rsx! {
                                                            button {
                                                                key: "recent-{emoji_idx}",
                                                                class: "rounded-lg p-2 hover:bg-accent transition flex items-center justify-center",
                                                                onclick: move |_| emit_selection(selection_for_click.clone()),
                                                                if is_valid_http_url(&entry) {
                                                                    if has_error {
                                                                        span { class: "text-xs text-muted-foreground", ":{fallback_text}:" }
                                                                    } else {
                                                                        img {
                                                                            src: "{entry}",
                                                                            alt: "recent custom emoji",
                                                                            class: "w-8 h-8 object-contain",
                                                                            loading: "lazy",
                                                                            onerror: {
                                                                                let entry = entry.clone();
                                                                                move |_| {
                                                                                    failed_images.write().insert(entry.clone());
                                                                                }
                                                                            },
                                                                        }
                                                                    }
                                                                } else {
                                                                    span { class: "text-2xl", "{entry}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if recent_emojis
                                                .iter()
                                                .all(|emoji| resolve_recent_selection(emoji, &installed_custom_map.read()).is_none())
                                            {
                                                p { class: "col-span-full text-center text-muted-foreground text-sm py-4",
                                                    "No recent emojis yet. Select some emojis to see them here!"
                                                }
                                            }
                                        }
                                    },
                                    EmojiCategory::Custom => rsx! {
                                        div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                            for (index, (shortcode, url, pack_coordinate)) in installed_custom_items.read().iter().enumerate() {
                                                {
                                                    let shortcode_value = shortcode.clone();
                                                    let url_value = url.clone();
                                                    let pack_ref = pack_coordinate.clone();
                                                    let invalid_url = !is_valid_http_url(url);
                                                    let transient_failed = failed_images.read().contains(url);
                                                    let has_error = invalid_url || transient_failed;
                                                    rsx! {
                                                        button {
                                                            key: "custom-{index}",
                                                            class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                            title: ":{shortcode_value}:",
                                                            onclick: move |_| {
                                                                if invalid_url {
                                                                    return;
                                                                }
                                                                emit_selection(EmojiSelection::Custom {
                                                                    shortcode: shortcode_value.clone(),
                                                                    url: url_value.clone(),
                                                                    pack_coordinate: pack_ref.clone(),
                                                                });
                                                            },
                                                            if has_error {
                                                                span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                            } else {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: ":{shortcode}:",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: {
                                                                        let url = url.clone();
                                                                        move |_| {
                                                                            failed_images.write().insert(url.clone());
                                                                        }
                                                                    },
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    EmojiCategory::Set(coordinate) => {
                                        let sets_data = emoji_sets.data();
                                        let sets_guard = sets_data.read();
                                        let set = sets_guard.iter().find(|s| format!("30030:{}:{}", s.author, s.identifier) == coordinate);
                                        rsx! {
                                            div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                                if let Some(set) = set {
                                                    for (index, custom_emoji) in set.emojis.iter().enumerate() {
                                                        {
                                                            let shortcode = custom_emoji.shortcode.clone();
                                                            let url = custom_emoji.image_url.clone();
                                                            let pack_coordinate = Some(format!("30030:{}:{}", set.author, set.identifier));
                                                            let invalid_url = !is_valid_http_url(&url);
                                                            let transient_failed = failed_images.read().contains(&url);
                                                            let has_error = invalid_url || transient_failed;
                                                            rsx! {
                                                                button {
                                                                    key: "set-{coordinate}-{index}",
                                                                    class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                                    title: ":{shortcode}:",
                                                                    onclick: move |_| {
                                                                        if invalid_url {
                                                                            return;
                                                                        }
                                                                        emit_selection(EmojiSelection::Custom {
                                                                            shortcode: shortcode.clone(),
                                                                            url: url.clone(),
                                                                            pack_coordinate: pack_coordinate.clone(),
                                                                        });
                                                                    },
                                                                    if has_error {
                                                                        span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                                    } else {
                                                                        img {
                                                                            src: "{url}",
                                                                            alt: ":{shortcode}:",
                                                                            class: "w-8 h-8 object-contain",
                                                                            loading: "lazy",
                                                                            onerror: {
                                                                                let url = url.clone();
                                                                                move |_| {
                                                                                    failed_images.write().insert(url.clone());
                                                                                }
                                                                            },
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    p { class: "col-span-full text-center text-muted-foreground text-sm py-4",
                                                        "Emoji set not found"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    EmojiCategory::All => {
                                        // Hold first paint until the font probe
                                        // resolves (bounded by its 500ms
                                        // fail-open timeout) so the demotion
                                        // ordering lands without a reflow.
                                        let support_pending =
                                            subdivision_render_support.read().is_none();
                                        let visible = visible_all.read();
                                        rsx! {
                                            if support_pending {
                                                div { class: "py-10 text-center text-sm text-muted-foreground",
                                                    "Loading flags…"
                                                }
                                            } else {
                                                div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                                    for (index, emoji) in visible.iter().take(*grid_chunk.read()).enumerate() {
                                                {
                                                    let glyph = emoji.glyph.clone();
                                                    let title = emoji_tooltip(emoji, tone_hint);
                                                    let sub_label = if entry_unrenderable(emoji, render_support.as_deref()) {
                                                        emoji.label.clone()
                                                    } else {
                                                        None
                                                    };
                                                    let click_glyph = glyph.clone();
                                                    let touch_glyph = glyph.clone();
                                                    let context_glyph = glyph.clone();
                                                    rsx! {
                                                        button {
                                                            key: "all-{index}",
                                                            class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                            title: "{title}",
                                                            onclick: move |_| emit_selection(EmojiSelection::Native { emoji: click_glyph.clone() }),
                                                            ontouchstart: {
                                                                let tone_glyph = touch_glyph.clone();
                                                                move |_| {
                                                                    tone_pending_glyph.set(Some(tone_glyph.clone()));
                                                                    let token = tone_press_generation.peek().wrapping_add(1);
                                                                    tone_press_generation.set(token);
                                                                    spawn(async move {
                                                                        sleep_ms(DEFAULT_LONG_PRESS_MS).await;
                                                                        if *tone_press_generation.peek() == token {
                                                                            complete_tone_long_press(
                                                                                tone_pending_glyph.peek().clone(),
                                                                                tone_menu,
                                                                                tone_press_consumed,
                                                                            );
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            ontouchmove: move |_| cancel_tone_press(tone_press_generation),
                                                            ontouchend: move |evt: TouchEvent| end_tone_press(evt, tone_press_generation, tone_press_consumed),
                                                            ontouchcancel: move |_| cancel_tone_press(tone_press_generation),
                                                            oncontextmenu: {
                                                                let tone_glyph = context_glyph.clone();
                                                                move |e: MouseEvent| {
                                                                    if cfg!(feature = "mobile_platform") {
                                                                        return;
                                                                    }
                                                                    e.prevent_default();
                                                                    e.stop_propagation();
                                                                    let tones = native_tone_variants(&tone_glyph);
                                                                    if !tones.is_empty() {
                                                                        tone_menu.set(Some(tones));
                                                                    }
                                                                }
                                                            },
                                                            "{glyph}"
                                                            if let Some(label_text) = &sub_label {
                                                                span { class: "block text-[9px] leading-none text-muted-foreground text-center mt-0.5 truncate",
                                                                    "{label_text}"
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
                                    EmojiCategory::Standard(idx) => {
                                        let is_flags = NATIVE_EMOJI_CATEGORIES
                                            .get(idx)
                                            .is_some_and(|(name, _)| name == "Flags");
                                        // Flags holds first paint until the font
                                        // probe resolves so the demotion order
                                        // lands without a reflow; every other
                                        // category paints immediately.
                                        let support_pending = is_flags
                                            && subdivision_render_support.read().is_none();
                                        let visible_flags = visible_flags.read();
                                        let static_items = NATIVE_EMOJI_CATEGORIES
                                            .get(idx)
                                            .map(|(_, items)| items.as_slice())
                                            .unwrap_or(&[]);
                                        let items: &[NativeEmojiEntry] = if is_flags {
                                            &visible_flags
                                        } else {
                                            static_items
                                        };
                                        rsx! {
                                            if support_pending {
                                                div { class: "py-10 text-center text-sm text-muted-foreground",
                                                    "Loading flags…"
                                                }
                                            } else {
                                                div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                                    for (emoji_idx, emoji) in items.iter().take(*grid_chunk.read()).enumerate() {
                                                    {
                                                        let glyph = emoji.glyph.clone();
                                                        let title = emoji_tooltip(emoji, tone_hint);
                                                        let sub_label = if entry_unrenderable(emoji, render_support.as_deref()) {
                                                            emoji.label.clone()
                                                        } else {
                                                            None
                                                        };
                                                        let click_glyph = glyph.clone();
                                                        let touch_glyph = glyph.clone();
                                                        let context_glyph = glyph.clone();
                                                        rsx! {
                                                            button {
                                                                key: "std-{idx}-{emoji_idx}",
                                                                class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                                title: "{title}",
                                                                onclick: move |_| emit_selection(EmojiSelection::Native { emoji: click_glyph.clone() }),
                                                                ontouchstart: {
                                                                    let tone_glyph = touch_glyph.clone();
                                                                    move |_| {
                                                                        tone_pending_glyph.set(Some(tone_glyph.clone()));
                                                                        let token = tone_press_generation.peek().wrapping_add(1);
                                                                        tone_press_generation.set(token);
                                                                        spawn(async move {
                                                                            sleep_ms(DEFAULT_LONG_PRESS_MS).await;
                                                                            if *tone_press_generation.peek() == token {
                                                                                complete_tone_long_press(
                                                                                    tone_pending_glyph.peek().clone(),
                                                                                    tone_menu,
                                                                                    tone_press_consumed,
                                                                                );
                                                                            }
                                                                        });
                                                                    }
                                                                },
                                                                ontouchmove: move |_| cancel_tone_press(tone_press_generation),
                                                                ontouchend: move |evt: TouchEvent| end_tone_press(evt, tone_press_generation, tone_press_consumed),
                                                            ontouchcancel: move |_| cancel_tone_press(tone_press_generation),
                                                            oncontextmenu: {
                                                                let tone_glyph = context_glyph.clone();
                                                                move |e: MouseEvent| {
                                                                    if cfg!(feature = "mobile_platform") {
                                                                        return;
                                                                    }
                                                                    e.prevent_default();
                                                                    e.stop_propagation();
                                                                    let tones = native_tone_variants(&tone_glyph);
                                                                    if !tones.is_empty() {
                                                                        tone_menu.set(Some(tones));
                                                                    }
                                                                }
                                                            },
                                                            "{glyph}"
                                                            if let Some(label_text) = &sub_label {
                                                                span { class: "block text-[9px] leading-none text-muted-foreground text-center mt-0.5 truncate",
                                                                    "{label_text}"
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
                                // Infinite-scroll sentinel for the chunked
                                // All/Flags grids (always mounted so the
                                // observer survives category switches).
                                div { id: "{sentinel_id}", class: "h-1 w-full" }
                            }
                        }
                        div { class: "border-t border-border p-3 bg-muted/40 rounded-b-lg",
                            button {
                                class: "w-full rounded-xl border border-border px-4 py-3 text-sm font-medium hover:bg-accent transition flex items-center justify-center gap-2",
                                onclick: move |_| show_manage_modal.set(true),
                                span { "📦" }
                                "Manage Emoji Packs"
                            }
                        }
                        if let Some(tones) = tone_menu.read().clone() {
                            div {
                                class: "absolute inset-0 z-10 bg-black/40 flex items-center justify-center rounded-lg",
                                onclick: move |_| tone_menu.set(None),
                                div {
                                    class: "bg-background border border-border rounded-lg shadow-xl p-3 max-w-[18rem] mx-4",
                                    onclick: move |e| e.stop_propagation(),
                                    p { class: "text-xs text-muted-foreground mb-2 text-center", "Choose a skin tone" }
                                    div { class: "flex flex-wrap gap-1 justify-center max-h-[16rem] overflow-y-auto",
                                        for (tone_idx, tone) in tones.iter().enumerate() {
                                            {
                                                let tone_glyph = tone.clone();
                                                let tone_title = emojis::get(tone)
                                                    .map(|e| e.name().to_string())
                                                    .unwrap_or_else(|| format!("Option {}", tone_idx + 1));
                                                rsx! {
                                                    button {
                                                        key: "tone-{tone_idx}",
                                                        class: "text-2xl rounded-lg p-2 hover:bg-accent transition",
                                                        title: "{tone_title}",
                                                        onclick: move |_| {
                                                            tone_menu.set(None);
                                                            emit_selection(EmojiSelection::Native { emoji: tone_glyph.clone() });
                                                        },
                                                        "{tone}"
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
                EmojiPackManagerModal {
                    show: show_manage_modal,
                    on_close: move |_| show_manage_modal.set(false),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_flag_ranks_first_for_us() {
        let matches = ranked_native_matches("us", None);
        assert_eq!(matches[0].glyph, "🇺🇸");
        assert_eq!(native_search_tier(&matches[0], "us"), Some(0));
    }

    #[test]
    fn ky_ranks_iso_owner_then_states() {
        let matches = ranked_native_matches("ky", None);
        // KY is the ISO 3166-1 code of the Cayman Islands (a country), so it
        // outranks the subdivisions whose segment is also "ky".
        assert_eq!(matches[0].name, "flag: Cayman Islands");
        assert!(
            matches.iter().take(4).any(|e| e.label.as_deref() == Some("US-KY")),
            "Kentucky should appear in the top 4 for \"ky\""
        );
    }

    #[test]
    fn ca_ranks_country_before_states() {
        let matches = ranked_native_matches("ca", None);
        assert_eq!(matches[0].name, "flag: Canada");
        // "ca" is the exact segment of ~8 subdivisions worldwide (Cahul,
        // Carazo, Cabañas, …); deterministic order, California in the top 10.
        assert!(
            matches.iter().take(10).any(|e| e.label.as_deref() == Some("US-CA")),
            "California should appear in the top 10 for \"ca\""
        );
    }

    #[test]
    fn kentucky_exact_name_matches() {
        let matches = ranked_native_matches("kentucky", None);
        assert_eq!(matches[0].label.as_deref(), Some("US-KY"));
    }

    #[test]
    fn york_prefers_state_over_county() {
        let matches = ranked_native_matches("york", None);
        let new_york = matches
            .iter()
            .position(|e| e.label.as_deref() == Some("US-NY"))
            .expect("New York in results");
        let north_yorkshire = matches
            .iter()
            .position(|e| e.name == "flag: North Yorkshire")
            .expect("North Yorkshire in results");
        assert!(new_york < north_yorkshire);
        assert_eq!(matches[new_york].depth, 1);
        assert_eq!(matches[north_yorkshire].depth, 2);
    }

    #[test]
    fn united_word_prefix_ranks_countries() {
        let matches = ranked_native_matches("united", None);
        let names: Vec<&str> = matches.iter().take(3).map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "flag: United States",
                "flag: United Kingdom",
                "flag: United Nations"
            ]
        );
    }

    #[test]
    fn abbreviation_and_full_name_both_reach_kentucky() {
        for query in ["ky", "us-ky", "usky", "kentucky"] {
            let matches = ranked_native_matches(query, None);
            assert!(
                matches
                    .iter()
                    .take(3)
                    .any(|e| e.label.as_deref() == Some("US-KY")),
                "query {query} should surface Kentucky in the top 3"
            );
        }
    }

    #[test]
    fn subdivision_tooltip_includes_iso_code() {
        let matches = ranked_native_matches("kentucky", None);
        let title = emoji_tooltip(&matches[0], "");
        assert_eq!(title, "flag: Kentucky (US-KY)");
    }

    #[cfg(test)]
    fn synth(glyph: &str, name: &str, is_subdivision: bool) -> NativeEmojiEntry {
        NativeEmojiEntry {
            glyph: glyph.to_string(),
            name: name.to_string(),
            group: "Flags",
            shortcodes: vec![],
            has_tones: false,
            is_subdivision,
            depth: u8::from(is_subdivision),
            label: is_subdivision.then(|| name.to_string()),
        }
    }

    #[test]
    fn probe_script_embeds_glyphs_and_fails_open() {
        let kentucky = "\u{1F3F4}\u{E0075}\u{E0073}\u{E006B}\u{E0079}\u{E007F}";
        let script = subdivision_render_probe_script(&[kentucky]);
        assert!(script.starts_with("return"));
        assert!(script.contains(kentucky), "glyph payload must be embedded");
        assert!(script.contains("'1'.repeat"), "must fail open on canvas errors");
    }

    #[test]
    fn probe_result_parsing() {
        let ok = serde_json::json!("0101");
        assert_eq!(
            parse_probe_result(&ok, 4).as_deref(),
            Some(&[false, true, false, true][..])
        );
        assert_eq!(parse_probe_result(&ok, 3), None, "length mismatch");
        let bad = serde_json::json!("01x1");
        assert_eq!(parse_probe_result(&bad, 4), None, "unexpected character");
        let not_string = serde_json::json!(true);
        assert_eq!(parse_probe_result(&not_string, 1), None);
    }

    #[test]
    fn partition_demotes_only_unsupported_subdivisions() {
        let items = vec![
            synth("a", "flag: A", false),
            synth("s1", "flag: Sub One", true),
            synth("s2", "flag: Sub Two", true),
            synth("b", "flag: B", false),
            synth("s3", "flag: Sub Three", true),
        ];
        // Verdicts align with subdivision order: s1 renders, s2/s3 do not.
        let partitioned = partition_flags_by_support(&items, Some(&[true, false, false]));
        let names: Vec<&str> = partitioned.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "flag: A",
                "flag: Sub One",
                "flag: B",
                "flag: Sub Two",
                "flag: Sub Three"
            ]
        );

        // Pending probe keeps the canonical order.
        let pending = partition_flags_by_support(&items, None);
        assert_eq!(
            pending.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            items.iter().map(|e| e.name.as_str()).collect::<Vec<_>>()
        );

        // All-supported is the identity partition.
        let all_ok = partition_flags_by_support(&items, Some(&[true, true, true]));
        assert_eq!(
            all_ok.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            items.iter().map(|e| e.name.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unrenderable_lookup_uses_canonical_index() {
        let first = CANONICAL_SUBDIVISIONS
            .first()
            .cloned()
            .expect("subdivisions exist");
        let all_supported = vec![true; CANONICAL_SUBDIVISIONS.len()];
        assert!(!entry_unrenderable(&first, None), "pending labels nothing");
        assert!(!entry_unrenderable(&first, Some(&all_supported)));
        let mut one_bad = all_supported.clone();
        one_bad[0] = false;
        assert!(entry_unrenderable(&first, Some(&one_bad)));
        // Non-subdivisions are never flagged.
        let country = synth("a", "flag: A", false);
        assert!(!entry_unrenderable(&country, Some(&one_bad)));
    }

    /// Search demotion mirrors the grids: within a relevance tier, flags the
    /// probe verified unrenderable sink below renderable ones.
    #[test]
    fn search_demotes_unrenderable_within_tier() {
        // Find the canonical indices of Kentucky (US-KY) and Kayanza (BI-KY).
        let kentucky_idx = CANONICAL_SUBDIVISIONS
            .iter()
            .position(|e| e.label.as_deref() == Some("US-KY"))
            .expect("Kentucky in canonical order");
        let kayanza_idx = CANONICAL_SUBDIVISIONS
            .iter()
            .position(|e| e.label.as_deref() == Some("BI-KY"))
            .expect("Kayanza in canonical order");
        let mut support = vec![true; CANONICAL_SUBDIVISIONS.len()];
        support[kentucky_idx] = false;
        support[kayanza_idx] = false;

        let matches = ranked_native_matches("ky", Some(&support));
        // Cayman Islands (country, renders) stays first; the renderable
        // subdivisions keep ranking; the two unrenderable ones sink to the
        // bottom of the tier.
        assert_eq!(matches[0].name, "flag: Cayman Islands");
        let kentucky_pos = matches
            .iter()
            .position(|e| e.label.as_deref() == Some("US-KY"))
            .unwrap();
        let kayanza_pos = matches
            .iter()
            .position(|e| e.label.as_deref() == Some("BI-KY"))
            .unwrap();
        let last_renderable_sub_pos = matches
            .iter()
            .take_while(|e| e.label.as_deref() != Some("BI-KY") && e.label.as_deref() != Some("US-KY"))
            .count()
            .saturating_sub(1);
        assert!(kentucky_pos > last_renderable_sub_pos);
        assert!(kayanza_pos > last_renderable_sub_pos);

        // With the probe pending, the previous order applies.
        let pending = ranked_native_matches("ky", None);
        assert_eq!(pending[0].name, "flag: Cayman Islands");
    }
}
