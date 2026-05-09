use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::components::blobbi::core::types::{BlobbiCompanion, BlobbiStage, BlobbiStats};
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::components::blobbi::visual::egg_visual::EggVisual;
use crate::hooks::blobbi::use_typewriter::TypewriterText;
use crate::stores::blobbi_profile_store;
use crate::stores::blobbi_store;
use crate::utils::nip_bb::*;

static SETUP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CeremonyPhase {
    #[default]
    Setup,
    Darkness,
    EggEntrance,
    FirstWords,
    EggBreathing,
    LightCrack,
    ProgressCracks,
    HeavyCracks,
    Burst,
    Reveal,
    Dialog,
    Naming,
    Complete,
}

#[component]
pub fn HatchingCeremony(
    blobbi: Option<BlobbiCompanion>,
    egg_only: bool,
    on_complete: EventHandler<String>,
) -> Element {
    let mut phase = use_signal(CeremonyPhase::default);
    let mut created_blobbi: Signal<Option<BlobbiCompanion>> = use_signal(|| None);
    let mut setup_done = use_signal(|| false);
    let mut setup_error: Signal<Option<String>> = use_signal(|| None);
    let mut name = use_signal(String::new);
    let mut naming_busy = use_signal(|| false);
    let mut naming_error: Signal<Option<String>> = use_signal(|| None);
    let mut dialog_line: Signal<u32> = use_signal(|| 0);
    let mut dialog_done: Signal<bool> = use_signal(|| false);

    let active_blobbi = created_blobbi().as_ref().or(blobbi.as_ref()).cloned();

    if phase() == CeremonyPhase::Setup && !setup_done() {
        setup_done.set(true);

        if let Some(egg) = blobbi.as_ref() {
            created_blobbi.set(Some(egg.clone()));
            phase.set(CeremonyPhase::Darkness);
        } else {
            spawn(async move {
                match run_silent_setup().await {
                    Ok(egg) => {
                        created_blobbi.set(Some(egg));
                        phase.set(CeremonyPhase::Darkness);
                    }
                    Err(e) => {
                        log::error!("Ceremony setup failed: {}", e);
                        setup_error.set(Some(e));
                    }
                }
            });
        }
    }

    if setup_error().is_some() {
        let err = setup_error().unwrap_or_default();
        return rsx! {
            div { class: "fixed inset-0 z-50 bg-black flex items-center justify-center p-8",
                div { class: "text-center",
                    p { class: "text-red-400 text-sm mb-4", "{err}" }
                    button {
                        class: "px-4 py-2 bg-white/10 text-white rounded-lg text-sm hover:bg-white/20 transition",
                        onclick: move |_| {
                            setup_done.set(false);
                            setup_error.set(None);
                        },
                        "Retry"
                    }
                }
            }
        };
    }

    if active_blobbi.is_none() {
        return rsx! {
            div { class: "fixed inset-0 z-50 bg-black flex items-center justify-center",
                p {
                    class: "text-white/50 text-lg font-mono animate-[onboard-soft-fade-in_1.5s_ease-out_forwards]",
                    "Something stirs..."
                }
            }
        };
    }

    let blobbi_data = active_blobbi.unwrap();
    let base_color = blobbi_data.visual_traits.base_color.clone();
    let blobbi_name = blobbi_data.display_name();

    let mut started = use_signal(|| false);
    if phase() == CeremonyPhase::Darkness && !started() {
        started.set(true);
        let is_egg_only = egg_only;
        spawn(async move {
            crate::platform::timer::sleep_ms(2000).await;
            phase.set(CeremonyPhase::EggEntrance);

            crate::platform::timer::sleep_ms(1500).await;
            phase.set(CeremonyPhase::FirstWords);

            crate::platform::timer::sleep_ms(2500).await;
            phase.set(CeremonyPhase::EggBreathing);

            if is_egg_only {
                crate::platform::timer::sleep_ms(2500).await;
                phase.set(CeremonyPhase::Complete);
            }
        });
    }

    if phase() == CeremonyPhase::Burst {
        let mut phase = phase;
        spawn(async move {
            crate::platform::timer::sleep_ms(1400).await;
            phase.set(CeremonyPhase::Reveal);
        });
    }

    if phase() == CeremonyPhase::Reveal && !egg_only {
        let mut phase = phase;
        spawn(async move {
            crate::platform::timer::sleep_ms(2200).await;
            phase.set(CeremonyPhase::Dialog);
        });
    }

    if phase() == CeremonyPhase::Complete {
        if egg_only {
            spawn(async move {
                if let Err(e) = finish_egg_only_onboarding().await {
                    log::error!("Failed to finalize onboarding: {}", e);
                }
                on_complete.call(String::new());
            });
        }
        return rsx! {
            div { class: "fixed inset-0 z-50 bg-black flex items-center justify-center animate-[blobbi-fade-to-white_2s_ease-in_forwards]" }
        };
    }

    let current_phase = phase();

    let crack_level = match current_phase {
        CeremonyPhase::EggBreathing => Some(0),
        CeremonyPhase::LightCrack => Some(1),
        CeremonyPhase::ProgressCracks => Some(2),
        CeremonyPhase::HeavyCracks => Some(3),
        CeremonyPhase::Burst => Some(3),
        _ => None,
    };

    if current_phase == CeremonyPhase::Naming {
        return rsx! {
            div { class: "fixed inset-0 z-50 flex items-center justify-center overflow-hidden select-none",
                style: "background: radial-gradient(ellipse at 50% 45%, rgb(60,140,180) 0%, rgb(70,160,195) 25%, rgb(85,175,205) 50%, rgb(100,190,210) 75%, rgb(115,195,195) 100%);",

                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    div {
                        class: "w-[900px] h-[900px] rounded-full opacity-70 blur-[30px] animate-[onboard-golden-rotate_20s_linear_infinite]",
                        style: "background: conic-gradient(from 0deg, rgba(255,250,230,0.15) 0deg, rgba(255,220,150,0.1) 45deg, rgba(255,250,230,0.15) 90deg, rgba(255,220,150,0.1) 135deg, rgba(255,250,230,0.15) 180deg, rgba(255,220,150,0.1) 225deg, rgba(255,250,230,0.15) 270deg, rgba(255,220,150,0.1) 315deg, rgba(255,250,230,0.15) 360deg);",
                    }
                }

                SparkleDecorations { phase_reveal: true }

                div { class: "relative flex flex-col items-center justify-center z-10 px-8",
                    div { class: "animate-[onboard-golden-fadein_1s_ease-out_forwards]",
                        {
                            let mut reveal_blobbi = blobbi_data.clone();
                            reveal_blobbi.stage = BlobbiStage::Baby;
                            rsx! {
                                BlobbiVisual {
                                    blobbi: reveal_blobbi,
                                    size: Some("160".to_string()),
                                }
                            }
                        }
                    }
                    p { class: "text-white text-xl font-mono mt-8 text-center",
                        TypewriterText {
                            key: "naming-prompt",
                            text: "Every life deserves a name.\nWhat will you call this one?".to_string(),
                            speed_ms: Some(35),
                        }
                    }
                    div { class: "mt-6 w-full max-w-xs",
                        input {
                            class: "w-full px-4 py-3 rounded-full bg-white/10 border border-white/20 text-white text-center text-lg focus:outline-none focus:shadow-[0_0_15px_rgba(255,255,255,0.15),0_0_40px_rgba(255,250,230,0.08)] placeholder-white/30",
                            r#type: "text",
                            placeholder: "Name your Blobbi...",
                            maxlength: "20",
                            value: "{name}",
                            autofocus: true,
                            oninput: move |e| name.set(e.value()),
                        }
                    }
                    if let Some(err) = naming_error() {
                        p { class: "text-red-400 text-xs mt-2", "{err}" }
                    }
                    div { class: "mt-4",
                        button {
                            class: if naming_busy() || name().trim().is_empty() {
                                "px-6 py-2.5 rounded-xl font-medium transition bg-white/10 text-white/40 cursor-not-allowed"
                            } else {
                                "px-6 py-2.5 rounded-xl font-medium transition bg-white/20 text-white hover:bg-white/30"
                            },
                            disabled: name().trim().is_empty() || naming_busy(),
                            onclick: move |_| {
                                let n = name().trim().to_string();
                                if n.is_empty() { return; }
                                let b = blobbi_data.clone();
                                naming_busy.set(true);
                                naming_error.set(None);
                                spawn(async move {
                                    match finish_naming(&b, &n).await {
                                        Ok(()) => {
                                            phase.set(CeremonyPhase::Complete);
                                            on_complete.call(n);
                                        }
                                        Err(e) => {
                                            log::error!("Naming failed: {}", e);
                                            naming_error.set(Some(e));
                                            naming_busy.set(false);
                                        }
                                    }
                                });
                            },
                            if naming_busy() { "Naming..." } else { "That's the one." }
                        }
                    }
                }
            }
        };
    }

    if current_phase == CeremonyPhase::Dialog {
        let lines = ["Something stirs...", "A tiny life has chosen you. It knows only warmth, and your presence."];
        let line_idx = dialog_line() as usize;
        let line_text = lines.get(line_idx).map(|s| s.to_string()).unwrap_or_default();

        return rsx! {
            div {
                class: "fixed inset-0 z-50 flex items-center justify-center overflow-hidden select-none",
                style: "background: radial-gradient(ellipse at 50% 45%, rgb(60,140,180) 0%, rgb(70,160,195) 25%, rgb(85,175,205) 50%, rgb(100,190,210) 75%, rgb(115,195,195) 100%);",
                onclick: move |_| {
                    if !dialog_done() {
                        dialog_done.set(true);
                        return;
                    }
                    let next = dialog_line() + 1;
                    if (next as usize) < lines.len() {
                        dialog_line.set(next);
                        dialog_done.set(false);
                    } else {
                        phase.set(CeremonyPhase::Naming);
                    }
                },

                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    div {
                        class: "w-[900px] h-[900px] rounded-full opacity-70 blur-[30px] animate-[onboard-golden-rotate_20s_linear_infinite]",
                        style: "background: conic-gradient(from 0deg, rgba(255,250,230,0.15) 0deg, rgba(255,220,150,0.1) 45deg, rgba(255,250,230,0.15) 90deg, rgba(255,220,150,0.1) 135deg, rgba(255,250,230,0.15) 180deg, rgba(255,220,150,0.1) 225deg, rgba(255,250,230,0.15) 270deg, rgba(255,220,150,0.1) 315deg, rgba(255,250,230,0.15) 360deg);",
                    }
                }

                SparkleDecorations { phase_reveal: true }

                div {
                    class: "relative flex flex-col items-center justify-center z-10 px-8",
                    style: "background: radial-gradient(ellipse at center, rgba(0,30,50,0.40) 0%, rgba(0,30,50,0.18) 35%, transparent 65%); mask: radial-gradient(ellipse at center, black 25%, transparent 65%);",

                    span { class: "text-white/40 text-xs uppercase tracking-widest mb-3", "???" }
                    p { class: "text-white text-xl font-mono text-center",
                        TypewriterText {
                            key: "dialog-{line_idx}",
                            text: line_text,
                            speed_ms: Some(35),
                        }
                    }
                    p {
                        class: "text-white/30 text-xs mt-8 animate-[onboard-continue-pulse_2.5s_ease-in-out_infinite]",
                        "▼"
                    }
                }
            }
        };
    }

    let egg_animation = match current_phase {
        CeremonyPhase::EggEntrance => {
            "animate-[onboard-soft-fade-in_1s_ease-out_forwards]"
        }
        CeremonyPhase::EggBreathing => {
            "animate-[onboard-egg-breathe_2s_ease-in-out_infinite]"
        }
        CeremonyPhase::LightCrack => {
            "animate-[onboard-egg-shake-light_0.5s_ease-in-out_infinite]"
        }
        CeremonyPhase::ProgressCracks => {
            "animate-[onboard-egg-shake-medium_0.4s_ease-in-out_infinite]"
        }
        CeremonyPhase::HeavyCracks => {
            "animate-[onboard-egg-shake-heavy_0.3s_ease-in-out_infinite]"
        }
        CeremonyPhase::Burst => "animate-[onboard-egg-burst_1s_ease-in_forwards]",
        _ => "",
    };

    let show_egg = matches!(
        current_phase,
        CeremonyPhase::EggEntrance
            | CeremonyPhase::EggBreathing
            | CeremonyPhase::LightCrack
            | CeremonyPhase::ProgressCracks
            | CeremonyPhase::HeavyCracks
            | CeremonyPhase::Burst
    );

    let show_sparkles = matches!(
        current_phase,
        CeremonyPhase::ProgressCracks | CeremonyPhase::HeavyCracks
    );

    let crack_for_egg = crack_level.filter(|_| !egg_only);

    let glow_opacity = match current_phase {
        CeremonyPhase::LightCrack => "0.25",
        CeremonyPhase::ProgressCracks => "0.35",
        CeremonyPhase::HeavyCracks => "0.50",
        _ => "0.15",
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("../blobbi.css") }

        div {
            class: "fixed inset-0 z-50 bg-black flex items-center justify-center overflow-hidden select-none",
            onclick: {
                let mut phase = phase;
                move |_| {
                    match phase() {
                        CeremonyPhase::EggBreathing if !egg_only => {
                            phase.set(CeremonyPhase::LightCrack);
                        }
                        CeremonyPhase::LightCrack => {
                            phase.set(CeremonyPhase::ProgressCracks);
                        }
                        CeremonyPhase::ProgressCracks => {
                            phase.set(CeremonyPhase::HeavyCracks);
                        }
                        CeremonyPhase::HeavyCracks => {
                            phase.set(CeremonyPhase::Burst);
                            spawn(async move {
                                crate::platform::timer::sleep_ms(1400).await;
                                phase.set(CeremonyPhase::Reveal);
                            });
                        }
                        CeremonyPhase::Reveal if !egg_only => {
                            phase.set(CeremonyPhase::Dialog);
                        }
                        _ => {}
                    }
                }
            },

            if current_phase == CeremonyPhase::Burst {
                div {
                    class: "absolute inset-0 bg-white animate-[onboard-screen-flash_2s_ease-out_forwards]",
                }
            }

            if current_phase == CeremonyPhase::Reveal {
                div {
                    class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    div {
                        class: "w-[700px] h-[700px] rounded-full blur-[15px] animate-[onboard-golden-fadein_2s_ease-out_forwards]",
                        style: "background: radial-gradient(circle, rgba(255,215,0,0.25) 0%, rgba(255,215,0,0.08) 40%, transparent 70%);",
                    }
                }
                div {
                    class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    div {
                        class: "w-80 h-80 rounded-full animate-[onboard-golden-fadein_1s_ease-out_forwards]",
                        style: "background: radial-gradient(circle, rgba(255,255,255,0.7) 0%, transparent 70%);",
                    }
                }
            }

            if show_sparkles {
                SparkleDecorations { phase_reveal: false }
            }

            div { class: "relative flex flex-col items-center justify-center z-10",

                if current_phase == CeremonyPhase::Darkness {
                    p {
                        class: "text-white/50 text-lg font-mono animate-[onboard-soft-fade-in_1.5s_ease-out_forwards]",
                        "Something stirs..."
                    }
                }

                if show_egg {
                    div {
                        class: "relative {egg_animation}",

                        div {
                            class: "absolute inset-0 -m-8 rounded-full blur-xl pointer-events-none",
                            style: "background: radial-gradient(circle, rgba(255,215,0,{glow_opacity}) 0%, transparent 70%);",
                        }

                        EggVisual {
                            base_color: base_color.clone(),
                            crack_level: crack_for_egg,
                        }
                    }
                }

                if current_phase == CeremonyPhase::FirstWords {
                    p { class: "text-white text-xl font-mono",
                        TypewriterText {
                            key: "first-words",
                            text: "You found an egg!".to_string(),
                            speed_ms: Some(40),
                        }
                    }
                }

                if current_phase == CeremonyPhase::EggBreathing {
                    div { class: "mt-4 animate-[onboard-soft-fade-in_0.5s_ease-out_forwards]",
                        if egg_only {
                            p { class: "text-white/70 text-lg font-mono",
                                TypewriterText {
                                    key: "breathing",
                                    text: "Your journey begins...".to_string(),
                                    speed_ms: Some(50),
                                }
                            }
                        } else {
                            p { class: "text-white/70 text-lg font-mono",
                                TypewriterText {
                                    key: "breathing",
                                    text: "It's warm... Tap to crack".to_string(),
                                    speed_ms: Some(50),
                                }
                            }
                        }
                    }
                }

                if current_phase == CeremonyPhase::HeavyCracks {
                    p {
                        class: "text-white text-xl font-bold mt-4 animate-[onboard-soft-fade-in_0.5s_ease-out_forwards]",
                        "It's hatching!"
                    }
                }

                if current_phase == CeremonyPhase::Reveal {
                    div { class: "animate-[onboard-golden-fadein_1s_ease-out_forwards]",
                        {
                            let mut reveal_blobbi = blobbi_data.clone();
                            reveal_blobbi.stage = BlobbiStage::Baby;
                            rsx! {
                                BlobbiVisual {
                                    blobbi: reveal_blobbi,
                                    size: Some("200".to_string()),
                                }
                            }
                        }
                    }
                    p { class: "text-white text-2xl font-bold mt-6",
                        TypewriterText {
                            key: "reveal-name",
                            text: format!("{} has arrived!", blobbi_name),
                            speed_ms: Some(45),
                        }
                    }
                    p {
                        class: "text-white/40 text-sm mt-8 animate-[onboard-continue-pulse_2s_ease-in-out_infinite]",
                        "Tap to continue"
                    }
                }
            }

            if current_phase == CeremonyPhase::Burst {
                ParticleBurst {}
            }

            if current_phase == CeremonyPhase::Reveal {
                DriftingMotes {}
            }
        }
    }
}

async fn run_silent_setup() -> Result<BlobbiCompanion, String> {
    if SETUP_IN_FLIGHT.load(Ordering::SeqCst) {
        return Err("Setup already in progress".to_string());
    }
    SETUP_IN_FLIGHT.store(true, Ordering::SeqCst);

    let result = run_silent_setup_inner().await;

    SETUP_IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

async fn run_silent_setup_inner() -> Result<BlobbiCompanion, String> {
    let pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let has_profile = blobbi_profile_store::get_profile().is_some();
    if !has_profile {
        let kind0_name = crate::stores::profiles::get_profile(&pubkey)
            .and_then(|m| m.name);

        let profile_d = profile_d_tag(&pubkey);
        let profile = crate::components::blobbi::core::types::BlobbonautProfile {
            d: profile_d,
            name: kind0_name.unwrap_or_else(|| "Blobbonaut".to_string()),
            coins: INITIAL_BLOBBONAUT_COINS,
            onboarding_done: false,
            ..Default::default()
        };

        crate::components::blobbi::core::builders::publish_profile(&profile).await?;
        blobbi_profile_store::set_profile(profile);
    }

    let mut profile = blobbi_profile_store::get_profile()
        .ok_or("Profile not found")?;

    if profile.coins < ADOPTION_FEE {
        return Err(format!(
            "Not enough coins. Need {} but have {}.",
            ADOPTION_FEE, profile.coins
        ));
    }
    profile.coins = profile.coins.saturating_sub(ADOPTION_FEE);

    let pet_id = generate_blobbi_pet_id();
    let d = blobbi_d_tag(&pubkey, &pet_id);
    let now = nostr_sdk::Timestamp::now().as_secs();

    let seed = crate::components::blobbi::core::seed::derive_seed(&pubkey, &d, now);
    let mut visual =
        crate::components::blobbi::core::seed::derive_visual_traits_from_seed(&seed);

    let is_divine = {
        let hash = crate::components::blobbi::core::seed::djb2_hash(&format!(
            "{}:{}",
            pubkey, pet_id
        ));
        (hash % 100) as f64 / 100.0 < DIVINE_EGG_CHANCE
    };

    let theme_tag;
    let crossover_tag;
    if is_divine {
        visual.base_color = DIVINE_PRIMARY_GREEN.to_string();
        visual.secondary_color = None;
        theme_tag = Some("divine".to_string());
        crossover_tag = Some("divine".to_string());
    } else {
        theme_tag = None;
        crossover_tag = None;
    }

    let mut blobbi = BlobbiCompanion {
        d: d.clone(),
        name: String::new(),
        stage: BlobbiStage::Egg,
        state: crate::components::blobbi::core::types::BlobbiState::Active,
        stats: BlobbiStats::full(),
        visual_traits: visual,
        seed: Some(seed),
        last_decay_at: Some(now),
        last_interaction: Some(now),
        source: Some("user".to_string()),
        egg_temperature: Some(STAT_MAX),
        egg_status: Some("warm".to_string()),
        shell_integrity: Some(STAT_MAX),
        theme: theme_tag,
        crossover_app: crossover_tag,
        ..Default::default()
    };

    crate::components::blobbi::actions::hatch_tasks::initialize_tasks_for_stage(&mut blobbi);

    crate::components::blobbi::core::builders::publish_blobbi_state(&blobbi).await?;

    profile.has.push(d.clone());
    profile.current_companion = Some(d.clone());
    if profile.starter_blobbi.is_none() {
        profile.starter_blobbi = Some(d.clone());
    }
    profile.lifetime_blobbis = profile.lifetime_blobbis.saturating_add(1);
    crate::components::blobbi::core::builders::publish_profile(&profile).await?;
    blobbi_profile_store::set_profile(profile);

    blobbi_store::update_blobbi_in_collection(&blobbi);
    blobbi_store::select_blobbi(d);

    Ok(blobbi)
}

async fn finish_egg_only_onboarding() -> Result<(), String> {
    if let Some(mut profile) = blobbi_profile_store::get_profile() {
        profile.onboarding_done = true;
        crate::components::blobbi::core::builders::publish_profile(&profile).await?;
        blobbi_profile_store::set_profile(profile);
    }
    Ok(())
}

async fn finish_naming(blobbi: &BlobbiCompanion, name: &str) -> Result<(), String> {
    let pubkey = crate::stores::auth_store::get_pubkey().unwrap_or_default();
    let mut named = blobbi.clone();
    named.name = name.to_string();

    let updated = crate::components::blobbi::actions::stage_transition::hatch_egg(&named, &pubkey);

    crate::components::blobbi::core::builders::publish_blobbi_state(&updated).await?;

    let record_tags = vec![
        ("record_type", "birth".to_string()),
        ("generation", "1".to_string()),
    ];
    let record_event = crate::components::blobbi::core::builders::build_record_event(
        &updated.d,
        "birth",
        1,
        record_tags,
        format!("{} was born!", name),
    );
    let event = crate::stores::publish_queue::signing::sign_event_builder(record_event)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;

    if let Some(mut profile) = blobbi_profile_store::get_profile() {
        profile.onboarding_done = true;
        crate::components::blobbi::core::builders::publish_profile(&profile).await?;
        blobbi_profile_store::set_profile(profile);
    }

    blobbi_store::update_blobbi_in_collection(&updated);

    Ok(())
}

#[component]
fn SparkleDecorations(phase_reveal: bool) -> Element {
    let inner_ring: [(f64, f64); 20] = [
        (50.0, 25.0), (60.0, 27.0), (70.0, 32.0), (78.0, 40.0),
        (82.0, 50.0), (78.0, 60.0), (70.0, 68.0), (60.0, 73.0),
        (50.0, 75.0), (40.0, 73.0), (30.0, 68.0), (22.0, 60.0),
        (18.0, 50.0), (22.0, 40.0), (30.0, 32.0), (40.0, 27.0),
        (55.0, 20.0), (65.0, 30.0), (35.0, 30.0), (45.0, 20.0),
    ];

    let outer_ring: [(f64, f64); 16] = [
        (50.0, 15.0), (65.0, 18.0), (78.0, 25.0), (87.0, 35.0),
        (90.0, 50.0), (87.0, 65.0), (78.0, 75.0), (65.0, 82.0),
        (50.0, 85.0), (35.0, 82.0), (22.0, 75.0), (13.0, 65.0),
        (10.0, 50.0), (13.0, 35.0), (22.0, 25.0), (35.0, 18.0),
    ];

    let scattered: [(f64, f64); 24] = [
        (12.0, 12.0), (88.0, 8.0), (5.0, 55.0), (92.0, 45.0),
        (15.0, 85.0), (85.0, 88.0), (50.0, 5.0), (30.0, 92.0),
        (70.0, 10.0), (8.0, 30.0), (93.0, 70.0), (45.0, 95.0),
        (55.0, 3.0), (75.0, 90.0), (25.0, 5.0), (95.0, 25.0),
        (3.0, 75.0), (60.0, 92.0), (40.0, 8.0), (82.0, 15.0),
        (18.0, 95.0), (68.0, 3.0), (32.0, 88.0), (90.0, 58.0),
    ];

    let base_class = if phase_reveal {
        "animate-[onboard-sparkle-twinkle_2s_ease-in-out_infinite]"
    } else {
        "animate-[onboard-sparkle-twinkle_1.5s_ease-in-out_infinite]"
    };

    rsx! {
        div { class: "absolute inset-0 pointer-events-none overflow-hidden",
            for (i, (x, y)) in inner_ring.iter().enumerate() {
                div {
                    key: "inner-{i}",
                    class: "absolute rounded-full {base_class}",
                    style: format!(
                        "left: {}%; top: {}%; width: {}px; height: {}px; animation-delay: {}s; background: {}; opacity: 0.7;",
                        x, y,
                        if i % 2 == 0 { 6 } else { 8 },
                        if i % 2 == 0 { 6 } else { 8 },
                        i as f64 * 0.15,
                        if i % 2 == 0 {
                            "radial-gradient(circle, rgba(255,255,255,1) 0%, rgba(255,255,255,0.4) 40%, transparent 70%)"
                        } else {
                            "radial-gradient(circle, rgba(255,240,130,1) 0%, rgba(255,220,80,0.3) 50%, transparent 70%)"
                        },
                    ),
                }
            }

            for (i, (x, y)) in outer_ring.iter().enumerate() {
                div {
                    key: "outer-{i}",
                    class: "absolute rounded-full {base_class}",
                    style: format!(
                        "left: {}%; top: {}%; width: {}px; height: {}px; animation-delay: {}s; background: {}; opacity: 0.5;",
                        x, y,
                        if i % 2 == 0 { 5 } else { 7 },
                        if i % 2 == 0 { 5 } else { 7 },
                        i as f64 * 0.25,
                        if i % 2 == 0 {
                            "radial-gradient(circle, rgba(255,255,255,0.8) 0%, rgba(255,255,255,0.3) 40%, transparent 70%)"
                        } else {
                            "radial-gradient(circle, rgba(255,240,130,0.8) 0%, rgba(255,220,80,0.2) 50%, transparent 70%)"
                        },
                    ),
                }
            }

            for (i, (x, y)) in scattered.iter().enumerate() {
                div {
                    key: "scatter-{i}",
                    class: "absolute rounded-full {base_class}",
                    style: format!(
                        "left: {}%; top: {}%; width: {}px; height: {}px; animation-delay: {}s; background: {}; opacity: 0.4;",
                        x, y,
                        if i % 3 == 0 { 4 } else { 6 },
                        if i % 3 == 0 { 4 } else { 6 },
                        i as f64 * 0.18,
                        if i % 2 == 0 {
                            "radial-gradient(circle, rgba(255,255,255,0.6) 0%, transparent 70%)"
                        } else {
                            "radial-gradient(circle, rgba(255,240,130,0.6) 0%, transparent 70%)"
                        },
                    ),
                }
            }
        }
    }
}

#[component]
fn DriftingMotes() -> Element {
    let motes: [(f64, f64, u32); 10] = [
        (45.0, 90.0, 0),
        (55.0, 85.0, 500),
        (50.0, 95.0, 300),
        (40.0, 88.0, 800),
        (60.0, 92.0, 200),
        (35.0, 95.0, 600),
        (65.0, 87.0, 400),
        (48.0, 93.0, 700),
        (52.0, 89.0, 100),
        (42.0, 91.0, 900),
    ];

    rsx! {
        div { class: "absolute inset-0 pointer-events-none overflow-hidden",
            for (i, (x, y, delay)) in motes.iter().enumerate() {
                div {
                    key: "mote-{i}",
                    class: "absolute rounded-full animate-[onboard-sparkle-drift_5s_ease-out_infinite]",
                    style: format!(
                        "left: {}%; top: {}%; width: {}px; height: {}px; animation-delay: {}ms; background: radial-gradient(circle, rgba(255,240,130,0.8) 0%, rgba(255,220,80,0.3) 50%, transparent 70%);",
                        x, y,
                        if i % 2 == 0 { 6 } else { 8 },
                        if i % 2 == 0 { 6 } else { 8 },
                        delay,
                    ),
                }
            }
        }
    }
}

#[component]
fn ParticleBurst() -> Element {
    let particles: [(f64, f64, &'static str, u32); 7] = [
        (50.0, 50.0, "w-3 h-3 bg-yellow-400/80", 0),
        (45.0, 40.0, "w-2 h-2 bg-orange-400/70", 100),
        (55.0, 35.0, "w-2.5 h-2.5 bg-yellow-300/80", 200),
        (40.0, 55.0, "w-2 h-2 bg-amber-400/70", 150),
        (60.0, 45.0, "w-3 h-3 bg-yellow-200/80", 50),
        (48.0, 30.0, "w-1.5 h-1.5 bg-white/80", 250),
        (52.0, 60.0, "w-2 h-2 bg-yellow-400/70", 100),
    ];

    rsx! {
        div { class: "absolute inset-0 pointer-events-none overflow-hidden",
            for (i, (x, y, size_class, delay)) in particles.iter().enumerate() {
                div {
                    key: "{i}",
                    class: format!("absolute rounded-full animate-[onboard-particle-rise_1.5s_ease-out_forwards] {}", size_class),
                    style: format!("left: {}%; top: {}%; animation-delay: {}ms;", x, y, delay),
                }
            }
        }
    }
}
