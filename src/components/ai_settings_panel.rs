use crate::components::ppq_settings_panel::PpqSettingsPanel;
use crate::stores::ai_provider_store::{
    self, normalize_base_url, ppq_provider, resolve_providers, sanitize_provider_input,
    AiProviderKind, AiProviderState, CustomAiProvider, ProviderAuth,
};
use dioxus::prelude::*;
use regex::Regex;
use std::net::IpAddr;
use std::sync::LazyLock;
use url::Url;

static LEADING_CREDENTIALS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^/@]+@").expect("valid credential-stripping regex"));

#[component]
pub fn AiSettingsPanel(#[props(default = false)] headerless: bool) -> Element {
    let mut state = use_signal(AiProviderState::default);
    let mut provider_state_load = use_signal(|| None::<Result<AiProviderState, String>>);
    let mut save_error = use_signal(|| None::<String>);
    let is_saving = use_signal(|| false);
    let mut editing_provider_id = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut provider_id = use_signal(String::new);
    let mut base_url = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let provider_state_ready = matches!(provider_state_load.read().as_ref(), Some(Ok(_)));

    use_effect(move || {
        if provider_state_load.read().is_some() {
            return;
        }

        spawn(async move {
            match ai_provider_store::load_provider_state().await {
                Ok(mut loaded_state) => {
                    let resolved = resolve_providers(&loaded_state);
                    if !resolved
                        .iter()
                        .any(|provider| provider.id == loaded_state.selected_provider_id)
                    {
                        loaded_state.selected_provider_id = ppq_provider(None).id;
                    }
                    provider_state_load.set(Some(Ok(loaded_state)));
                }
                Err(e) => {
                    provider_state_load.set(Some(Err(e)));
                }
            }
        });
    });

    use_effect(move || {
        let Some(load_result) = provider_state_load.read().clone() else {
            return;
        };
        match load_result {
            Ok(loaded_state) => {
                state.set(loaded_state);
                save_error.set(None);
            }
            Err(e) => {
                save_error.set(Some(e));
            }
        }
    });

    if provider_state_load.read().is_none() {
        return rsx! {
            div { class: "rounded-xl border border-border bg-card p-6",
                p { class: "text-sm text-muted-foreground", "Loading AI settings..." }
            }
        };
    }

    let providers = resolve_providers(&state.read());
    let selected_provider_id = state.read().selected_provider_id.clone();

    rsx! {
        div { class: "space-y-6",
            if !headerless {
                div { class: "rounded-xl border border-border bg-card p-6",
                    h2 { class: "text-xl font-semibold text-foreground", "AI Settings" }
                    p { class: "mt-2 text-sm text-muted-foreground",
                        "Manage AI providers and local model preferences for this device."
                    }
                }
            }

            div { class: "rounded-xl border border-border bg-card p-6",
                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground", "Providers" }
                div { class: "mt-4 space-y-3",
                    for provider in providers.iter() {
                        {
                            let provider_clone = provider.clone();
                            let provider_id_for_use = provider.id.clone();
                            let provider_id_for_edit = provider.id.clone();
                            let provider_id_for_delete = provider.id.clone();
                            let is_selected = provider.id == selected_provider_id;
                            let is_custom = !provider.is_builtin;
                            let display_base_url = sanitize_display_base_url(&provider.base_url);
                            rsx! {
                                div { key: "{provider.id}", class: "flex flex-col gap-3 rounded-xl border border-border p-4 md:flex-row md:items-center md:justify-between",
                                    div {
                                        div { class: "flex items-center gap-2",
                                            p { class: "font-medium text-foreground", "{provider.name}" }
                                            if is_selected {
                                                span { class: "rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary", "Active" }
                                            }
                                        }
                                        p { class: "text-sm text-muted-foreground", "{display_base_url}" }
                                        p { class: "text-xs text-muted-foreground", "Authentication: {provider.authentication_label()}" }
                                    }
                                    div { class: "flex flex-wrap items-center gap-2",
                                        if !is_selected {
                                            button {
                                                r#type: "button",
                                                class: "rounded-lg border border-border px-3 py-2 text-sm transition hover:bg-accent",
                                                disabled: !provider_state_ready || *is_saving.read(),
                                                onclick: move |_| {
                                                    let mut next_state = state.read().clone();
                                                    next_state.selected_provider_id = provider_id_for_use.clone();
                                                    persist_provider_state(
                                                        next_state,
                                                        state,
                                                        is_saving,
                                                        save_error,
                                                        || {},
                                                    );
                                                },
                                                "Use"
                                            }
                                        }
                                        if is_custom {
                                            button {
                                                r#type: "button",
                                                class: "rounded-lg border border-border px-3 py-2 text-sm transition hover:bg-accent",
                                                disabled: !provider_state_ready || *is_saving.read(),
                                                onclick: move |_| {
                                                    editing_provider_id.set(Some(provider_id_for_edit.clone()));
                                                    name.set(provider_clone.name.clone());
                                                    provider_id.set(provider_clone.id.clone());
                                                    base_url.set(provider_clone.base_url.clone());
                                                    api_key.set(match &provider_clone.auth {
                                                        ProviderAuth::BearerToken(value) => value.clone(),
                                                        ProviderAuth::PpqManaged { .. } => String::new(),
                                                    });
                                                    save_error.set(None);
                                                },
                                                "Edit"
                                            }
                                            button {
                                                r#type: "button",
                                                class: "rounded-lg border border-red-500/20 px-3 py-2 text-sm text-red-600 transition hover:bg-red-500/10 dark:text-red-400",
                                                disabled: !provider_state_ready || *is_saving.read(),
                                                onclick: move |_| {
                                                    let mut next_state = state.read().clone();
                                                    next_state.custom_providers.retain(|item| item.id != provider_id_for_delete);
                                                    next_state.selected_model_by_provider.remove(&provider_id_for_delete);
                                                    if next_state.selected_provider_id == provider_id_for_delete {
                                                        next_state.selected_provider_id = ppq_provider(None).id;
                                                    }
                                                    let should_reset_editor = editing_provider_id.read().as_deref()
                                                        == Some(provider_id_for_delete.as_str());
                                                    persist_provider_state(
                                                        next_state,
                                                        state,
                                                        is_saving,
                                                        save_error,
                                                        move || {
                                                            if should_reset_editor {
                                                                editing_provider_id.set(None);
                                                                name.set(String::new());
                                                                provider_id.set(String::new());
                                                                base_url.set(String::new());
                                                                api_key.set(String::new());
                                                                save_error.set(None);
                                                            }
                                                        },
                                                    );
                                                },
                                                "Delete"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            PpqSettingsPanel {
                state,
                is_saving,
                save_error,
                provider_state_ready,
            }

            div { class: "rounded-xl border border-border bg-card p-6",
                h3 { class: "text-base font-semibold text-foreground",
                    if editing_provider_id.read().is_some() { "Edit Custom Provider" } else { "Add Custom Provider" }
                }
                p { class: "mt-1 text-sm text-muted-foreground",
                    "Configure a custom OpenAI-compatible provider with local-only credentials."
                }
                div { class: "mt-4 grid gap-4 md:grid-cols-2",
                    label { class: "block space-y-2",
                        span { class: "text-sm font-medium text-foreground", "Name *" }
                        input {
                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                            placeholder: "e.g., My Custom API",
                            value: "{name}",
                            disabled: !provider_state_ready || *is_saving.read(),
                            oninput: move |evt| name.set(evt.value()),
                        }
                    }
                    label { class: "block space-y-2",
                        span { class: "text-sm font-medium text-foreground", "ID *" }
                        input {
                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                            placeholder: "e.g., my-custom-api",
                            value: "{provider_id}",
                            disabled: !provider_state_ready || *is_saving.read(),
                            oninput: move |evt| provider_id.set(evt.value()),
                        }
                    }
                    label { class: "block space-y-2 md:col-span-2",
                        span { class: "text-sm font-medium text-foreground", "Base URL *" }
                        input {
                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                            placeholder: "https://api.example.com/v1",
                            value: "{base_url}",
                            disabled: !provider_state_ready || *is_saving.read(),
                            oninput: move |evt| base_url.set(evt.value()),
                        }
                    }
                    div { class: "space-y-2 md:col-span-2",
                        span { class: "text-sm font-medium text-foreground", "Authentication" }
                        p { class: "text-sm text-muted-foreground", "API Key" }
                    }
                    label { class: "block space-y-2 md:col-span-2",
                        span { class: "text-sm font-medium text-foreground", "API Key *" }
                        input {
                            r#type: "password",
                            autocomplete: "new-password",
                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                            value: "{api_key}",
                            disabled: !provider_state_ready || *is_saving.read(),
                            oninput: move |evt| api_key.set(evt.value()),
                        }
                    }
                }

                if let Some(err) = save_error.read().as_ref() {
                    p { class: "mt-4 text-sm text-red-600 dark:text-red-400", "{err}" }
                }

                div { class: "mt-5 flex flex-wrap items-center gap-3",
                    button {
                        r#type: "button",
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60",
                        disabled: !provider_state_ready || *is_saving.read(),
                        onclick: move |_| {
                            let editing = editing_provider_id.read().clone();
                            let current_name = name.read().clone();
                            let current_provider_id = provider_id.read().clone();
                            let current_base_url = base_url.read().clone();
                            let current_api_key = api_key.read().clone();
                            let current_state = state.read().clone();
                            match build_custom_provider(
                                &current_name,
                                &current_provider_id,
                                &current_base_url,
                                &current_api_key,
                                &current_state,
                                editing.as_deref(),
                            ) {
                                Ok(provider) => {
                                    let mut next_state = current_state;
                                    let current_selected = next_state.selected_provider_id.clone();
                                    if let Some(original_id) = editing.as_deref() {
                                        if let Some(existing) = next_state
                                            .custom_providers
                                            .iter_mut()
                                            .find(|item| item.id == original_id)
                                        {
                                            *existing = provider.clone();
                                        }
                                        if original_id != provider.id {
                                            if let Some(saved_model) = next_state.selected_model_by_provider.remove(original_id) {
                                                next_state.selected_model_by_provider.insert(provider.id.clone(), saved_model);
                                            }
                                        }
                                        if Some(current_selected.as_str()) == editing.as_deref() {
                                            next_state.selected_provider_id = provider.id.clone();
                                        }
                                    } else {
                                        next_state.custom_providers.push(provider.clone());
                                        next_state.selected_provider_id = provider.id.clone();
                                    }
                                    persist_provider_state(
                                        next_state,
                                        state,
                                        is_saving,
                                        save_error,
                                        move || {
                                            editing_provider_id.set(None);
                                            name.set(String::new());
                                            provider_id.set(String::new());
                                            base_url.set(String::new());
                                            api_key.set(String::new());
                                        },
                                    );
                                }
                                Err(err) => save_error.set(Some(err)),
                            }
                        },
                        if *is_saving.read() {
                            "Saving..."
                        } else if editing_provider_id.read().is_some() {
                            "Save Provider"
                        } else {
                            "Add Provider"
                        }
                    }
                    if editing_provider_id.read().is_some() {
                        button {
                            r#type: "button",
                            class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent",
                            disabled: !provider_state_ready || *is_saving.read(),
                            onclick: move |_| {
                                editing_provider_id.set(None);
                                name.set(String::new());
                                provider_id.set(String::new());
                                base_url.set(String::new());
                                api_key.set(String::new());
                                save_error.set(None);
                            },
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

fn persist_provider_state(
    next_state: AiProviderState,
    mut state: Signal<AiProviderState>,
    mut is_saving: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    on_success: impl FnOnce() + 'static,
) {
    is_saving.set(true);
    save_error.set(None);
    if let Err(e) = ai_provider_store::cache_provider_state(&next_state) {
        save_error.set(Some(e));
        is_saving.set(false);
        return;
    }
    spawn(async move {
        match ai_provider_store::save_provider_state(&next_state).await {
            Ok(()) => {
                state.set(next_state);
                on_success();
            }
            Err(e) => save_error.set(Some(e)),
        }
        is_saving.set(false);
    });
}

fn build_custom_provider(
    name: &str,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    state: &AiProviderState,
    editing_provider_id: Option<&str>,
) -> Result<CustomAiProvider, String> {
    let name = sanitize_provider_input(name);
    let provider_id = sanitize_provider_input(provider_id);
    let base_url = normalize_base_url(base_url);
    let api_key = sanitize_provider_input(api_key);

    if name.is_empty() {
        return Err("Name is required".to_string());
    }
    if provider_id.is_empty() {
        return Err("ID is required".to_string());
    }
    if provider_id == ppq_provider(None).id {
        return Err("ID is reserved for the built-in PPQ provider".to_string());
    }
    if base_url.is_empty() {
        return Err("Base URL is required".to_string());
    }
    validate_provider_base_url(&base_url)?;
    if api_key.is_empty() {
        return Err("API key is required".to_string());
    }

    let is_different_original = match editing_provider_id {
        Some(original) => original != provider_id,
        None => true,
    };
    let duplicate = state
        .custom_providers
        .iter()
        .any(|provider| provider.id == provider_id && is_different_original);
    if duplicate {
        return Err("ID must be unique across custom providers".to_string());
    }

    Ok(CustomAiProvider {
        id: provider_id,
        name,
        base_url,
        api_key,
        provider_kind: AiProviderKind::OpenAiCompatible,
    })
}

fn validate_provider_base_url(base_url: &str) -> Result<(), String> {
    let parsed =
        Url::parse(base_url).map_err(|_| "Base URL must be an absolute URL".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Base URL must not include embedded credentials".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Base URL must not include a query string or fragment".to_string());
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = parsed
                .host_str()
                .ok_or_else(|| "Base URL must include a host".to_string())?;
            if is_private_development_host(host) {
                Ok(())
            } else {
                Err("Base URL must use https unless it targets localhost or a private development host".to_string())
            }
        }
        _ => Err("Base URL must be an absolute HTTP/HTTPS URL".to_string()),
    }
}

fn sanitize_display_base_url(base_url: &str) -> String {
    match Url::parse(base_url) {
        Ok(mut parsed) => {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string().trim_end_matches('/').to_string()
        }
        Err(_) => LEADING_CREDENTIALS_REGEX
            .replace(base_url, "")
            .trim_end_matches('/')
            .to_string(),
    }
}

fn is_private_development_host(host: &str) -> bool {
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }

    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            let octets = ip.octets();
            octets[0] == 10
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        }
        Ok(IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_provider_input() {
        let state = AiProviderState::default();
        let provider = build_custom_provider(
            " Custom ",
            " custom ",
            " https://api.example.com/v1/ ",
            " secret ",
            &state,
            None,
        )
        .unwrap();

        assert_eq!(provider.name, "Custom");
        assert_eq!(provider.id, "custom");
        assert_eq!(provider.base_url, "https://api.example.com/v1");
        assert_eq!(provider.api_key, "secret");
    }

    #[test]
    fn rejects_insecure_public_base_url() {
        let err = validate_provider_base_url("http://api.example.com/v1").unwrap_err();
        assert!(err.contains("https"));
    }

    #[test]
    fn allows_insecure_private_development_base_url() {
        assert!(validate_provider_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_provider_base_url("http://192.168.1.15:8080/v1").is_ok());
    }

    #[test]
    fn rejects_base_url_with_embedded_credentials() {
        let err = validate_provider_base_url("https://user:pass@example.com/v1").unwrap_err();
        assert!(err.contains("credentials"));
    }

    #[test]
    fn rejects_base_url_with_query_or_fragment() {
        assert!(validate_provider_base_url("https://api.example.com/v1?token=secret").is_err());
        assert!(validate_provider_base_url("https://api.example.com/v1#fragment").is_err());
    }
}
