use crate::services::ppq::{self, IMPORT_KEY_NAME};
use crate::stores::ai_provider_store::PpqAccountState;
use dioxus::prelude::*;

#[component]
pub fn PpqImportForm(
    on_imported: EventHandler<PpqAccountState>,
    replace_mode: bool,
) -> Element {
    let mut credit_id_input = use_signal(String::new);
    let mut api_key_input = use_signal(String::new);
    let mut importing = use_signal(|| false);
    let mut import_error = use_signal(|| None::<String>);
    let mut confirmed_replace = use_signal(|| false);

    let credit_id_entered = !credit_id_input.read().trim().is_empty();
    let can_import = !*importing.read() && credit_id_entered;

    rsx! {
        div { class: "rounded-xl border border-border bg-background p-5 space-y-4 text-left",
            h4 { class: "font-medium text-foreground", "Import Existing Credit ID" }
            p { class: "text-sm text-muted-foreground",
                "Already have a PPQ account from ppq.ai? Import its Credit ID to use its balance here. A dedicated \"{IMPORT_KEY_NAME}\" API key is reused or created automatically for chat."
            }

            if replace_mode && !*confirmed_replace.read() {
                div { class: "rounded-lg border border-amber-500/40 bg-amber-500/10 p-4 space-y-3",
                    p { class: "text-sm text-foreground",
                        "Importing a different Credit ID replaces the PPQ account stored on this device. The previous account's keys remain on PPQ — only the local nostr.blue state changes."
                    }
                    button {
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                        onclick: move |_| confirmed_replace.set(true),
                        "Continue"
                    }
                }
            } else {
                div { class: "space-y-4",
                    label { class: "block space-y-2",
                        span { class: "text-sm font-medium text-foreground", "Credit ID" }
                        input {
                            class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm font-mono",
                            r#type: "text",
                            placeholder: "4af59b9d-f6ec-4531-82f7-ce776d49e207",
                            autocomplete: "off",
                            spellcheck: "false",
                            value: "{credit_id_input}",
                            disabled: *importing.read(),
                            oninput: move |evt| credit_id_input.set(evt.value()),
                        }
                    }
                    label { class: "block space-y-2",
                        span { class: "text-sm font-medium text-foreground", "Existing API key (optional)" }
                        input {
                            class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                            r#type: "password",
                            placeholder: "sk-... — leave empty to auto-create a key",
                            autocomplete: "off",
                            value: "{api_key_input}",
                            disabled: *importing.read(),
                            oninput: move |evt| api_key_input.set(evt.value()),
                        }
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Your Credit ID grants full access to the PPQ account balance. It is stored locally and synced only through your encrypted Nostr preferences."
                    }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                            disabled: !can_import,
                            onclick: move |_| {
                                if *importing.read() {
                                    return;
                                }
                                importing.set(true);
                                import_error.set(None);
                                let credit_id = credit_id_input.read().trim().to_string();
                                let pasted_key = {
                                    let trimmed = api_key_input.read().trim().to_string();
                                    (!trimmed.is_empty()).then_some(trimmed)
                                };
                                spawn(async move {
                                    match import_account(&credit_id, pasted_key).await {
                                        Ok(account_state) => {
                                            credit_id_input.set(String::new());
                                            api_key_input.set(String::new());
                                            confirmed_replace.set(false);
                                            importing.set(false);
                                            on_imported.call(account_state);
                                        }
                                        Err(err) => {
                                            import_error.set(Some(err));
                                            importing.set(false);
                                        }
                                    }
                                });
                            },
                            if *importing.read() {
                                "Importing..."
                            } else if replace_mode {
                                "Import / Replace Credit ID"
                            } else {
                                "Import Credit ID"
                            }
                        }
                    }
                    if let Some(err) = import_error.read().as_ref() {
                        p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                    }
                }
            }
        }
    }
}

async fn import_account(
    credit_id: &str,
    pasted_key: Option<String>,
) -> Result<PpqAccountState, String> {
    if let Some(api_key) = pasted_key {
        let credit_id = ppq::validate_credit_id(credit_id, Some(&api_key)).await?;
        return Ok(PpqAccountState {
            credit_id,
            api_key: String::new(),
            managed_api_key: Some(api_key),
            active_api_key_id: None,
        });
    }
    let imported = ppq::import_credit_id(credit_id).await?;
    let api_key = imported
        .api_key
        .ok_or_else(|| "PPQ did not return the created API key".to_string())?;
    Ok(PpqAccountState {
        credit_id: imported.credit_id,
        api_key: String::new(),
        managed_api_key: Some(api_key),
        active_api_key_id: imported.key_id,
    })
}
