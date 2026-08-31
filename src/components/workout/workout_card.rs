//! Workout card renderer for kind-1301 events. Renders both wire forms:
//! activity cardio metrics and strength
//! breakdowns with kind-33401 template title resolution.
use super::units::{self, WorkoutUnits};
use super::exercise_type_icon::ExerciseTypeIcon;
use crate::components::icons::{BookmarkIcon, MessageCircleIcon, Repeat2Icon, ShareIcon, ZapIcon};
use crate::components::{ConfirmModal, ReactionButton, ReplyComposer, RichContent, SensitiveContent, ZapModal};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::services::aggregation::InteractionCounts;
use crate::stores::bookmarks;
use crate::stores::nostr_client::{self, delete_repost, publish_repost, HAS_SIGNER};
use crate::stores::workout_template_cache;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::format::format_relative_time_or;
use crate::utils::nip19_urls::note_route_id;
use crate::utils::nip36;
use crate::utils::nips::nip101e::{self, format_duration_time, ExerciseSet, ExerciseGroup, ExerciseType, WorkoutRecord};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
use nostr_sdk::{nips::nip19::Nip19Event, Event as NostrEvent, ToBech32};

#[cfg(feature = "web")]
const INTERACTIVE_ELEMENT_SELECTOR: &str =
    "a, button, input, textarea, select, summary, [role='button'], [role='link'], [contenteditable='true'], video, audio, iframe, [data-interactive]";

#[derive(Clone, Copy, PartialEq)]
enum HeroKind {
    Distance,
    Steps,
    Duration,
    None,
}

/// Label resolution chain: localized type label → raw exercise
/// verb → raw `type` code capitalized → "Workout".
fn type_label(workout: &WorkoutRecord) -> String {
    if let Some(t) = workout.activity_type() {
        return t.hashtag().to_string();
    }
    if let Some(verb) = &workout.exercise {
        let mut chars = verb.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => "Workout".to_string(),
        };
    }
    if let Some(code) = &workout.workout_type_code {
        let mut chars = code.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => "Workout".to_string(),
        };
    }
    "Workout".to_string()
}

fn format_set(set: &ExerciseSet, u: WorkoutUnits) -> String {
    match (set.reps, set.weight_kg) {
        (Some(reps), Some(_)) => {
            let w = set.weight_kg.map(|kg| units::format_weight_kg(kg, u)).unwrap_or_default();
            format!("{} \u{d7} {}", reps, w)
        }
        (Some(reps), None) => format!("{} reps", reps),
        (None, Some(_)) => set.weight_kg.map(|kg| units::format_weight_kg(kg, u)).unwrap_or_default(),
        (None, None) => "\u{2014}".to_string(),
    }
}

/// Per-set descriptors collapsed to `N × descriptor` when all identical
/// Summary line for the hero metric.
fn summary_line(group: &ExerciseGroup, u: WorkoutUnits) -> String {
    let descriptors: Vec<String> = group.sets.iter().map(|s| format_set(s, u)).collect();
    if descriptors.len() > 1 && descriptors.windows(2).all(|w| w[0] == w[1]) {
        format!("{} \u{d7} {}", descriptors.len(), descriptors[0])
    } else {
        descriptors.join(", ")
    }
}

/// Hero metric precedence: distance (> 0) > steps > duration > none.
fn hero_kind(workout: &WorkoutRecord) -> HeroKind {
    if let Some(d) = &workout.distance {
        if d.to_meters() > 0.0 {
            return HeroKind::Distance;
        }
    }
    if workout.steps.is_some() {
        return HeroKind::Steps;
    }
    if workout.effective_duration_seconds().is_some() {
        return HeroKind::Duration;
    }
    HeroKind::None
}

/// Secondary stats grid: strength-form
/// strength aggregates first, then cardio metrics; the hero metric is
/// excluded. Cycling reports speed, everything else pace.
fn build_stats(workout: &WorkoutRecord, hero: HeroKind, u: WorkoutUnits) -> Vec<(String, String)> {
    let mut stats: Vec<(String, String)> = Vec::new();
    let groups = workout.exercise_groups();
    if !groups.is_empty() {
        stats.push((groups.len().to_string(), "Exercises".to_string()));
        let set_count: usize = groups.iter().map(|g| g.sets.len()).sum();
        stats.push((set_count.to_string(), "Sets".to_string()));
        if groups.iter().any(|g| g.total_volume_kg().is_some()) {
            let total_volume: f64 = groups.iter().filter_map(|g| g.total_volume_kg()).sum();
            stats.push((units::format_weight_kg(total_volume, u), "Volume".to_string()));
        }
    }
    let duration = workout.effective_duration_seconds();
    let distance = workout.distance.as_ref();
    if hero != HeroKind::Duration {
        if let Some(d) = duration {
            stats.push((format_duration_time(d), "Duration".to_string()));
        }
    }
    if hero != HeroKind::Distance {
        if let Some(d) = distance {
            let (value, unit) = units::format_distance_parts(d.to_meters(), u);
            stats.push((format!("{} {}", value, unit), "Distance".to_string()));
        }
    }
    if let (Some(d), Some(dist)) = (duration, distance) {
        if dist.to_meters() > 0.0 {
            let label = if workout.activity_type() == Some(ExerciseType::Cycling) {
                units::speed_label(d, dist.to_meters(), u)
            } else {
                units::pace_label(d, dist.to_meters(), u)
            };
            if !label.is_empty() {
                let name = if workout.activity_type() == Some(ExerciseType::Cycling) {
                    "Speed"
                } else {
                    "Pace"
                };
                stats.push((label, name.to_string()));
            }
        }
    }
    if let Some(gain) = &workout.elevation_gain {
        stats.push((units::format_elevation(gain.to_meters(), u), "Elevation gain".to_string()));
    }
    if let Some(loss) = &workout.elevation_loss {
        stats.push((units::format_elevation(loss.to_meters(), u), "Elevation loss".to_string()));
    }
    if let Some(kcal) = workout.calories {
        stats.push((format!("{} kcal", kcal), "Calories".to_string()));
    }
    if hero != HeroKind::Steps {
        if let Some(steps) = workout.steps {
            stats.push((steps.to_string(), "Steps".to_string()));
        }
    }
    if let Some(bpm) = workout.avg_heart_rate {
        stats.push((format!("{} bpm", bpm), "Heart rate".to_string()));
    }
    if let Some(bpm) = workout.max_heart_rate {
        stats.push((format!("{} bpm", bpm), "Max heart rate".to_string()));
    }
    if let Some(sets) = workout.sets {
        stats.push((sets.to_string(), "Sets".to_string()));
    }
    if let Some(reps) = workout.reps {
        stats.push((reps.to_string(), "Reps".to_string()));
    }
    if let Some(w) = &workout.weight {
        stats.push((units::format_weight_kg(w.to_kilograms(), u), "Weight".to_string()));
    }
    stats
}

/// One strength-form exercise row: resolves the kind-33401 template title via the
/// cache (slug fallback until fetched).
#[component]
fn ExerciseRow(group: ExerciseGroup, units_pref: WorkoutUnits) -> Element {
    let reference = group.reference.clone();
    use_effect(use_reactive(&reference, |r: String| {
        spawn(async move {
            workout_template_cache::fetch_template(r).await;
        });
    }));
    // Subscribe to cache mutations via the version bump signal.
    let _version = *workout_template_cache::WORKOUT_TEMPLATE_CACHE_VERSION.read();
    let fallback = group.display_name().unwrap_or_else(|| "\u{2014}".to_string());
    let name = workout_template_cache::cached_title(&group.reference).unwrap_or(fallback);
    let summary = summary_line(&group, units_pref);
    rsx! {
        div { class: "flex w-full items-start justify-between gap-2",
            span { class: "text-sm font-medium flex-1 break-words", "{name}" }
            span { class: "text-xs text-muted-foreground whitespace-nowrap", "{summary}" }
        }
    }
}

#[component]
pub fn WorkoutCard(
    event: NostrEvent,
    #[props(default = None)] precomputed_counts: Option<InteractionCounts>,
    #[props(default = None)] replies_count: Option<usize>,
    #[props(default = None)] on_comment_created: Option<EventHandler<NostrEvent>>,
) -> Element {
    let workout = match nip101e::parse_workout(&event) {
        Ok(w) => w,
        Err(_) => return rsx! {},
    };
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_metadata = author_pubkey.clone();
    let author_pubkey_for_display = author_pubkey.clone();
    let author_pubkey_for_link = author_pubkey.clone();
    let event_id = event.id;
    let event_id_str = event_id.to_string();
    let event_id_repost = event_id_str.clone();
    let event_id_bookmark = event_id_str.clone();
    let created_at = event.created_at;
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    // Interaction bar state
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut user_repost_id = use_signal(|| None::<String>);
    let mut show_repost_menu = use_signal(|| false);
    let mut show_undo_repost_confirm = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_str);
    let has_signer = *HAS_SIGNER.read();
    let toast = consume_toast();
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let reaction = use_reaction(
        event_id.to_hex(),
        event.pubkey.to_hex(),
        precomputed_counts.as_ref(),
    );
    use_effect(use_reactive(&precomputed_counts, move |counts_opt| {
        if let Some(counts) = counts_opt {
            reply_count.set(counts.replies.min(501));
            repost_count.set(counts.reposts.min(501));
            zap_amount_sats.set(counts.zap_amount_sats);
            is_reposted.set(counts.user_reposted.unwrap_or(false));
            user_repost_id.set(counts.user_repost_id.clone());
            is_zapped.set(counts.user_zapped.unwrap_or(false));
        }
    }));
    let _metadata_task = use_future(move || {
        let pubkey_str = author_pubkey_for_metadata.clone();
        async move {
            match nostr_sdk::PublicKey::parse(&pubkey_str) {
                Ok(pk) => {
                    if let Some(client) = nostr_client::get_client() {
                        if let Ok(Some(metadata)) =
                            client.fetch_metadata(pk, std::time::Duration::from_secs(5)).await
                        {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse pubkey: {}", e);
                }
            }
        }
    });
    let units_pref = units::effective_units();
    let hero = hero_kind(&workout);
    let stats = build_stats(&workout, hero, units_pref);
    let exercise_groups = workout.exercise_groups();
    let label = type_label(&workout);
    let author_name = author_metadata
        .read()
        .as_ref()
        .and_then(crate::stores::profiles::display_name_or_name)
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey_for_display));
    let time_ago = format_relative_time_or(created_at.as_secs(), "now");
    let content_warning = nip36::get_content_warning(&event.tags);
    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };
    let zap_button_class = if *is_zapped.read() {
        "flex items-center gap-1 text-yellow-500 transition px-2 py-1.5 rounded"
    } else {
        "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded"
    };
    let bookmark_button_class = if is_bookmarked {
        "flex items-center text-blue-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };
    let nav = use_navigator();
    let event_id_for_nav = event.id.to_hex();
    let author_for_nav = event.pubkey.to_hex();
    let source_badge = workout.source_or_client().map(|s| s.to_uppercase());
    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition border-b border-border cursor-pointer",
            onclick: move |_evt: MouseEvent| {
                #[cfg(feature = "web")]
                {
                    if let Some(target) = _evt.data.as_web_event().target() {
                        if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                            if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some() {
                                return;
                            }
                        }
                    }
                }
                nav.push(Route::AddressViewer {
                    address: note_route_id(&event_id_for_nav, Some(&author_for_nav)),
                });
            },
            // Author header
            div { class: "flex items-center gap-2 mb-3",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&author_pubkey_for_link),
                    },
                    class: "font-semibold hover:underline",
                    "{author_name}"
                }
                span { class: "text-muted-foreground text-sm", "\u{b7} {time_ago}" }
            }
            {
                let inner = rsx! {
                    // Workout header: icon + title + source badge
                    div { class: "flex items-center gap-3",
                        div { class: "w-10 h-10 rounded-full bg-primary/10 flex items-center justify-center shrink-0",
                            ExerciseTypeIcon {
                                exercise_type: workout.activity_type(),
                                class: "w-6 h-6 text-primary".to_string(),
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold truncate", "{workout.title.clone().unwrap_or_else(|| label.clone())}" }
                            if workout.title.is_some() {
                                div { class: "text-xs text-muted-foreground truncate", "{label}" }
                            }
                        }
                        if let Some(badge) = &source_badge {
                            span { class: "text-xs font-medium text-primary bg-primary/10 rounded px-1.5 py-0.5 uppercase shrink-0", "{badge}" }
                        }
                    }
                    // Hero metric
                    {
                        match hero {
                            HeroKind::Distance => {
                                let (value, unit) = units::format_distance_parts(workout.distance.as_ref().unwrap().to_meters(), units_pref);
                                rsx! {
                                    div { class: "mt-2",
                                        div { class: "flex items-baseline gap-1",
                                            span { class: "text-3xl font-bold text-primary", "{value}" }
                                            span { class: "text-sm text-muted-foreground", "{unit}" }
                                        }
                                        div { class: "text-xs text-muted-foreground", "Distance" }
                                    }
                                }
                            }
                            HeroKind::Steps => {
                                rsx! {
                                    div { class: "mt-2",
                                        span { class: "text-3xl font-bold text-primary", "{workout.steps.unwrap()}" }
                                        div { class: "text-xs text-muted-foreground", "Steps" }
                                    }
                                }
                            }
                            HeroKind::Duration => {
                                rsx! {
                                    div { class: "mt-2",
                                        span { class: "text-3xl font-bold text-primary", "{format_duration_time(workout.effective_duration_seconds().unwrap())}" }
                                        div { class: "text-xs text-muted-foreground", "Duration" }
                                    }
                                }
                            }
                            HeroKind::None => rsx! {},
                        }
                    }
                    // Stats grid
                    if !stats.is_empty() {
                        div { class: "grid grid-cols-3 gap-x-3 gap-y-2 mt-3",
                            for (i, (value, stat_label)) in stats.iter().enumerate() {
                                div { key: "{i}",
                                    span { class: "block text-base font-semibold", "{value}" }
                                    span { class: "block text-xs text-muted-foreground", "{stat_label}" }
                                }
                            }
                        }
                    }
                    // Strength-form exercise breakdown
                    if !exercise_groups.is_empty() {
                        div { class: "mt-3 space-y-1",
                            for (i, group) in exercise_groups.iter().enumerate() {
                                ExerciseRow { key: "{i}", group: group.clone(), units_pref }
                            }
                        }
                    }
                    // Notes content
                    if !workout.content.trim().is_empty() {
                        div { class: "mt-2 text-sm break-words",
                            RichContent {
                                content: workout.content.clone(),
                                tags: event.tags.iter().cloned().collect(),
                            }
                        }
                    }
                };
                if let Some(reason) = content_warning {
                    rsx! { SensitiveContent { reason, {inner} } }
                } else {
                    inner
                }
            }
            // Interaction bar
            div { class: "flex items-center justify-between max-w-md mt-2 -ml-2",
                button {
                    r#type: "button",
                    aria_label: "Comment",
                    class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded text-muted-foreground",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        show_comment_composer.set(true);
                    },
                    MessageCircleIcon { class: "h-4 w-4".to_string(), filled: false }
                    span { class: "text-xs",
                        {
                            let count = replies_count.unwrap_or(*reply_count.read());
                            if count > 500 {
                                "500+".to_string()
                            } else if count > 0 {
                                count.to_string()
                            } else {
                                "".to_string()
                            }
                        }
                    }
                }
                div { class: "relative",
                    button {
                        r#type: "button",
                        aria_label: "Repost",
                        class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
                        disabled: !has_signer || *is_reposting.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            if has_signer && !*is_reposting.read() {
                                show_repost_menu.toggle();
                            }
                        },
                        Repeat2Icon { class: "h-4 w-4".to_string(), filled: false }
                        span { class: "text-xs",
                            {
                                let count = *repost_count.read();
                                if count > 500 {
                                    "500+".to_string()
                                } else if count > 0 {
                                    count.to_string()
                                } else {
                                    "".to_string()
                                }
                            }
                        }
                    }
                    if *show_repost_menu.read() {
                        div {
                            class: "fixed inset-0 z-40",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_repost_menu.set(false);
                            },
                        }
                        div {
                            class: "absolute bottom-full left-0 mb-1 bg-card border border-border rounded-lg shadow-lg py-1 min-w-[120px] z-50",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            button {
                                class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_repost_menu.set(false);
                                    if *is_reposted.read() {
                                        show_undo_repost_confirm.set(true);
                                    } else {
                                        let event_id_clone = event_id_repost.clone();
                                        is_reposting.set(true);
                                        spawn(async move {
                                            match publish_repost(event_id_clone, None).await {
                                                Ok(repost_id) => {
                                                    is_reposted.set(true);
                                                    user_repost_id.set(Some(repost_id));
                                                    let current_count = *repost_count.peek();
                                                    repost_count.set((current_count + 1).min(501));
                                                    is_reposting.set(false);
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to repost event: {}", e);
                                                    is_reposting.set(false);
                                                }
                                            }
                                        });
                                    }
                                },
                                Repeat2Icon { class: "h-4 w-4".to_string(), filled: false }
                                if *is_reposted.read() {
                                    "Undo Repost"
                                } else {
                                    "Repost"
                                }
                            }
                            button {
                                class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_repost_menu.set(false);
                                    let nevent = Nip19Event::new(event.id).author(event.pubkey);
                                    match nevent.to_bech32() {
                                        Ok(nevent_str) => {
                                            nav.push(Route::NoteNew {
                                                quote: Some(nevent_str),
                                            });
                                        }
                                        Err(e) => {
                                            log::warn!("Failed to encode nevent for quote: {}", e);
                                        }
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "h-4 w-4",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z",
                                    }
                                }
                                "Quote"
                            }
                        }
                    }
                }
                ReactionButton { reaction: reaction.clone(), has_signer }
                {
                    let has_lightning = author_metadata
                        .read()
                        .as_ref()
                        .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                        .is_some();
                    if has_lightning {
                        rsx! {
                            button {
                                r#type: "button",
                                aria_label: "Zap",
                                class: "{zap_button_class}",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_zap_modal.set(true);
                                },
                                ZapIcon { class: "h-4 w-4".to_string(), filled: *is_zapped.read() }
                                span { class: "text-xs",
                                    {
                                        let amount = *zap_amount_sats.read();
                                        if amount > 0 { crate::utils::format::format_sats_compact(amount) } else { "".to_string() }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
                button {
                    r#type: "button",
                    aria_label: "Bookmark",
                    class: "{bookmark_button_class} hover:bg-blue-500/10 px-2 py-1.5 rounded",
                    disabled: !has_signer || *is_bookmarking.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_bookmarking.read() {
                            return;
                        }
                        let event_id_clone = event_id_bookmark.clone();
                        let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);
                        is_bookmarking.set(true);
                        spawn(async move {
                            let result = if currently_bookmarked {
                                bookmarks::unbookmark_event(event_id_clone).await
                            } else {
                                bookmarks::bookmark_event(event_id_clone).await
                            };
                            if let Err(e) = result {
                                log::error!("Failed to toggle bookmark: {}", e);
                            }
                            is_bookmarking.set(false);
                        });
                    },
                    BookmarkIcon { class: "h-4 w-4".to_string(), filled: is_bookmarked }
                }
                button {
                    r#type: "button",
                    aria_label: "Share",
                    class: "flex items-center text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 px-2 py-1.5 rounded transition",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        let nevent = Nip19Event::new(event_id).author(event.pubkey);
                        if let Ok(nevent_str) = nevent.to_bech32() {
                            let share_url = format!("https://njump.me/{}", nevent_str);
                            spawn(async move {
                                if copy_to_clipboard(&share_url).await.is_ok() {
                                    toast.success("Link copied".to_string(), ToastOptions::new());
                                } else {
                                    toast.error("Copy failed".to_string(), ToastOptions::new());
                                }
                            });
                        }
                    },
                    ShareIcon { class: "h-4 w-4".to_string(), filled: false }
                }
            }
            if *show_zap_modal.read() {
                {
                    let meta = author_metadata.read();
                    let recipient_name = meta
                        .as_ref()
                        .and_then(crate::stores::profiles::display_name_or_name)
                        .unwrap_or_else(|| truncate_pubkey(&event.pubkey.to_string()));
                    let lud16 = meta.as_ref().and_then(|m| m.lud16.clone());
                    let lud06 = meta.as_ref().and_then(|m| m.lud06.clone());
                    rsx! {
                        ZapModal {
                            recipient_pubkey: event.pubkey.to_hex(),
                            recipient_name: recipient_name,
                            lud16: lud16,
                            lud06: lud06,
                            event_id: Some(event_id.to_hex()),
                            on_close: move |_| show_zap_modal.set(false),
                        }
                    }
                }
            }
            if *show_comment_composer.read() {
                ReplyComposer {
                    target: event.clone(),
                    root_event: None,
                    on_close: move |_| {
                        show_comment_composer.set(false);
                    },
                    on_success: move |comment_event: NostrEvent| {
                        if on_comment_created.is_none() {
                            let current = *reply_count.read();
                            reply_count.set((current + 1).min(501));
                        }
                        if let Some(handler) = on_comment_created.as_ref() {
                            handler.call(comment_event);
                        }
                        show_comment_composer.set(false);
                    },
                }
            }
            if *show_undo_repost_confirm.read() {
                ConfirmModal {
                    title: "Undo Repost".to_string(),
                    message: "Are you sure you want to undo this repost?".to_string(),
                    confirm_text: Some("Undo".to_string()),
                    cancel_text: None,
                    on_confirm: move |_| {
                        show_undo_repost_confirm.set(false);
                        if let Some(repost_id) = user_repost_id.peek().clone() {
                            is_reposting.set(true);
                            spawn(async move {
                                match delete_repost(repost_id).await {
                                    Ok(_) => {
                                        is_reposted.set(false);
                                        user_repost_id.set(None);
                                        let current_count = *repost_count.peek();
                                        if current_count > 0 {
                                            repost_count.set(current_count - 1);
                                        }
                                        is_reposting.set(false);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to undo repost: {}", e);
                                        is_reposting.set(false);
                                    }
                                }
                            });
                        }
                    },
                    on_cancel: move |_| {
                        show_undo_repost_confirm.set(false);
                    },
                }
            }
        }
    }
}
