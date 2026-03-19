use crate::stores::profiles;
use crate::stores::zap_goals_store::{self, ZapGoalProgress};
use dioxus::prelude::*;
use url::Url;

#[derive(Props, Clone, PartialEq)]
pub struct ZapGoalCardProps {
    pub progress: ZapGoalProgress,
    #[props(default = false)]
    pub compact: bool,
    pub on_contribute: EventHandler<()>,
}

#[component]
pub fn ZapGoalCard(props: ZapGoalCardProps) -> Element {
    let profile = profiles::get_cached_profile(&props.progress.goal.author_pubkey);
    let display_name = profile
        .as_ref()
        .map(|profile| profile.get_display_name())
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&props.progress.goal.author_pubkey));
    let avatar_url = profile
        .as_ref()
        .map(|profile| profile.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                props.progress.goal.author_pubkey
            )
        });
    let card_class = if props.progress.goal.is_project_goal {
        "rounded-2xl border border-sky-300/60 bg-sky-500/8 p-4 shadow-sm"
    } else {
        "rounded-2xl border border-border bg-card p-4 shadow-sm"
    };
    let summary = props
        .progress
        .goal
        .summary
        .clone()
        .unwrap_or_else(|| "Zap Goal".to_string());
    let content = props.progress.goal.content.trim().to_string();
    let preview = if props.compact {
        content.chars().take(160).collect::<String>()
    } else {
        content.chars().take(260).collect::<String>()
    };
    let percentage = props.progress.percentage.clamp(0.0, 100.0);
    let safe_goal_url = props.progress.goal.url.as_ref().and_then(|url| {
        let parsed = Url::parse(url).ok()?;
        matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
    });

    rsx! {
        article { class: "{card_class}",
            div { class: "flex items-start justify-between gap-4",
                div { class: "flex items-center gap-3 min-w-0",
                    img {
                        class: "h-11 w-11 rounded-full border border-border object-cover shrink-0",
                        src: "{avatar_url}",
                        alt: "{display_name}",
                    }
                    div { class: "min-w-0",
                        div { class: "flex items-center gap-2 flex-wrap",
                            h3 { class: "font-semibold text-foreground truncate", "{display_name}" }
                            if props.progress.goal.is_project_goal {
                                span { class: "rounded-full bg-sky-500/15 px-2 py-0.5 text-xs font-medium text-sky-700 dark:text-sky-300",
                                    "nostr.blue"
                                }
                            }
                        }
                        div { class: "text-xs text-muted-foreground flex flex-wrap items-center gap-2",
                            span { "{crate::utils::format::truncate_pubkey(&props.progress.goal.author_pubkey)}" }
                            span { "•" }
                            span { "Created {zap_goals_store::format_goal_date(props.progress.goal.created_at)}" }
                        }
                    }
                }
                button {
                    class: "shrink-0 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                    onclick: move |_| props.on_contribute.call(()),
                    "Contribute"
                }
            }

            div { class: "mt-4 space-y-3",
                div {
                    h4 { class: "text-lg font-semibold text-foreground", "{summary}" }
                    if !preview.is_empty() {
                        p { class: "mt-1 text-sm leading-6 text-muted-foreground whitespace-pre-wrap",
                            "{preview}"
                            if content.len() > preview.len() {
                                "…"
                            }
                        }
                    }
                }

                if let Some(image) = props.progress.goal.image.clone() {
                    img {
                        class: "max-h-72 w-full rounded-xl border border-border object-cover",
                        src: "{image}",
                        alt: "{summary}",
                    }
                }

                div { class: "rounded-xl border border-border/80 bg-background/60 p-3",
                    div { class: "mb-2 flex items-center justify-between gap-3 text-sm",
                        div { class: "font-medium text-foreground",
                            "{props.progress.raised_sats.to_string()} sats"
                            span { class: "text-muted-foreground", " raised" }
                        }
                        div { class: "text-muted-foreground",
                            "Target {props.progress.goal.amount_sats.to_string()} sats"
                        }
                    }
                    div { class: "h-2.5 overflow-hidden rounded-full bg-muted",
                        div {
                            class: "h-full rounded-full bg-linear-to-r from-sky-400 via-cyan-400 to-emerald-400 transition-all",
                            style: "width: {percentage}%;",
                        }
                    }
                    div { class: "mt-2 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground",
                        span { "{props.progress.percentage.round()}% funded" }
                        span { "{zap_goals_store::format_time_remaining(props.progress.goal.closed_at)}" }
                    }
                }

                div { class: "flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground",
                    span { "{props.progress.contributor_count} contributor(s)" }
                    if let Some(url) = safe_goal_url {
                        a {
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "text-primary hover:underline",
                            "Related link"
                        }
                    }
                }

                if !props.progress.recent_contributors.is_empty() {
                    div { class: "space-y-2",
                        p { class: "text-xs font-medium uppercase tracking-wide text-muted-foreground",
                            "Recent contributors"
                        }
                        div { class: "space-y-2",
                            for contributor in props.progress.recent_contributors.iter() {
                                {
                                    let contributor_profile = profiles::get_cached_profile(&contributor.pubkey);
                                    let contributor_name = contributor_profile
                                        .as_ref()
                                        .map(|profile| profile.get_display_name())
                                        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&contributor.pubkey));
                                    let contributor_avatar = contributor_profile
                                        .as_ref()
                                        .map(|profile| profile.get_avatar_url())
                                        .unwrap_or_else(|| {
                                            format!(
                                                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                                                contributor.pubkey
                                            )
                                        });
                                    rsx! {
                                        div {
                                            key: "{contributor.pubkey}",
                                            class: "flex items-center justify-between gap-3 rounded-xl bg-background/70 px-3 py-2",
                                            div { class: "flex min-w-0 items-center gap-3",
                                                img {
                                                    class: "h-8 w-8 rounded-full border border-border object-cover",
                                                    src: "{contributor_avatar}",
                                                    alt: "{contributor_name}",
                                                }
                                                div { class: "min-w-0",
                                                    p { class: "truncate text-sm font-medium text-foreground",
                                                        "{contributor_name}"
                                                    }
                                                    if let Some(comment) = contributor.comment.clone() {
                                                        p { class: "truncate text-xs text-muted-foreground",
                                                            "\"{comment}\""
                                                        }
                                                    }
                                                }
                                            }
                                            span { class: "shrink-0 text-sm font-medium text-foreground",
                                                "{contributor.amount_sats} sats"
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
