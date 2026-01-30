//! NIP-49 Password Modal Component
//!
//! Modal for entering password to decrypt encrypted private keys or
//! to set a password for migrating unencrypted keys.

use crate::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use crate::stores::auth_store::{cancel_password_prompt, restore_with_password, PASSWORD_PROMPT};
use dioxus::prelude::*;

/// Password entry modal for NIP-49 encrypted key decryption
#[component]
pub fn PasswordModal() -> Element {
    let prompt_state = PASSWORD_PROMPT.read();

    // Don't render if not required
    if !prompt_state.required {
        return rsx! {};
    }

    let mut password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut show_password = use_signal(|| false);

    // Determine if this is initial setup (migration) or unlock
    let is_migration = prompt_state.ncryptsec.is_none();

    let passwords_match = password.read().as_str() == confirm_password.read().as_str();
    let password_valid = !password.read().is_empty() && password.read().len() >= 8;
    let can_submit = if is_migration {
        password_valid && passwords_match && !prompt_state.loading
    } else {
        !password.read().is_empty() && !prompt_state.loading
    };

    // Centralized submit logic for both button and Enter key
    let do_submit = move || {
        let pwd = password.read().clone();

        // For migration, validate password confirmation
        if is_migration {
            let confirm = confirm_password.read().clone();
            if pwd != confirm {
                PASSWORD_PROMPT.write().error = Some("Passwords do not match".to_string());
                return;
            }
            if let Some(err) = crate::utils::nip49::validate_password(&pwd) {
                PASSWORD_PROMPT.write().error = Some(err);
                return;
            }
        }

        spawn(async move {
            let _ = restore_with_password(&pwd).await;
        });
    };

    let handle_submit = move |_| {
        do_submit();
    };

    let handle_cancel = move |_| {
        cancel_password_prompt();
    };

    let handle_key_press = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter && can_submit {
            do_submit();
        }
    };

    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    cancel_password_prompt();
                }
            },

            DialogContent {
                class: "max-w-md",

                // Close button
                button {
                    class: "absolute top-4 right-4 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: handle_cancel,
                    "aria-label": "Close",
                    disabled: prompt_state.loading,
                    crate::components::icons::XIcon { class: "w-5 h-5" }
                }

                // Title with icon
                DialogTitle {
                    class: "flex items-center gap-2 text-lg font-bold",
                    span { class: "text-2xl", if is_migration { "🔐" } else { "🔓" } }
                    if is_migration {
                        "Secure Your Private Key"
                    } else {
                        "Unlock Your Account"
                    }
                }

                // Description
                DialogDescription {
                    class: "text-muted-foreground text-sm mt-2",
                    if is_migration {
                        "Your private key will be encrypted with this password. You'll need it to unlock your account on this device."
                    } else {
                        "Enter your password to decrypt your private key and access your account."
                    }
                }

                // Error display
                if let Some(err) = prompt_state.error.as_ref() {
                    div {
                        class: "mt-4 p-3 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded-lg",
                        p {
                            class: "text-sm text-red-800 dark:text-red-200",
                            "{err}"
                        }
                    }
                }

                // Password input
                div {
                    class: "mt-6 space-y-4",

                    div {
                        label {
                            class: "block text-sm font-medium mb-1",
                            "Password"
                        }
                        div {
                            class: "relative",
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background pr-12 focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: if *show_password.read() { "text" } else { "password" },
                                placeholder: "Enter password",
                                value: "{password}",
                                oninput: move |evt| password.set(evt.value()),
                                onkeypress: handle_key_press,
                                disabled: prompt_state.loading,
                                autofocus: true
                            }
                            button {
                                class: "absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors text-sm",
                                r#type: "button",
                                onclick: move |_| {
                                    let current = *show_password.read();
                                    show_password.set(!current);
                                },
                                if *show_password.read() { "🙈" } else { "👁️" }
                            }
                        }
                    }

                    // Confirm password (only for migration)
                    if is_migration {
                        div {
                            label {
                                class: "block text-sm font-medium mb-1",
                                "Confirm Password"
                            }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: if *show_password.read() { "text" } else { "password" },
                                placeholder: "Confirm password",
                                value: "{confirm_password}",
                                oninput: move |evt| confirm_password.set(evt.value()),
                                onkeypress: handle_key_press,
                                disabled: prompt_state.loading
                            }
                        }

                        // Password requirements
                        div {
                            class: "text-xs space-y-1",
                            div {
                                class: if password.read().len() >= 8 {
                                    "text-green-600 dark:text-green-400"
                                } else {
                                    "text-muted-foreground"
                                },
                                "• At least 8 characters"
                            }
                            div {
                                class: if passwords_match && !password.read().is_empty() {
                                    "text-green-600 dark:text-green-400"
                                } else {
                                    "text-muted-foreground"
                                },
                                "• Passwords match"
                            }
                        }

                        p {
                            class: "text-xs text-amber-600 dark:text-amber-400 mt-2",
                            "Remember this password! Without it, you cannot access your account on this device."
                        }
                    }
                }

                // Buttons
                div {
                    class: "flex gap-3 mt-6",

                    // Cancel button
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg border border-border hover:bg-accent transition-colors disabled:opacity-50",
                        onclick: handle_cancel,
                        disabled: prompt_state.loading,
                        "Cancel"
                    }

                    // Submit button
                    button {
                        class: "flex-1 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        onclick: handle_submit,
                        disabled: !can_submit,

                        if prompt_state.loading {
                            span {
                                class: "inline-flex items-center gap-2",
                                span {
                                    class: "w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"
                                }
                                if is_migration { "Encrypting..." } else { "Unlocking..." }
                            }
                        } else if is_migration {
                            "Set Password"
                        } else {
                            "Unlock"
                        }
                    }
                }
            }
        }
    }
}
