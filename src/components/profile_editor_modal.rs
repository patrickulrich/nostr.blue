use dioxus::prelude::*;
use crate::stores::{nostr_client, profiles, auth_store};
use crate::components::MediaUploader;
use nostr_sdk::Metadata;

#[derive(Props, Clone, PartialEq)]
pub struct ProfileEditorModalProps {
    /// Signal to control modal visibility
    pub show: Signal<bool>,
}

#[component]
pub fn ProfileEditorModal(mut props: ProfileEditorModalProps) -> Element {
    // Form fields
    let mut name = use_signal(|| String::new());
    let mut display_name = use_signal(|| String::new());
    let mut about = use_signal(|| String::new());
    let mut picture = use_signal(|| String::new());
    let mut banner = use_signal(|| String::new());
    let mut website = use_signal(|| String::new());
    let mut nip05 = use_signal(|| String::new());
    let mut lud16 = use_signal(|| String::new());

    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut show_picture_uploader = use_signal(|| false);
    let mut show_banner_uploader = use_signal(|| false);

    // Load current profile when modal opens
    use_effect(use_reactive(&*props.show.read(), move |is_shown| {
        if is_shown {
            spawn(async move {
                if let Some(pubkey) = auth_store::get_pubkey() {
                    // Fetch profile from cache or relays
                    match profiles::fetch_profile(pubkey.clone()).await {
                        Ok(profile) => {
                            name.set(profile.name.unwrap_or_default());
                            display_name.set(profile.display_name.unwrap_or_default());
                            about.set(profile.about.unwrap_or_default());
                            picture.set(profile.picture.unwrap_or_default());
                            banner.set(profile.banner.unwrap_or_default());
                            website.set(profile.website.unwrap_or_default());
                            nip05.set(profile.nip05.unwrap_or_default());
                            lud16.set(profile.lud16.unwrap_or_default());
                        }
                        Err(e) => {
                            log::error!("Failed to load profile for editing: {}", e);
                        }
                    }
                }
            });
        }
    }));

    // Save profile
    let handle_save = move |_| {
        saving.set(true);
        error.set(None);
        success.set(false);

        spawn(async move {
            let mut metadata = Metadata::new()
                .name(name.read().clone())
                .display_name(display_name.read().clone())
                .about(about.read().clone())
                .nip05(nip05.read().clone())
                .lud16(lud16.read().clone());

            // Only add URLs if they're valid
            if let Ok(url) = nostr_sdk::Url::parse(&picture.read().clone()) {
                metadata = metadata.picture(url);
            }
            if let Ok(url) = nostr_sdk::Url::parse(&banner.read().clone()) {
                metadata = metadata.banner(url);
            }
            if let Ok(url) = nostr_sdk::Url::parse(&website.read().clone()) {
                metadata = metadata.website(url);
            }

            match nostr_client::publish_metadata(metadata).await {
                Ok(_) => {
                    log::info!("Profile updated successfully");
                    success.set(true);

                    // Close modal after a short delay
                    spawn(async move {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        props.show.set(false);
                        success.set(false);
                    });
                }
                Err(e) => {
                    log::error!("Failed to update profile: {}", e);
                    error.set(Some(e));
                }
            }

            saving.set(false);
        });
    };

    // Picture upload handler
    let handle_picture_uploaded = move |url: String| {
        picture.set(url);
        show_picture_uploader.set(false);
    };

    // Banner upload handler
    let handle_banner_uploaded = move |url: String| {
        banner.set(url);
        show_banner_uploader.set(false);
    };

    // Close modal
    let close_modal = move |_| {
        props.show.set(false);
        error.set(None);
        success.set(false);
        show_picture_uploader.set(false);
        show_banner_uploader.set(false);
    };

    if !*props.show.read() {
        return rsx! { div {} };
    }

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 z-50 flex items-center justify-center p-4",
            onclick: close_modal,

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-xl shadow-2xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(), // Prevent close when clicking inside modal

                // Modal header
                div {
                    class: "sticky top-0 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 p-6 flex items-center justify-between z-10",
                    h2 {
                        class: "text-2xl font-bold text-gray-900 dark:text-white",
                        "✏️ Edit Profile"
                    }
                    button {
                        class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 text-2xl",
                        onclick: close_modal,
                        "✕"
                    }
                }

                // Modal body
                div {
                    class: "p-6 space-y-6",

                    // Profile Picture
                    div {
                        class: "space-y-3",
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Profile Picture"
                        }

                        if !picture.read().is_empty() {
                            div {
                                class: "flex items-center gap-4",
                                img {
                                    class: "w-24 h-24 rounded-full object-cover",
                                    src: "{picture}",
                                    alt: "Profile picture"
                                }
                                button {
                                    class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400",
                                    onclick: move |_| {
                                        picture.set(String::new());
                                        show_picture_uploader.set(true);
                                    },
                                    "Remove"
                                }
                            }
                        }

                        if *show_picture_uploader.read() || picture.read().is_empty() {
                            MediaUploader {
                                on_upload: handle_picture_uploaded,
                                button_label: "Upload Profile Picture"
                            }
                        } else {
                            button {
                                class: "px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg text-sm hover:bg-gray-200 dark:hover:bg-gray-600 transition",
                                onclick: move |_| show_picture_uploader.set(true),
                                "Change Picture"
                            }
                        }
                    }

                    // Banner
                    div {
                        class: "space-y-3",
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Banner Image"
                        }

                        if !banner.read().is_empty() {
                            div {
                                class: "space-y-2",
                                img {
                                    class: "w-full h-32 rounded-lg object-cover",
                                    src: "{banner}",
                                    alt: "Banner"
                                }
                                button {
                                    class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400",
                                    onclick: move |_| {
                                        banner.set(String::new());
                                        show_banner_uploader.set(true);
                                    },
                                    "Remove"
                                }
                            }
                        }

                        if *show_banner_uploader.read() || banner.read().is_empty() {
                            MediaUploader {
                                on_upload: handle_banner_uploaded,
                                button_label: "Upload Banner"
                            }
                        } else {
                            button {
                                class: "px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg text-sm hover:bg-gray-200 dark:hover:bg-gray-600 transition",
                                onclick: move |_| show_banner_uploader.set(true),
                                "Change Banner"
                            }
                        }
                    }

                    // Name
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Name"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "Your name",
                            value: "{name}",
                            oninput: move |evt| name.set(evt.value())
                        }
                    }

                    // Display Name
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Display Name"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "Display name",
                            value: "{display_name}",
                            oninput: move |evt| display_name.set(evt.value())
                        }
                    }

                    // About
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "About"
                        }
                        textarea {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white resize-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            rows: "4",
                            placeholder: "Tell us about yourself...",
                            value: "{about}",
                            oninput: move |evt| about.set(evt.value())
                        }
                    }

                    // Website
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Website"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "url",
                            placeholder: "https://example.com",
                            value: "{website}",
                            oninput: move |evt| website.set(evt.value())
                        }
                    }

                    // NIP-05
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "NIP-05 Identifier"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "user@domain.com",
                            value: "{nip05}",
                            oninput: move |evt| nip05.set(evt.value())
                        }
                    }

                    // Lightning Address
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Lightning Address"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "user@getalby.com",
                            value: "{lud16}",
                            oninput: move |evt| lud16.set(evt.value())
                        }
                    }

                    // Error message
                    if let Some(err) = error.read().as_ref() {
                        div {
                            class: "p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }

                    // Success message
                    if *success.read() {
                        div {
                            class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg",
                            "✅ Profile updated successfully!"
                        }
                    }
                }

                // Modal footer
                div {
                    class: "sticky bottom-0 bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 p-6 flex gap-3 justify-end",
                    button {
                        class: "px-6 py-2 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium hover:bg-gray-50 dark:hover:bg-gray-700 transition",
                        onclick: close_modal,
                        disabled: *saving.read(),
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-3 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-lg font-medium transition",
                        disabled: *saving.read(),
                        onclick: handle_save,
                        if *saving.read() {
                            "Saving..."
                        } else {
                            "Save Profile"
                        }
                    }
                }
            }
        }
    }
}
