use crate::components::{ZapGoalCard, ZapModal};
use crate::stores::zap_goals_store::{
    self, fetch_goal_progress_batch, fetch_project_goals, PROJECT_DONATION_LUD16,
    PROJECT_DONATION_NPUB,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use qrcode::render::svg;
use qrcode::QrCode;

fn donation_qr_svg() -> Result<String, String> {
    let code = QrCode::new(format!("lightning:{PROJECT_DONATION_LUD16}"))
        .map_err(|e| format!("Failed to generate QR code: {e}"))?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#0f172a"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

fn load_project_goals(
    mut goals: Signal<Vec<zap_goals_store::ZapGoalProgress>>,
    mut loading: Signal<bool>,
    mut error_message: Signal<Option<String>>,
    request_generation: &Signal<u32>,
) {
    let mut request_generation = *request_generation;
    let generation = request_generation.peek().wrapping_add(1);
    request_generation.set(generation);
    loading.set(true);
    error_message.set(None);
    spawn(async move {
        let result = match fetch_project_goals(6).await {
            Ok(project_goals) => fetch_goal_progress_batch(&project_goals).await,
            Err(error) => Err(error),
        };

        if *request_generation.peek() != generation {
            return;
        }

        match result {
            Ok(progress) => goals.set(progress),
            Err(error) => error_message.set(Some(error)),
        }

        if *request_generation.peek() != generation {
            return;
        }
        loading.set(false);
    });
}

#[component]
pub fn AboutDonate() -> Element {
    let toast = consume_toast();
    let mut selected_amount = use_signal(|| 21u64);
    let mut show_modal = use_signal(|| false);
    let loading = use_signal(|| true);
    let goals = use_signal(Vec::<zap_goals_store::ZapGoalProgress>::new);
    let error_message = use_signal(|| None::<String>);
    let request_generation = use_signal(|| 0u32);
    let mut selected_goal_event_id = use_signal(|| None::<String>);
    let mut selected_goal_relays = use_signal(|| None::<Vec<String>>);
    let qr_svg = donation_qr_svg().ok();

    use_effect(move || {
        let initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
        if !initialized {
            return;
        }
        load_project_goals(goals, loading, error_message, &request_generation);
    });

    let copy_address = move |_| {
        spawn(async move {
            match crate::platform::clipboard::copy_to_clipboard(PROJECT_DONATION_LUD16).await {
                Ok(_) => toast.success("Copied Lightning address".to_string(), ToastOptions::new()),
                Err(error) => toast.error(error, ToastOptions::new()),
            }
        });
    };

    rsx! {
        div { class: "mx-auto max-w-5xl px-4 py-8",
            div { class: "rounded-3xl border border-sky-300/50 bg-linear-to-br from-sky-500/12 via-background to-cyan-500/10 p-6 shadow-sm",
                div { class: "inline-flex rounded-full bg-sky-500/15 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-sky-700 dark:text-sky-300",
                    "Support nostr.blue"
                }
                h1 { class: "mt-4 text-4xl font-bold tracking-tight text-foreground",
                    "Fund the next stretch of development with zaps."
                }
                p { class: "mt-4 max-w-2xl text-base leading-7 text-muted-foreground",
                    "Every contribution goes straight to the project via Lightning. Support ongoing development, relay costs, and new features with a direct zap."
                }

                div { class: "mt-6 rounded-2xl border border-border bg-background/70 p-4",
                    p { class: "text-xs font-medium uppercase tracking-wide text-muted-foreground",
                        "Lightning address"
                    }
                    p { class: "mt-2 break-all font-mono text-lg font-semibold text-foreground",
                        "{PROJECT_DONATION_LUD16}"
                    }
                    if let Some(svg) = qr_svg.clone() {
                        div {
                            class: "mt-5 inline-block overflow-hidden rounded-2xl border border-border bg-white p-4",
                            dangerous_inner_html: "{svg}",
                        }
                        p { class: "mt-3 text-xs text-muted-foreground",
                            "QR payload: lightning:{PROJECT_DONATION_LUD16}"
                        }
                    }
                    div { class: "mt-4 flex flex-wrap gap-2",
                        button {
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                            onclick: copy_address,
                            "Copy address"
                        }
                        a {
                            href: "lightning:{PROJECT_DONATION_LUD16}",
                            class: "rounded-lg border border-border px-4 py-2 text-sm font-medium transition hover:bg-accent",
                            "Open in wallet"
                        }
                    }
                }

                div { class: "mt-6 flex flex-wrap gap-2",
                    for amount in [21u64, 100, 500, 1000, 5000, 10000] {
                        button {
                            key: "preset-{amount}",
                            class: "rounded-full border border-border bg-background px-4 py-2 text-sm font-medium transition hover:bg-accent",
                            onclick: move |_| {
                                selected_amount.set(amount);
                                selected_goal_event_id.set(None);
                                selected_goal_relays.set(None);
                                show_modal.set(true);
                            },
                            "{amount} sats"
                        }
                    }
                }
            }

            div { class: "mt-10 flex items-center justify-between gap-3",
                div {
                    h2 { class: "text-2xl font-bold text-foreground", "Active project goals" }
                    p { class: "mt-1 text-sm text-muted-foreground",
                        "Published funding goals are tracked directly on Nostr using NIP-75."
                    }
                }
            }

            if let Some(error) = error_message.read().as_ref() {
                div { class: "mt-4 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive",
                    "{error}"
                }
            }

            if *loading.read() {
                div { class: "mt-4 space-y-4",
                    for idx in 0..2 {
                        div { key: "donate-goal-skeleton-{idx}", class: "rounded-2xl border border-border bg-card p-4 animate-pulse",
                            div { class: "mb-4 h-6 w-40 rounded bg-muted" }
                            div { class: "mb-2 h-4 w-full rounded bg-muted" }
                            div { class: "h-3 w-full rounded bg-muted" }
                        }
                    }
                }
            } else if *crate::stores::nostr_client::CLIENT_INITIALIZED.read()
                && error_message.read().is_none()
                && goals.read().is_empty()
            {
                div { class: "mt-4 rounded-2xl border border-dashed border-border bg-card px-6 py-10 text-center",
                    p { class: "text-sm text-muted-foreground",
                        "No project goals are published yet. You can still support development with a direct zap."
                    }
                }
            } else {
                div { class: "mt-4 space-y-4",
                    for progress in goals.read().iter() {
                        {
                            let goal = progress.clone();
                            rsx! {
                                ZapGoalCard {
                                    key: "{goal.goal.event_id}",
                                    progress: goal.clone(),
                                    compact: true,
                                    on_contribute: move |_| {
                                        selected_amount.set(21);
                                        selected_goal_event_id.set(Some(goal.goal.event_id.clone()));
                                        selected_goal_relays.set(Some(goal.goal.relays.clone()));
                                        show_modal.set(true);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            if *show_modal.read() {
                ZapModal {
                    recipient_pubkey: PROJECT_DONATION_NPUB.to_string(),
                    recipient_name: "nostr.blue".to_string(),
                    lud16: Some(PROJECT_DONATION_LUD16.to_string()),
                    lud06: None,
                    event_id: selected_goal_event_id.read().clone(),
                    initial_amount: Some(*selected_amount.read()),
                    relay_hints: selected_goal_relays.read().clone(),
                    on_close: move |_| {
                        show_modal.set(false);
                        selected_goal_event_id.set(None);
                        selected_goal_relays.set(None);
                        if *crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                            load_project_goals(goals, loading, error_message, &request_generation);
                        }
                    },
                }
            }
        }
    }
}
