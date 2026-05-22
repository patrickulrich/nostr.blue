use crate::stores::profiles;
use crate::stores::social::group_store::{
    add_user_to_group, fetch_join_requests, ignore_join_request, JoinRequest,
};
use crate::utils::time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[component]
pub fn GroupJoinRequests(relay_url: String, group_id: String) -> Element {
    let mut requests = use_signal(Vec::<JoinRequest>::new);
    let mut loading = use_signal(|| true);

    {
        let relay = relay_url.clone();
        let gid = group_id.clone();
        use_effect(move || {
            let relay = relay.clone();
            let gid = gid.clone();
            spawn(async move {
                let cached = crate::stores::social::group_store::get_cached_join_requests(
                    &relay, &gid,
                );
                if !cached.is_empty() {
                    requests.set(cached);
                    loading.set(false);
                }
                if let Ok(fetched) = fetch_join_requests(&relay, &gid).await {
                    requests.set(fetched);
                }
                loading.set(false);
            });
        });
    }

    rsx! {
        div { class: "space-y-3",
            if *loading.read() {
                div { class: "text-center py-4 text-muted-foreground text-sm", "Loading requests..." }
            } else if requests().is_empty() {
                div { class: "text-center py-4 text-muted-foreground text-sm", "No pending requests" }
            } else {
                for req in requests() {
                    {
                        let relay = relay_url.clone();
                        let gid = group_id.clone();
                        rsx! {
                            JoinRequestItem {
                                key: "{req.id}",
                                request: req,
                                relay_url: relay,
                                group_id: gid,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn JoinRequestItem(
    request: JoinRequest,
    relay_url: String,
    group_id: String,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let pk = request.author.clone();

    {
        let pk = pk.clone();
        use_effect(move || {
            let pk = pk.clone();
            spawn(async move {
                if let Ok(p) = profiles::fetch_profile(pk).await {
                    profile.set(Some(p));
                }
            });
        });
    }

    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| truncate_pubkey(&request.author));
    let initial = display_name.chars().next().unwrap_or('?');
    let ts = time::format_time_ago(request.created_at);

    rsx! {
        div { class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg",
            div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-sm font-semibold text-muted-foreground overflow-hidden shrink-0",
                if let Some(url) = profile.read().as_ref().and_then(|p| p.picture.clone()).filter(|u| !u.is_empty()) {
                    img { class: "w-full h-full object-cover", src: "{url}", loading: "lazy" }
                } else {
                    "{initial}"
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-sm font-medium text-foreground truncate", "{display_name}" }
                if !request.content.is_empty() {
                    div { class: "text-xs text-muted-foreground truncate", "{request.content}" }
                }
                if let Some(code) = &request.invite_code {
                    div { class: "text-xs text-blue-500", "Invite code: {code}" }
                }
                div { class: "text-xs text-muted-foreground", "{ts}" }
            }
            div { class: "flex gap-2 shrink-0",
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    let author = request.author.clone();
                    let req_id = request.id.clone();
                    rsx! {
                        button {
                            class: "px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-xs hover:bg-primary/90 transition",
                            onclick: move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let author = author.clone();
                                let req_id = req_id.clone();
                                spawn(async move {
                                    if add_user_to_group(&relay, &gid, &author, vec![]).await.is_ok() {
                                        ignore_join_request(&relay, &gid, &req_id);
                                    }
                                });
                            },
                            "Accept"
                        }
                    }
                }
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    let req_id = request.id.clone();
                    rsx! {
                        button {
                            class: "px-3 py-1.5 bg-accent text-foreground rounded-lg text-xs hover:bg-accent/80 transition",
                            onclick: move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let req_id = req_id.clone();
                                spawn(async move {
                                    ignore_join_request(&relay, &gid, &req_id);
                                });
                            },
                            "Ignore"
                        }
                    }
                }
            }
        }
    }
}
