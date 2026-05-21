use crate::components::{
    ClientInitializing, RecipeDetailView, RecipeDetailViewSkeleton, ShareModal, ZapModal,
};
use crate::hooks::{use_nostr_resource, NostrResourceState};
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::recipe_store;
use dioxus::prelude::*;

#[component]
pub fn RecipeDetail(naddr: String) -> Element {
    let naddr_clone = naddr.clone();
    let recipe = use_nostr_resource(move || {
        let naddr_str = naddr_clone.clone();
        async move {
            match recipe_store::fetch_recipe_by_naddr(&naddr_str).await {
                Ok(Some(r)) => Ok(r),
                Ok(None) => Err("Recipe not found".to_string()),
                Err(e) => Err(e),
            }
        }
    });
    let recipe_state = recipe.state();
    let is_owner = use_memo(move || {
        if let NostrResourceState::Loaded(ref r) = &*recipe_state.read() {
            if let Some(pubkey) = auth_store::get_pubkey() {
                return r.event.pubkey.to_hex() == pubkey;
            }
        }
        false
    });
    let mut show_zap_modal = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    Link {
                        to: Route::RecipesHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                        "Recipes"
                    }
                    if let NostrResourceState::Loaded(_) = &*recipe_state.read() {
                        div { class: "flex items-center gap-2",
                            button {
                                class: "p-2 rounded-lg hover:bg-muted transition",
                                onclick: move |_| show_share_modal.set(true),
                                title: "Share",
                                svg {
                                    class: "w-5 h-5",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.367 2.684 3 3 0 00-5.367-2.684z",
                                    }
                                }
                            }
                            if *HAS_SIGNER.read() {
                                button {
                                    class: "p-2 rounded-lg hover:bg-muted transition text-amber-500",
                                    onclick: move |_| show_zap_modal.set(true),
                                    title: "Zap",
                                    svg {
                                        class: "w-5 h-5",
                                        fill: "currentColor",
                                        view_box: "0 0 24 24",
                                        path { d: "M13 10V3L4 14h7v7l9-11h-7z" }
                                    }
                                }
                            }
                            if *HAS_SIGNER.read() && !*is_owner.read() {
                                Link {
                                    to: Route::RecipeFork {
                                        naddr: naddr.clone(),
                                    },
                                    class: "px-3 py-1.5 rounded-lg hover:bg-muted transition text-sm font-medium flex items-center gap-1",
                                    svg {
                                        class: "w-4 h-4",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M8 7v8a2 2 0 002 2h6M8 7V5a2 2 0 012-2h4.586a1 1 0 01.707.293l4.414 4.414a1 1 0 01.293.707V15a2 2 0 01-2 2h-2M8 7H6a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2v-2",
                                        }
                                    }
                                    "Fork"
                                }
                            }
                            if *is_owner.read() {
                                Link {
                                    to: Route::RecipeFork {
                                        naddr: naddr.clone(),
                                    },
                                    class: "px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:bg-primary/90 transition",
                                    "Edit"
                                }
                            }
                        }
                    }
                }
            }
            match &*recipe_state.read() {
                NostrResourceState::Initializing => rsx! {
                    div { class: "p-4",
                        ClientInitializing {}
                    }
                },
                NostrResourceState::Loading => rsx! {
                    div { class: "p-4",
                        RecipeDetailViewSkeleton {}
                    }
                },
                NostrResourceState::AuthRequired => rsx! {
                    div { class: "p-4",
                        ClientInitializing {}
                    }
                },
                NostrResourceState::Error(e) => rsx! {
                    div { class: "p-4",
                        div { class: "flex flex-col items-center justify-center py-12 text-center",
                            span { class: "text-5xl mb-4", "🍽️" }
                            h2 { class: "text-xl font-semibold mb-2", "Recipe Not Found" }
                            p { class: "text-muted-foreground mb-4", "{e}" }
                            Link {
                                to: Route::RecipesHome {},
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                                "Browse Recipes"
                            }
                        }
                    }
                },
                NostrResourceState::Loaded(r) => rsx! {
                    div { class: "p-4",
                        RecipeDetailView { recipe: r.clone() }
                    }
                    if *show_zap_modal.read() {
                        ZapModal {
                            recipient_pubkey: r.event.pubkey.to_hex(),
                            recipient_name: r.metadata.title.clone(),
                            event_id: Some(r.event.id.to_hex()),
                            on_close: move |_| show_zap_modal.set(false),
                        }
                    }
                    if *show_share_modal.read() {
                        ShareModal {
                            event: r.event.clone(),
                            on_close: move |_| show_share_modal.set(false),
                        }
                    }
                },
            }
        }
    }
}
