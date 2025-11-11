use dioxus::prelude::*;
use crate::stores::auth_store;
use crate::hooks::{use_user_lists, delete_list, UserList};
use crate::utils::{get_list_type_name, get_list_icon, get_item_count};

#[component]
pub fn Lists() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let (lists, loading, error, mut refresh_trigger) = use_user_lists();

    // Delete confirmation state
    let mut delete_confirm_open = use_signal(|| false);
    let mut list_to_delete = use_signal(|| None::<UserList>);
    let mut deleting = use_signal(|| false);

    // Handle delete confirmation
    let handle_delete = move |_| {
        if let Some(list) = list_to_delete.read().clone() {
            deleting.set(true);
            spawn(async move {
                match delete_list(&list.event).await {
                    Ok(_) => {
                        log::info!("List deleted successfully");
                        delete_confirm_open.set(false);
                        list_to_delete.set(None);

                        // Refresh the lists to show the deletion
                        refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));
                    }
                    Err(e) => {
                        log::error!("Failed to delete list: {}", e);
                        // Could show error toast here
                    }
                }
                deleting.set(false);
            });
        }
    };

    let mut confirm_delete = move |list: UserList| {
        list_to_delete.set(Some(list));
        delete_confirm_open.set(true);
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 pt-3 flex items-center justify-between",
                    h1 {
                        class: "text-xl font-bold px-4 py-3 flex items-center gap-2",
                        "📋 Lists"
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Not logged in
                if !auth.is_authenticated {
                    div {
                        class: "flex flex-col items-center justify-center py-20 px-4 text-center",
                        div {
                            class: "text-6xl mb-4",
                            "📋"
                        }
                        h2 {
                            class: "text-2xl font-bold mb-2",
                            "Organize your Nostr"
                        }
                        p {
                            class: "text-muted-foreground max-w-sm mb-6",
                            "Log in to create and manage custom lists of people, relays, and content."
                        }
                    }
                }
                // Loading state
                else if *loading.read() {
                    div {
                        class: "flex flex-col items-center justify-center py-20 px-4",
                        div {
                            class: "animate-spin text-4xl mb-4",
                            "⏳"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Loading your lists..."
                        }
                    }
                }
                // Error state
                else if let Some(err) = error.read().as_ref() {
                    div {
                        class: "flex flex-col items-center justify-center py-20 px-4 text-center",
                        div {
                            class: "text-6xl mb-4",
                            "⚠️"
                        }
                        h2 {
                            class: "text-2xl font-bold mb-2",
                            "Error loading lists"
                        }
                        p {
                            class: "text-muted-foreground max-w-sm mb-6",
                            "{err}"
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                            onclick: move |_| {
                                // Reload the page to retry
                                web_sys::window()
                                    .and_then(|w| w.location().reload().ok());
                            },
                            "Try Again"
                        }
                    }
                }
                // Empty state
                else if lists.read().is_empty() {
                    div {
                        class: "flex flex-col items-center justify-center py-20 px-4 text-center",
                        div {
                            class: "text-6xl mb-4",
                            "📋"
                        }
                        h2 {
                            class: "text-2xl font-bold mb-2",
                            "No lists yet"
                        }
                        p {
                            class: "text-muted-foreground max-w-sm mb-6",
                            "Create custom lists to organize people, relays, bookmarks, or curated content."
                        }
                        div {
                            class: "text-left space-y-4 max-w-md",

                            // People Lists
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/50 rounded-lg",
                                div {
                                    class: "text-2xl mt-1",
                                    "👥"
                                }
                                div {
                                    h3 {
                                        class: "font-semibold",
                                        "People Lists"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground",
                                        "Organize contacts into groups (friends, work, etc.)"
                                    }
                                }
                            }

                            // Relay Lists
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/50 rounded-lg",
                                div {
                                    class: "text-2xl mt-1",
                                    "🔗"
                                }
                                div {
                                    h3 {
                                        class: "font-semibold",
                                        "Relay Lists"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground",
                                        "Save and share your favorite relay configurations"
                                    }
                                }
                            }

                            // Bookmark Collections
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/50 rounded-lg",
                                div {
                                    class: "text-2xl mt-1",
                                    "🔖"
                                }
                                div {
                                    h3 {
                                        class: "font-semibold",
                                        "Bookmark Collections"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground",
                                        "Organize your saved posts into categories"
                                    }
                                }
                            }

                            // Content Curations
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/50 rounded-lg",
                                div {
                                    class: "text-2xl mt-1",
                                    "📚"
                                }
                                div {
                                    h3 {
                                        class: "font-semibold",
                                        "Content Curations"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground",
                                        "Curate and share collections of notes"
                                    }
                                }
                            }
                        }
                    }
                }
                // Lists grid
                else {
                    div {
                        class: "grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3",

                        for list in lists.read().iter() {
                            ListCard {
                                key: "{list.id}",
                                list: list.clone(),
                                on_delete: move |list: UserList| confirm_delete(list)
                            }
                        }
                    }
                }
            }

            // Delete Confirmation Dialog
            if *delete_confirm_open.read() {
                div {
                    class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
                    onclick: move |_| {
                        if !*deleting.read() {
                            delete_confirm_open.set(false);
                        }
                    },

                    div {
                        class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4",
                        onclick: move |e| e.stop_propagation(),

                        h2 {
                            class: "text-xl font-bold mb-2",
                            "Delete List?"
                        }
                        p {
                            class: "text-muted-foreground mb-6",
                            "Are you sure you want to delete \""
                            {list_to_delete.read().as_ref().map(|l| l.name.as_str()).unwrap_or("")}
                            "\"? This action cannot be undone."
                        }
                        div {
                            class: "flex gap-2 justify-end",
                            button {
                                class: "px-4 py-2 border border-border rounded-lg hover:bg-muted transition-colors",
                                disabled: *deleting.read(),
                                onclick: move |_| delete_confirm_open.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors disabled:opacity-50",
                                disabled: *deleting.read(),
                                onclick: handle_delete,
                                if *deleting.read() {
                                    span {
                                        class: "animate-spin mr-2",
                                        "⏳"
                                    }
                                }
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// List card component
#[component]
fn ListCard(list: UserList, on_delete: EventHandler<UserList>) -> Element {
    let navigator = use_navigator();
    let item_count = get_item_count(&list.tags);
    let list_clone = list.clone();

    rsx! {
        div {
            class: "border border-border rounded-lg hover:shadow-md transition-shadow bg-card",

            // Card header
            div {
                class: "p-4 border-b border-border",
                h3 {
                    class: "text-lg font-semibold flex items-center gap-2",
                    span {
                        class: "text-2xl",
                        "{get_list_icon(list.kind)}"
                    }
                    "{list.name}"
                }
                p {
                    class: "text-xs text-muted-foreground mt-1",
                    "{get_list_type_name(list.kind)} • {item_count} items"
                }
            }

            // Card content (description)
            if !list.description.is_empty() {
                div {
                    class: "p-4",
                    p {
                        class: "text-sm text-muted-foreground line-clamp-2",
                        "{list.description}"
                    }
                }
            }

            // Card footer (actions)
            div {
                class: "p-4 flex gap-2",
                button {
                    class: "flex-1 px-4 py-2 border border-border rounded-lg hover:bg-muted transition-colors",
                    onclick: move |_| {
                        // Navigate to list detail page
                        navigator.push(format!("/lists/{}", list.identifier));
                    },
                    "View"
                }
                button {
                    class: "px-4 py-2 text-red-600 hover:bg-red-50 dark:hover:bg-red-950 rounded-lg transition-colors",
                    onclick: move |_| on_delete.call(list_clone.clone()),
                    "🗑️"
                }
            }
        }
    }
}
