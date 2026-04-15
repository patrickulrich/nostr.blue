use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::stores::blobbi_profile_store;
use crate::stores::blobbi_store;
use crate::utils::nip_bb::*;

use super::egg_preview::EggPreview;

static EGG_CREATION_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnboardingStep {
    #[default]
    Welcome,
    Adopt,
    Naming,
}

#[component]
pub fn OnboardingFlow(on_complete: EventHandler<()>) -> Element {
    let step = use_signal(OnboardingStep::default);
    let chosen_color = use_signal(|| DEFAULT_BASE_COLORS[0].to_string());

    match step() {
        OnboardingStep::Welcome => rsx! {
            WelcomeStep { step }
        },
        OnboardingStep::Adopt => rsx! {
            AdoptStep { step, chosen_color }
        },
        OnboardingStep::Naming => rsx! {
            NamingStep { step, on_complete }
        },
    }
}

#[component]
fn WelcomeStep(mut step: Signal<OnboardingStep>) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center min-h-[60vh] p-8 text-center",
            div { class: "text-6xl mb-6 animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
                "\u{1F95A}"
            }
            h2 { class: "text-2xl font-bold text-foreground mb-3",
                "Welcome to Nostrich!"
            }
            p { class: "text-muted-foreground text-sm max-w-sm mb-8",
                "Adopt a virtual Nostrich pet that lives on Nostr. Care for it, watch it hatch from an egg, and help it grow. Your Nostrich is unique and lives forever on the network."
            }
            div { class: "grid grid-cols-3 gap-4 mb-8 max-w-xs",
                div { class: "flex flex-col items-center gap-1",
                    span { class: "text-2xl", "\u{1F95A}" }
                    span { class: "text-[10px] text-muted-foreground", "Hatch" }
                }
                div { class: "flex flex-col items-center gap-1",
                    span { class: "text-2xl", "\u{1F426}" }
                    span { class: "text-[10px] text-muted-foreground", "Nurture" }
                }
                div { class: "flex flex-col items-center gap-1",
                    span { class: "text-2xl", "\u{1F9F8}" }
                    span { class: "text-[10px] text-muted-foreground", "Evolve" }
                }
            }
            button {
                class: "px-8 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-xl font-medium transition text-lg",
                onclick: move |_| step.set(OnboardingStep::Adopt),
                "Choose Your Egg"
            }
        }
    }
}

#[component]
fn AdoptStep(mut step: Signal<OnboardingStep>, chosen_color: Signal<String>) -> Element {
    rsx! {
        div { class: "flex flex-col items-center p-6",
            h2 { class: "text-lg font-bold mb-4",
                "Pick a Color"
            }
            EggPreview { base_color: chosen_color() }
            div { class: "flex flex-wrap justify-center gap-3 mt-6 mb-4",
                for color in DEFAULT_BASE_COLORS {
                    button {
                        class: if chosen_color() == *color {
                            "w-10 h-10 rounded-full border-2 border-foreground ring-2 ring-offset-2 ring-blue-500 transition"
                        } else {
                            "w-10 h-10 rounded-full border-2 border-transparent hover:border-foreground/30 transition"
                        },
                        style: "background-color: {color}",
                        onclick: move |_| chosen_color.set(color.to_string()),
                    }
                }
            }
            p { class: "text-xs text-muted-foreground mb-6",
                "Adoption fee: {ADOPTION_FEE} coins"
            }
            button {
                class: "px-6 py-2.5 bg-blue-500 hover:bg-blue-600 text-white rounded-xl font-medium transition",
                onclick: move |_| step.set(OnboardingStep::Naming),
                "This One!"
            }
        }
    }
}

#[component]
fn NamingStep(mut step: Signal<OnboardingStep>, on_complete: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let chosen_color = use_signal(|| DEFAULT_BASE_COLORS[0].to_string());

    rsx! {
        div { class: "flex flex-col items-center justify-center min-h-[60vh] p-8 text-center",
            div { class: "text-6xl mb-4 animate-[blobbi-incubation-glow_2s_ease-in-out_infinite]",
                "\u{1F95A}"
            }
            h2 { class: "text-lg font-bold mb-2",
                "Name your Nostrich!"
            }
            p { class: "text-sm text-muted-foreground mb-6",
                "This name will be stored with your pet on Nostr"
            }
            input {
                class: "w-full max-w-xs px-4 py-2.5 rounded-xl bg-card border border-border text-center text-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                r#type: "text",
                placeholder: "Name your Nostrich...",
                maxlength: "20",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }

            if let Some(err) = error_msg() {
                div { class: "mt-3 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-xs max-w-xs",
                    "{err}"
                }
            }

            div { class: "flex gap-3 mt-6",
                button {
                    class: "px-5 py-2 rounded-xl border border-border hover:bg-accent transition text-sm",
                    disabled: creating(),
                    onclick: move |_| step.set(OnboardingStep::Adopt),
                    "Back"
                }
                button {
                    class: if creating() {
                        "px-6 py-2.5 bg-blue-400 text-white rounded-xl font-medium transition opacity-70 cursor-not-allowed"
                    } else {
                        "px-6 py-2.5 bg-blue-500 hover:bg-blue-600 text-white rounded-xl font-medium transition disabled:opacity-50"
                    },
                    disabled: name().trim().is_empty() || creating(),
                    onclick: move |_| {
                        let n = name();
                        let color = chosen_color();
                        creating.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match create_egg_and_profile(&n, &color).await {
                                Ok(()) => on_complete.call(()),
                                Err(e) => {
                                    log::error!("Failed to create egg: {}", e);
                                    error_msg.set(Some(e));
                                    creating.set(false);
                                }
                            }
                        });
                    },
                    if creating() { "Creating..." } else { "Adopt!" }
                }
            }
        }
    }
}

async fn create_egg_and_profile(name: &str, color: &str) -> Result<(), String> {
    if EGG_CREATION_IN_FLIGHT.load(Ordering::SeqCst) {
        return Err("Egg creation already in progress".to_string());
    }
    EGG_CREATION_IN_FLIGHT.store(true, Ordering::SeqCst);

    let result = create_egg_and_profile_inner(name, color).await;

    EGG_CREATION_IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

async fn create_egg_and_profile_inner(name: &str, color: &str) -> Result<(), String> {
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
        return Err(format!("Not enough coins. Need {} but have {}.", ADOPTION_FEE, profile.coins));
    }
    profile.coins = profile.coins.saturating_sub(ADOPTION_FEE);

    let pet_id = crate::utils::generate_option_id();
    let d = blobbi_d_tag(&pubkey, &pet_id);
    let now = nostr_sdk::Timestamp::now().as_secs();

    let seed = crate::components::blobbi::core::seed::derive_seed(&pubkey, &d, now);
    let mut visual = crate::components::blobbi::core::seed::derive_visual_traits_from_seed(&seed);
    visual.base_color = color.to_string();

    let is_divine = {
        let hash = crate::components::blobbi::core::seed::djb2_hash(&format!("{}:{}", pubkey, pet_id));
        (hash % 100) as f64 / 100.0 < DIVINE_EGG_CHANCE
    };

    let (_final_color, _secondary, theme_tag, crossover_tag) = if is_divine {
        visual.base_color = DIVINE_PRIMARY_GREEN.to_string();
        visual.secondary_color = None;
        (DIVINE_PRIMARY_GREEN.to_string(), None, Some("divine".to_string()), Some("divine".to_string()))
    } else {
        (color.to_string(), visual.secondary_color.clone(), None, None)
    };

    let mut blobbi = crate::components::blobbi::core::types::BlobbiCompanion {
        d: d.clone(),
        name: name.to_string(),
        stage: BlobbiStage::Egg,
        state: BlobbiState::Active,
        stats: crate::components::blobbi::core::types::BlobbiStats::full(),
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

    let record_tags = vec![
        ("record_type", "birth".to_string()),
        ("generation", "1".to_string()),
    ];
    let record_event = crate::components::blobbi::core::builders::build_record_event(
        &d,
        "birth",
        1,
        record_tags,
        format!("{} was born!", name),
    );
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    client.send_event_builder(record_event).await
        .map_err(|e| format!("Failed to publish birth record: {}", e))?;

    profile.has.push(d.clone());
    profile.current_companion = Some(d.clone());
    profile.onboarding_done = true;
    if profile.starter_blobbi.is_none() {
        profile.starter_blobbi = Some(d.clone());
    }
    profile.lifetime_blobbis = profile.lifetime_blobbis.saturating_add(1);
    crate::components::blobbi::core::builders::publish_profile(&profile).await?;
    blobbi_profile_store::set_profile(profile);

    blobbi_store::update_blobbi_in_collection(&blobbi);
    blobbi_store::select_blobbi(d);

    Ok(())
}
