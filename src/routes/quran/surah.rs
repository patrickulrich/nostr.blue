use crate::components::content_share_modal::{ContentShareModal, ContentType};
use crate::components::icons::{SparklesIcon, VolumeIcon};
use crate::components::{ConfirmModal, HighlightModal};
use crate::routes::Route;
use crate::services::quran_api::{format_ayah_reference, get_audio_url};
use crate::stores::ai_chat_seed_store::{queue_ai_chat_seed, AiChatSeedPayload};
use crate::stores::audio::music_player::{self, MusicTrack};
use crate::stores::audio::nostr_music::TrackSource;
use crate::stores::quran_store::{self, CachedSurah};
use crate::stores::{ai_chat_store, auth_store};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::collections::HashSet;

fn build_quran_track(
    surah: u32,
    surah_name: &str,
    reciter: &str,
    ayah_number: u32,
    ayah_in_surah: u32,
) -> MusicTrack {
    let url = get_audio_url(reciter, ayah_number);
    MusicTrack {
        id: format!("quran-{}-{}-{}", surah, reciter, ayah_number),
        title: format!("{} {}:{}", surah_name, surah, ayah_in_surah),
        artist: reciter.to_string(),
        album: Some(format!("Surah {}", surah_name)),
        media_url: url,
        album_art_url: None,
        artist_art_url: None,
        duration: None,
        artist_id: None,
        album_id: None,
        artist_npub: None,
        source: TrackSource::Quran {
            reciter: reciter.to_string(),
            surah,
        },
        msat_total: None,
        created_at: None,
        is_podcast: true,
        is_live_stream: false,
        value_block: None,
        chapters_url: None,
        transcripts: Vec::new(),
    }
}

fn play_quran_surah_audio(
    surah: u32,
    surah_name: &str,
    reciter: &str,
    ayahs: &[crate::services::quran_api::Ayah],
) {
    if ayahs.is_empty() {
        return;
    }
    let first = &ayahs[0];
    let track = build_quran_track(surah, surah_name, reciter, first.number, first.number_in_surah);
    let mut state = music_player::MUSIC_PLAYER.write();
    state.stop_at_end = true;
    drop(state);
    music_player::play_or_toggle_track(track, None, None);

    if ayahs.len() > 1 {
        let rest: Vec<MusicTrack> = ayahs[1..]
            .iter()
            .map(|a| build_quran_track(surah, surah_name, reciter, a.number, a.number_in_surah))
            .collect();
        music_player::append_to_playlist(rest);
    }
}

#[component]
fn QuranAudioMenu(surah: u32, surah_name: String) -> Element {
    let audio_editions = quran_store::AUDIO_EDITIONS.read();
    let verse_by_verse: Vec<_> = audio_editions
        .iter()
        .filter(|e| e.edition_type == "versebyverse")
        .take(8)
        .collect();
    if verse_by_verse.is_empty() {
        return VNode::empty();
    }

    let mut is_open = use_signal(|| false);

    rsx! {
        div { class: "relative",
            button {
                class: "p-1.5 hover:bg-muted rounded-lg transition text-muted-foreground",
                title: "Audio",
                onclick: move |evt: MouseEvent| {
                    evt.stop_propagation();
                    is_open.toggle();
                },
                VolumeIcon { class: "w-4 h-4".to_string() }
            }

            if is_open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        is_open.set(false);
                    },
                }

                div { class: "absolute right-0 mt-2 w-56 bg-background border border-border rounded-lg shadow-lg z-50 py-1",
                    p { class: "px-4 py-1.5 text-xs font-medium text-muted-foreground uppercase tracking-wide", "Audio Reciters" }
                    div { class: "h-px bg-border mx-2 my-1" }

                    for (i, edition) in verse_by_verse.iter().enumerate() {
                        {
                            let reciter_id = edition.identifier.clone();
                            let reciter_name = edition.english_name.clone();
                            let s_name = surah_name.clone();
                            let surah_for_click = surah;
                            let translation_for_cache = quran_store::CURRENT_TRANSLATION.read().clone();
                            rsx! {
                                button {
                                    key: "{i}",
                                    class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2.5",
                                    onclick: move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        is_open.set(false);
                                        if let Some(data) = quran_store::get_cached_surah(surah_for_click, &translation_for_cache) {
                                            play_quran_surah_audio(
                                                surah_for_click,
                                                &s_name,
                                                &reciter_id,
                                                &data.arabic.ayahs,
                                            );
                                        }
                                    },
                                    svg {
                                        class: "w-4 h-4 text-muted-foreground shrink-0",
                                        view_box: "0 0 24 24",
                                        fill: "currentColor",
                                        polygon { points: "8,5 19,12 8,19" }
                                    }
                                    span { class: "text-sm", "{reciter_name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn QuranSurah(surah: u32) -> Element {
    if !(1..=114).contains(&surah) {
        return rsx! {
            div { class: "flex flex-col items-center justify-center h-full gap-4",
                h2 { class: "text-xl font-bold text-destructive", "Surah Not Found" }
                p { class: "text-muted-foreground",
                    "Surah {surah} does not exist. There are 114 surahs in the Quran."
                }
                Link {
                    to: Route::QuranHome {},
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                    "Back to Quran"
                }
            }
        };
    }

    let navigator = navigator();
    let translation = quran_store::CURRENT_TRANSLATION.read().clone();
    let mut surah_data = use_signal(|| Option::<CachedSurah>::None);
    let mut loading = use_signal(|| false);
    let mut active_tab = use_signal(|| "translation");
    let mut selected_ayahs = use_signal(HashSet::<u32>::new);
    let mut show_toolbar = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut share_title = use_signal(String::new);
    let mut share_url = use_signal(String::new);
    let mut share_content = use_signal(String::new);
    let mut highlight_feedback = use_signal(|| None::<(bool, String)>);
    let mut show_highlight_modal = use_signal(|| false);
    let mut pending_highlight_text = use_signal(String::new);
    let mut pending_highlight_reference = use_signal(String::new);
    let mut show_ai_chat_confirm = use_signal(|| false);
    let mut pending_ai_chat_seed = use_signal(|| None::<AiChatSeedPayload>);
    let is_authenticated = auth_store::is_authenticated();
    let toast = consume_toast();

    let current_key = format!("{}/{}", surah, translation);
    let mut loaded_key = use_signal(String::new);

    if *loaded_key.peek() != current_key {
        loaded_key.set(current_key.clone());
        selected_ayahs.set(HashSet::new());
        show_toolbar.set(false);
        surah_data.set(None);
        let s = surah;
        let t = translation.clone();
        let request_key = current_key.clone();
        spawn(async move {
            loading.set(true);
            let result = quran_store::load_surah(s, &t).await;
            if *loaded_key.peek() != request_key {
                return;
            }
            match result {
                Ok(data) => {
                    surah_data.set(Some(data));
                }
                Err(e) => {
                    log::error!("Failed to load surah {}: {}", s, e);
                }
            }
            loading.set(false);
            let _ = quran_store::fetch_surah_highlights(s).await;
            if auth_store::is_authenticated() {
                if let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() {
                    spawn(async move {
                        let _ = quran_store::fetch_user_highlights(&pubkey).await;
                    });
                }
            }
        });
    }

    let surah_ref = quran_store::get_surah_ref(surah);
    let surah_name = surah_ref
        .as_ref()
        .map(|s| s.english_name.clone())
        .unwrap_or_else(|| format!("Surah {}", surah));
    let surah_name_arabic = surah_ref
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let edition_label = quran_store::CURRENT_TRANSLATION.read().clone();
    let surah_name_for_copy = surah_name.clone();
    let surah_name_for_highlight = surah_name.clone();
    let surah_name_for_share = surah_name.clone();
    let surah_name_for_ai = surah_name.clone();
    let edition_for_copy = edition_label.clone();
    let edition_for_highlight = edition_label.clone();
    let edition_for_share = edition_label.clone();
    let edition_for_ai = edition_label.clone();

    let mut handle_ayah_click = move |ayah_num: u32| {
        let mut current = selected_ayahs.write();
        if current.contains(&ayah_num) {
            current.remove(&ayah_num);
        } else {
            current.insert(ayah_num);
        }
        let is_empty = current.is_empty();
        drop(current);
        show_toolbar.set(!is_empty);
    };

    let clear_selection = move |_| {
        selected_ayahs.set(HashSet::new());
        show_toolbar.set(false);
    };

    let copy_ayahs = {
        let selected_ayahs_for_copy = selected_ayahs;
        let mut selected_ayahs_for_clear = selected_ayahs;
        let mut show_toolbar_for_clear = show_toolbar;
        let surah_name_for_copy = surah_name_for_copy.clone();
        let edition_for_copy = edition_for_copy.clone();
        move |_| {
            if let Some(data) = surah_data.read().as_ref() {
                let mut selected: Vec<u32> = selected_ayahs_for_copy.read().iter().copied().collect();
                selected.sort();
                if selected.is_empty() {
                    return;
                }
                let text = build_selected_text(data, &selected, &surah_name_for_copy);
                let reference = format_selected_ayahs_reference(&surah_name_for_copy, &selected, &edition_for_copy);
                let full_text = format!("{}\n\u{2014} {}", text, reference);
                let toast_inner = toast;
                spawn(async move {
                    if let Err(e) = crate::platform::clipboard::copy_to_clipboard(&full_text).await {
                        log::error!("Clipboard write failed: {:?}", e);
                        toast_inner.error(
                            "Failed to copy to clipboard".to_string(),
                            ToastOptions::new(),
                        );
                        return;
                    }
                    selected_ayahs_for_clear.set(HashSet::new());
                    show_toolbar_for_clear.set(false);
                });
            }
        }
    };

    let open_highlight_modal = {
        let surah_name_for_highlight = surah_name_for_highlight.clone();
        let edition_for_highlight = edition_for_highlight.clone();
        move |_| {
            if let Some(data) = surah_data.read().as_ref() {
                let selected: Vec<u32> = selected_ayahs.read().iter().copied().collect();
                if selected.is_empty() {
                    return;
                }
                let text = build_selected_text(data, &selected, &surah_name_for_highlight);
                let reference = format_selected_ayahs_reference(&surah_name_for_highlight, &selected, &edition_for_highlight);
                pending_highlight_text.set(text);
                pending_highlight_reference.set(reference);
                show_highlight_modal.set(true);
            }
        }
    };

    let selected_count = selected_ayahs.read().len();

    rsx! {
        div { class: "flex flex-col h-full",
            // Sticky header
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-10 border-b border-border",
                div { class: "max-w-3xl mx-auto p-4",
                    div { class: "flex items-center justify-between gap-4",
                        // Back arrow
                        Link {
                            to: Route::QuranHome {},
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M15 19l-7-7 7-7",
                                }
                            }
                        }
                        // Center title
                        div { class: "text-center flex-1",
                            h1 { class: "text-xl font-bold", "{surah_name}" }
                            p { class: "text-sm text-muted-foreground", "{surah_name_arabic} · {edition_label}" }
                        }
                        // Right: Audio + Prev/Next arrows
                        div { class: "flex gap-1 items-center",
                            QuranAudioMenu {
                                surah,
                                surah_name: surah_name.clone(),
                            }
                            if surah > 1 {
                                Link {
                                    to: Route::QuranSurah { surah: surah - 1 },
                                    class: "p-2 hover:bg-muted rounded-lg transition",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        class: "w-5 h-5",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M15 19l-7-7 7-7",
                                        }
                                    }
                                }
                            }
                            if surah < 114 {
                                Link {
                                    to: Route::QuranSurah { surah: surah + 1 },
                                    class: "p-2 hover:bg-muted rounded-lg transition",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        class: "w-5 h-5",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M9 5l7 7-7 7",
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Tab bar
            div { class: "flex gap-1 border-b border-border px-4 max-w-3xl mx-auto",
                button {
                    class: if *active_tab.read() == "arabic" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("arabic"),
                    "Arabic"
                }
                button {
                    class: if *active_tab.read() == "translation" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("translation"),
                    "Translation"
                }
                button {
                    class: if *active_tab.read() == "both" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("both"),
                    "Both"
                }
            }

            // Scrollable content
            div { class: "flex-1 overflow-y-auto",
                div { class: "max-w-3xl mx-auto p-4 pb-32",
                    if *loading.read() {
                        div { class: "space-y-4 animate-pulse",
                            for i in 0..10 {
                                div {
                                    key: "{i}",
                                    class: "h-4 bg-muted rounded",
                                    style: "width: {70 + (i % 3) * 10}%",
                                }
                            }
                        }
                    } else if let Some(data) = surah_data.read().clone() {
                        {
                            let current_tab = active_tab.read().clone();
                            let show_arabic = current_tab == "arabic" || current_tab == "both";
                            let show_translation = current_tab == "translation" || current_tab == "both";

                            rsx! {
                                // Bismillah
                                if surah != 9 && surah != 1 {
                                    div {
                                        class: "text-center text-2xl leading-loose py-4",
                                        dir: "rtl",
                                        "بِسْمِ ٱللَّهِ ٱلرَّحْمَـٰنِ ٱلرَّحِيمِ"
                                    }
                                }

                                // Ayah list
                                div { class: "prose prose-lg dark:prose-invert max-w-none leading-relaxed",
                                    for ayah in &data.arabic.ayahs {
                                        {
                                            let ayah_num = ayah.number_in_surah;
                                            let is_selected = selected_ayahs.read().contains(&ayah_num);
                                            let translation_text = data.translation.as_ref().and_then(|t| {
                                                t.ayahs.iter().find(|a| a.number_in_surah == ayah_num).map(|a| a.text.clone())
                                            });
                                            let is_highlighted = quran_store::is_ayah_highlighted(surah, ayah.number);
                                            let highlight_count = quran_store::get_ayah_highlight_count(ayah.number_in_surah);
                                            let sajda = ayah.sajda.is_sajda();

                                            rsx! {
                                                div {
                                                    key: "{ayah_num}",
                                                    tabindex: 0,
                                                    role: "button",
                                                    class: "cursor-pointer rounded px-0.5 transition-colors block focus:outline-hidden focus:ring-2 focus:ring-primary mb-3",
                                                    class: if is_selected { "bg-primary/20 ring-2 ring-primary" } else if is_highlighted { "bg-yellow-100 dark:bg-yellow-900/30" } else { "" },
                                                    onclick: move |_| handle_ayah_click(ayah_num),
                                                    div { class: "flex items-start gap-3",
                                                        sup { class: "text-xs text-cyan-500 mr-1 select-none", "{ayah_num}" }
                                                        div { class: "flex-1 min-w-0 space-y-1",
                                                            if show_arabic {
                                                                div {
                                                                    class: "text-xl leading-loose",
                                                                    dir: "rtl",
                                                                    "{ayah.text}"
                                                                }
                                                            }
                                                            if show_translation {
                                                                if let Some(ref text) = translation_text {
                                                                    div { class: "text-foreground/90 leading-relaxed",
                                                                        "{text}"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if highlight_count > 0 && !is_selected {
                                                            span {
                                                                class: "ml-1 text-xs text-muted-foreground opacity-60",
                                                                title: "{highlight_count} highlights",
                                                                "{highlight_count}"
                                                            }
                                                        }
                                                        if sajda {
                                                            span {
                                                                class: "text-xs text-orange-500",
                                                                title: "Prostration (Sajda)",
                                                                "۩"
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
                    } else if !*loading.read() {
                        div { class: "text-center py-16",
                            div { class: "text-destructive font-medium", "Error loading surah" }
                            p { class: "text-sm text-muted-foreground mt-2", "Failed to load surah. Please try again." }
                            button {
                                class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                onclick: move |_| loaded_key.set(String::new()),
                                "Try Again"
                            }
                        }
                    }
                }
            }

            // Floating toolbar
            if *show_toolbar.read() && selected_count > 0 {
                div { class: "fixed bottom-20 left-1/2 -translate-x-1/2 bg-card border border-border rounded-xl shadow-lg p-3 z-50",
                    div { class: "flex items-center gap-3",
                        span { class: "text-sm text-muted-foreground",
                            "{selected_count} selected"
                        }
                        div { class: "w-px h-6 bg-border" }
                        // Copy
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Copy ayahs",
                            onclick: copy_ayahs,
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z",
                                }
                            }
                        }
                        // Ask AI
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Ask AI about these ayahs",
                            onclick: move |_| {
                                if let Some(data) = surah_data.read().as_ref() {
                                    let selected: Vec<u32> = selected_ayahs.read().iter().copied().collect();
                                    if selected.is_empty() {
                                        return;
                                    }
                                    let reference = format_selected_ayahs_reference(&surah_name_for_ai, &selected, &edition_for_ai);
                                    let text = build_selected_text(data, &selected, &surah_name_for_ai);
                                    let message = format!("Quran passage: {}\n\n{}", reference, text);
                                    let payload = AiChatSeedPayload {
                                        source: "quran".to_string(),
                                        title_hint: Some(reference),
                                        message,
                                    };
                                    let nav = navigator;
                                    spawn(async move {
                                        let account_key = ai_chat_store::current_account_key();
                                        match ai_chat_store::load_chat_state(&account_key).await {
                                            Ok(state) if ai_chat_store::has_saved_conversation_context(&state) => {
                                                pending_ai_chat_seed.set(Some(payload));
                                                show_ai_chat_confirm.set(true);
                                            }
                                            Ok(_) => {
                                                queue_ai_chat_seed(payload);
                                                nav.push(Route::AIChat {});
                                            }
                                            Err(err) => {
                                                log::warn!("Failed to load AI chat state before Quran seed: {err}");
                                                queue_ai_chat_seed(payload);
                                                nav.push(Route::AIChat {});
                                            }
                                        }
                                    });
                                }
                            },
                            SparklesIcon { class: "w-5 h-5".to_string() }
                        }
                        // Highlight
                        if is_authenticated {
                            button {
                                class: "p-2 hover:bg-muted rounded-lg transition",
                                title: "Highlight ayahs",
                                onclick: open_highlight_modal,
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5 text-yellow-500",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path { d: "M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" }
                                }
                            }
                            // Share
                            button {
                                class: "p-2 hover:bg-muted rounded-lg transition",
                                title: "Share ayahs",
                                onclick: move |_| {
                                    if let Some(data) = surah_data.read().as_ref() {
                                        let selected: Vec<u32> = selected_ayahs.read().iter().copied().collect();
                                        if selected.is_empty() {
                                            return;
                                        }
                                        let text = build_selected_text(data, &selected, &surah_name_for_share);
                                        let reference = format_selected_ayahs_reference(&surah_name_for_share, &selected, &edition_for_share);
                                        let url = format!("https://nostr.blue/quran/{}", surah);
                                        share_title.set(reference);
                                        share_url.set(url);
                                        share_content.set(text);
                                        show_share_modal.set(true);
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z",
                                    }
                                }
                            }
                        }
                        // Clear
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Clear selection",
                            onclick: clear_selection,
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M6 18L18 6M6 6l12 12",
                                }
                            }
                        }
                    }
                }
            }

            // Highlight feedback toast
            if let Some((success, message)) = highlight_feedback.read().clone() {
                div {
                    class: "fixed bottom-36 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded-lg shadow-lg text-sm font-medium",
                    class: if success { "bg-green-100 text-green-800 dark:bg-green-900/50 dark:text-green-200" } else { "bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-200" },
                    "{message}"
                }
            }

            // Share modal
            if *show_share_modal.read() {
                ContentShareModal {
                    title: share_title.read().clone(),
                    url: share_url.read().clone(),
                    content_type: ContentType::QuranAyah,
                    image_url: None,
                    content: Some(share_content.read().clone()),
                    on_close: move |_| show_share_modal.set(false),
                }
            }

            // AI chat confirm modal
            if *show_ai_chat_confirm.read() {
                ConfirmModal {
                    title: "Start a new AI chat?".to_string(),
                    message: "This opens AI Chat with the selected ayahs as context and switches away from your current saved conversation.".to_string(),
                    confirm_text: Some("Start new chat".to_string()),
                    cancel_text: Some("Cancel".to_string()),
                    on_confirm: move |_| {
                        if let Some(payload) = pending_ai_chat_seed.read().clone() {
                            queue_ai_chat_seed(payload);
                            navigator.push(Route::AIChat {});
                        }
                        pending_ai_chat_seed.set(None);
                        show_ai_chat_confirm.set(false);
                    },
                    on_cancel: move |_| {
                        pending_ai_chat_seed.set(None);
                        show_ai_chat_confirm.set(false);
                    },
                }
            }

            // Highlight modal
            if *show_highlight_modal.read() {
                HighlightModal {
                    content: pending_highlight_text.read().clone(),
                    reference: pending_highlight_reference.read().clone(),
                    on_confirm: move |comment: Option<String>| {
                        let text = pending_highlight_text.read().clone();
                        let reference = pending_highlight_reference.read().clone();
                        spawn(async move {
                            match quran_store::create_highlight(
                                &text,
                                &reference,
                                surah,
                                0,
                                comment.as_deref(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    log::info!("Quran highlight created");
                                    highlight_feedback.set(Some((true, "Highlight saved".to_string())));
                                    spawn(async move {
                                        crate::platform::timer::sleep_ms(2000).await;
                                        highlight_feedback.set(None);
                                    });
                                }
                                Err(e) => {
                                    log::error!("Failed to create highlight: {}", e);
                                    highlight_feedback.set(Some((false, format!("Failed: {}", e))));
                                    spawn(async move {
                                        crate::platform::timer::sleep_ms(4000).await;
                                        highlight_feedback.set(None);
                                    });
                                }
                            }
                        });
                        selected_ayahs.set(HashSet::new());
                        show_toolbar.set(false);
                        show_highlight_modal.set(false);
                    },
                    on_cancel: move |_| {
                        show_highlight_modal.set(false);
                    },
                }
            }
        }
    }
}

fn build_selected_text(
    data: &CachedSurah,
    ayah_nums: &[u32],
    surah_name: &str,
) -> String {
    let mut lines = Vec::new();
    for &num in ayah_nums {
        if let Some(ayah) = data.arabic.ayahs.iter().find(|a| a.number_in_surah == num) {
            let translation_text = data.translation.as_ref().and_then(|t| {
                t.ayahs
                    .iter()
                    .find(|a| a.number_in_surah == num)
                    .map(|a| a.text.clone())
            });
            let mut line = format!("{}:{} ", surah_name, num);
            line.push_str(&ayah.text);
            if let Some(trans) = translation_text {
                line.push('\n');
                line.push_str(&trans);
            }
            lines.push(line);
        }
    }
    lines.join(" ")
}

fn format_selected_ayahs_reference(
    surah_name: &str,
    ayah_nums: &[u32],
    edition: &str,
) -> String {
    if ayah_nums.len() == 1 {
        format_ayah_reference(surah_name, ayah_nums[0], edition)
    } else if let (Some(&first), Some(&last)) = (ayah_nums.first(), ayah_nums.last()) {
        format!(
            "{}:{}-{} ({})",
            surah_name, first, last, edition
        )
    } else {
        format!("{} ({})", surah_name, edition)
    }
}
