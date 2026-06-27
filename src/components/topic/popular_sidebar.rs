use crate::stores::profiles::get_profile;
use crate::stores::social::topic_store::ScoredPost;
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Props, Clone, PartialEq)]
pub struct PopularSidebarProps {
    pub posts: Vec<ScoredPost>,
}

#[component]
pub fn PopularSidebar(props: PopularSidebarProps) -> Element {
    let top_users = compute_top_users(&props.posts);
    let popular_topics = compute_popular_topics(&props.posts);

    rsx! {
        div {
            class: "space-y-4",
            if !top_users.is_empty() {
                div {
                    class: "bg-card border border-border rounded-lg p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-3", "Top Posters" }
                    div {
                        class: "space-y-2.5",
                        for user in top_users.iter().take(5) {
                            {
                                let profile = get_profile(&user.pubkey);
                                let name = profile
                                    .as_ref()
                                    .and_then(|p| p.display_name.clone())
                                    .or_else(|| profile.and_then(|p| p.name.clone()))
                                    .unwrap_or_else(|| {
                                        let pk = &user.pubkey;
                                        if pk.len() >= 8 {
                                            format!("{}..{}", &pk[..4], &pk[pk.len()-4..])
                                        } else {
                                            pk.clone()
                                        }
                                    });
                                let pk_hex = user.pubkey.clone();
                                let post_count = user.post_count;
                                let total_score = user.total_score as i32;
                                rsx! {
                                    a {
                                        href: "/profile/{pk_hex}",
                                        class: "flex items-center justify-between gap-2 py-1.5 px-2 rounded-md hover:bg-accent/50 transition no-underline",
                                        div {
                                            class: "flex items-center gap-2 min-w-0",
                                            span { class: "text-sm font-medium text-foreground truncate", "{name}" }
                                        }
                                        div {
                                            class: "flex items-center gap-2 shrink-0",
                                            span {
                                                class: "text-xs text-muted-foreground",
                                                "{post_count} posts"
                                            }
                                            span {
                                                class: "text-xs font-medium text-primary",
                                                "+{total_score}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !popular_topics.is_empty() {
                div {
                    class: "bg-card border border-border rounded-lg p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-3", "Popular Topics" }
                    div {
                        class: "flex flex-wrap gap-2",
                        for topic in popular_topics.iter().take(10) {
                            a {
                                href: "/topics/t/{topic.name}",
                                class: "inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-full bg-muted hover:bg-accent/50 transition text-sm no-underline",
                                span { class: "font-medium text-foreground", "#{topic.name}" }
                                span { class: "text-xs text-muted-foreground", "{topic.post_count}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct UserStats {
    pubkey: String,
    post_count: usize,
    total_score: f64,
}

struct TopicStats {
    name: String,
    post_count: usize,
}

fn compute_top_users(posts: &[ScoredPost]) -> Vec<UserStats> {
    let mut map: HashMap<String, (usize, f64)> = HashMap::new();
    for sp in posts {
        let entry = map.entry(sp.post.pubkey.clone()).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += sp.score;
    }
    let mut users: Vec<UserStats> = map
        .into_iter()
        .map(|(pubkey, (post_count, total_score))| UserStats {
            pubkey,
            post_count,
            total_score,
        })
        .collect();
    users.sort_by(|a, b| {
        b.total_score
            .partial_cmp(&a.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    users
}

fn compute_popular_topics(posts: &[ScoredPost]) -> Vec<TopicStats> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for sp in posts {
        *map.entry(sp.post.topic.clone()).or_insert(0) += 1;
    }
    let mut topics: Vec<TopicStats> = map
        .into_iter()
        .map(|(name, post_count)| TopicStats { name, post_count })
        .collect();
    topics.sort_by_key(|b| std::cmp::Reverse(b.post_count));
    topics
}
