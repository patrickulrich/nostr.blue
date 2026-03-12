use crate::components::icons::{SendIcon, SettingsIcon, SparklesIcon, TrashIcon};
use crate::components::ClientInitializing;
use crate::services::ai_chat::{
    get_available_models, send_chat_message, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, ChatModel, ChatRole, ToolCall, ToolDefinition, ToolFunction,
};
use crate::stores::ai_provider_store::{
    self, normalize_base_url, resolve_providers, sanitize_provider_input, shakespeare_provider,
    AiProviderConfig, AiProviderKind, AiProviderState, CustomAiProvider,
};
use crate::stores::{nostr_client, theme_store};
use crate::utils::markdown::render_markdown;
use dioxus::document;
use dioxus::prelude::*;
use serde_json::json;
use url::Url;

const SYSTEM_PROMPT: &str = "You are Nostrich, an AI assistant inside nostr.blue. Be concise and helpful for the user. Your personality is a fun ostrich that represents the nostr community.";
const THEME_TOOL_NAME: &str = "set_theme";

#[derive(Clone, Debug, PartialEq)]
struct DisplayMessage {
    id: String,
    role: DisplayRole,
    content: String,
    tool_calls: Vec<ExecutedToolCall>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
struct ExecutedToolCall {
    id: String,
    name: String,
    result: String,
}

#[derive(Clone, serde::Deserialize)]
struct ThemeToolArgs {
    theme: String,
}

#[derive(Clone, PartialEq, Props)]
struct AISettingsModalProps {
    state: AiProviderState,
    on_close: EventHandler<MouseEvent>,
    on_saved: EventHandler<AiProviderState>,
}

#[component]
pub fn AIChat() -> Element {
    let mut messages = use_signal(Vec::<DisplayMessage>::new);
    let mut input = use_signal(String::new);
    let loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut models = use_signal(Vec::<ChatModel>::new);
    let mut selected_model = use_signal(String::new);
    let mut provider_state = use_signal(AiProviderState::default);
    let mut providers = use_signal(|| vec![shakespeare_provider()]);
    let mut provider_state_loaded = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let messages_container_id = use_signal(|| "ai-chat-messages".to_string());

    use_effect(move || {
        if *provider_state_loaded.read() {
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
                        loaded_state.selected_provider_id = shakespeare_provider().id;
                    }
                    providers.set(resolve_providers(&loaded_state));
                    provider_state.set(loaded_state);
                }
                Err(e) => {
                    error.set(Some(e));
                    let default_state = AiProviderState::default();
                    providers.set(resolve_providers(&default_state));
                    provider_state.set(default_state);
                }
            }
            provider_state_loaded.set(true);
        });
    });

    use_effect(move || {
        let _ = messages.read().len();
        let id = messages_container_id.read().clone();
        spawn(async move {
            let script = format!(
                "(() => {{ const el = document.getElementById({:?}); if (el) {{ el.scrollTop = el.scrollHeight; return true; }} return false; }})()",
                id
            );
            let _ = document::eval(&script).await;
        });
    });

    use_effect(move || {
        if !*provider_state_loaded.read() {
            return;
        }

        let selected_provider_id = provider_state.read().selected_provider_id.clone();
        let available_providers = providers.read().clone();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        spawn(async move {
            let Some(provider) = available_providers
                .into_iter()
                .find(|provider| provider.id == selected_provider_id)
            else {
                return;
            };

            if provider.requires_signer() && !has_signer {
                models.set(Vec::new());
                selected_model.set(String::new());
                error.set(None);
                return;
            }

            match get_available_models(&provider).await {
                Ok(available_models) => {
                    if provider_state.read().selected_provider_id != provider.id {
                        return;
                    }
                    let selected_is_valid = available_models
                        .iter()
                        .any(|model| model.id == *selected_model.read());
                    if !selected_is_valid {
                        selected_model.set(
                            available_models
                                .first()
                                .map(|model| model.id.clone())
                                .unwrap_or_default(),
                        );
                    }
                    models.set(available_models);
                    error.set(None);
                }
                Err(e) => {
                    if provider_state.read().selected_provider_id != provider.id {
                        return;
                    }
                    models.set(Vec::new());
                    selected_model.set(String::new());
                    error.set(Some(e));
                }
            }
        });
    });

    if !*nostr_client::CLIENT_INITIALIZED.read() || !*provider_state_loaded.read() {
        return rsx! { ClientInitializing {} };
    }

    let active_provider = current_provider(&providers.read(), &provider_state.read());
    let shakespeare_blocked =
        active_provider.requires_signer() && !*nostr_client::HAS_SIGNER.read();

    let provider_for_keydown = active_provider.clone();
    let provider_for_click = active_provider.clone();

    rsx! {
        div { class: "min-h-screen flex flex-col bg-background",
            div { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur-sm",
                div { class: "mx-auto flex max-w-5xl items-center justify-between gap-4 px-4 py-4",
                    div { class: "flex items-center gap-3",
                        div { class: "flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary",
                            SparklesIcon { class: "w-5 h-5".to_string() }
                        }
                        div {
                            h1 { class: "text-xl font-semibold", "AI Chat" }
                            p { class: "text-sm text-muted-foreground",
                                "Provider: {active_provider.name}"
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        select {
                            class: "h-10 rounded-lg border border-border bg-card px-3 text-sm text-foreground focus:outline-hidden",
                            value: "{selected_model}",
                            disabled: models.read().is_empty() || *loading.read() || shakespeare_blocked,
                            onchange: move |evt| selected_model.set(evt.value()),
                            if models.read().is_empty() {
                                option { value: "", if shakespeare_blocked { "Sign in for Shakespeare models" } else { "Loading models..." } }
                            } else {
                                for model in models.read().iter() {
                                    {
                                        let label = if model.total_cost == Some(0.0) {
                                            format!("{} · FREE", model.name)
                                        } else {
                                            model.name.clone()
                                        };
                                        rsx! {
                                            option { key: "{model.id}", value: "{model.id}", "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            class: if messages.read().is_empty() {
                                "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground opacity-50"
                            } else {
                                "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent"
                            },
                            disabled: messages.read().is_empty() || *loading.read(),
                            title: "Clear conversation",
                            onclick: move |_| {
                                messages.set(Vec::new());
                                error.set(None);
                            },
                            TrashIcon { class: "w-4 h-4".to_string() }
                        }
                        button {
                            class: "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent",
                            disabled: *loading.read(),
                            title: "AI settings",
                            onclick: move |_| show_settings.set(true),
                            SettingsIcon { class: "w-4 h-4".to_string() }
                        }
                    }
                }
            }

            div {
                id: "{messages_container_id}",
                class: "flex-1 overflow-y-auto",
                div { class: "mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6",
                    if shakespeare_blocked {
                        SignInGate {}
                    } else if messages.read().is_empty() {
                        EmptyState { provider_name: active_provider.name.clone() }
                    } else {
                        for message in messages.read().iter() {
                            MessageBubble { key: "{message.id}", message: message.clone() }
                        }
                    }

                    if *loading.read() {
                        div { class: "max-w-3xl rounded-2xl border border-border bg-card px-4 py-3 text-sm text-muted-foreground shadow-sm",
                            "Thinking..."
                        }
                    }

                    if let Some(err) = error.read().as_ref() {
                        div { class: "max-w-3xl rounded-2xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-600 dark:text-red-400",
                            "{err}"
                        }
                    }
                }
            }

            div { class: "border-t border-border bg-background",
                div { class: "mx-auto max-w-5xl px-4 py-4",
                    div { class: "rounded-2xl border border-border bg-card p-3 shadow-sm",
                        textarea {
                            class: "min-h-[96px] w-full resize-none bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-hidden disabled:cursor-not-allowed disabled:opacity-60",
                            placeholder: if shakespeare_blocked {
                                "Open AI settings to switch providers or sign in for Shakespeare..."
                            } else if selected_model.read().is_empty() {
                                "Select a model first..."
                            } else {
                                "Send a message..."
                            },
                            value: "{input}",
                            disabled: selected_model.read().is_empty() || *loading.read() || shakespeare_blocked,
                            oninput: move |evt| input.set(evt.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                    evt.prevent_default();
                                    submit_message(input, selected_model, loading, error, messages, provider_for_keydown.clone());
                                }
                            },
                        }
                        div { class: "mt-3 flex items-center justify-between gap-3",
                            p { class: "text-xs text-muted-foreground", "Enter to send. Shift+Enter for newline." }
                            button {
                                class: if input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read() || shakespeare_blocked {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-muted text-muted-foreground cursor-not-allowed"
                                } else {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-primary text-primary-foreground transition hover:bg-primary/90"
                                },
                                disabled: input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read() || shakespeare_blocked,
                                onclick: move |_| submit_message(input, selected_model, loading, error, messages, provider_for_click.clone()),
                                SendIcon { class: "w-4 h-4".to_string() }
                            }
                        }
                    }
                }
            }

            if *show_settings.read() {
                AISettingsModal {
                    state: provider_state.read().clone(),
                    on_close: move |_| show_settings.set(false),
                    on_saved: move |new_state: AiProviderState| {
                        providers.set(resolve_providers(&new_state));
                        provider_state.set(new_state);
                        show_settings.set(false);
                        models.set(Vec::new());
                        selected_model.set(String::new());
                        error.set(None);
                    },
                }
            }
        }
    }
}

#[component]
fn SignInGate() -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center p-6",
            div { class: "max-w-md w-full rounded-2xl border border-border bg-card p-8 text-center shadow-sm",
                div { class: "mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-7 h-7".to_string() }
                }
                h2 { class: "text-2xl font-semibold", "Sign in to use Shakespeare" }
                p { class: "mt-2 text-sm text-muted-foreground",
                    "Shakespeare uses NIP-98 authenticated requests. Sign in, or open AI settings and switch to a custom provider with your own API key."
                }
            }
        }
    }
}

#[component]
fn EmptyState(provider_name: String) -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center",
            div { class: "max-w-2xl text-center",
                div { class: "mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-3xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-8 h-8".to_string() }
                }
                h2 { class: "text-3xl font-semibold tracking-tight", "AI Chat" }
                p { class: "mt-3 text-base text-muted-foreground",
                    "Ask questions, iterate on ideas, or switch the app between light, dark, and system theme. Current provider: {provider_name}."
                }
            }
        }
    }
}

#[component]
fn MessageBubble(message: DisplayMessage) -> Element {
    let is_user = message.role == DisplayRole::User;
    let html_content = if is_user {
        None
    } else {
        Some(render_markdown(&message.content))
    };

    rsx! {
        div { class: if is_user { "flex justify-end" } else { "flex justify-start" },
            div { class: if is_user {
                    "max-w-3xl rounded-2xl bg-primary px-4 py-3 text-sm text-primary-foreground shadow-sm"
                } else {
                    "max-w-3xl rounded-2xl border border-border bg-card px-4 py-3 text-sm text-foreground shadow-sm"
                },
                if is_user {
                    p { class: "whitespace-pre-wrap break-words", "{message.content}" }
                } else if let Some(rendered) = html_content {
                    div {
                        class: "prose prose-sm max-w-none prose-neutral dark:prose-invert [&_p]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3 [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5",
                        dangerous_inner_html: "{rendered}",
                    }
                }
                if !message.tool_calls.is_empty() {
                    div { class: "mt-4 space-y-2 border-t border-border pt-3",
                        for call in message.tool_calls.iter() {
                            div { key: "{call.id}", class: "rounded-lg bg-muted/60 px-3 py-2 text-xs text-muted-foreground",
                                p { class: "font-medium text-foreground", "Tool: {call.name}" }
                                p { class: "mt-1 whitespace-pre-wrap break-words", "{call.result}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AISettingsModal(props: AISettingsModalProps) -> Element {
    let state = use_signal(|| props.state.clone());
    let mut save_error = use_signal(|| None::<String>);
    let is_saving = use_signal(|| false);
    let mut editing_provider_id = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut provider_id = use_signal(String::new);
    let mut base_url = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let on_close = props.on_close;
    let on_saved = props.on_saved;

    let providers = resolve_providers(&state.read());
    let selected_provider_id = state.read().selected_provider_id.clone();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm",
            onclick: move |evt| on_close.call(evt),
            div {
                class: "max-h-[90vh] w-full max-w-3xl overflow-y-auto rounded-2xl border border-border bg-background shadow-xl",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-border px-6 py-4",
                    div {
                        h2 { class: "text-xl font-semibold", "AI Settings" }
                        p { class: "text-sm text-muted-foreground", "Manage local AI providers for this device." }
                    }
                    button {
                        class: "rounded-lg p-2 text-muted-foreground transition hover:bg-accent",
                        onclick: move |evt| on_close.call(evt),
                        "Close"
                    }
                }

                div { class: "space-y-6 px-6 py-5",
                    div { class: "rounded-xl border border-border bg-card p-4",
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
                                    let display_base_url = provider.base_url.clone();
                                    rsx! {
                                        div { key: "{provider.id}", class: "flex flex-col gap-3 rounded-xl border border-border p-4 md:flex-row md:items-center md:justify-between",
                                            div {
                                                div { class: "flex items-center gap-2",
                                                    p { class: "font-medium", "{provider.name}" }
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
                                                        class: "rounded-lg border border-border px-3 py-2 text-sm transition hover:bg-accent",
                                                        disabled: *is_saving.read(),
                                                        onclick: move |_| {
                                                            let mut next_state = state.read().clone();
                                                            next_state.selected_provider_id = provider_id_for_use.clone();
                                                            persist_provider_state(next_state, state, is_saving, save_error, on_saved);
                                                        },
                                                        "Use"
                                                    }
                                                }
                                                if is_custom {
                                                    button {
                                                        class: "rounded-lg border border-border px-3 py-2 text-sm transition hover:bg-accent",
                                                        disabled: *is_saving.read(),
                                                        onclick: move |_| {
                                                            editing_provider_id.set(Some(provider_id_for_edit.clone()));
                                                            name.set(provider_clone.name.clone());
                                                            provider_id.set(provider_clone.id.clone());
                                                            base_url.set(provider_clone.base_url.clone());
                                                            api_key.set(match &provider_clone.auth {
                                                                ai_provider_store::ProviderAuth::BearerToken(value) => value.clone(),
                                                                ai_provider_store::ProviderAuth::Nip98 => String::new(),
                                                            });
                                                            save_error.set(None);
                                                        },
                                                        "Edit"
                                                    }
                                                    button {
                                                        class: "rounded-lg border border-red-500/20 px-3 py-2 text-sm text-red-600 transition hover:bg-red-500/10 dark:text-red-400",
                                                        disabled: *is_saving.read(),
                                                        onclick: move |_| {
                                                            let mut next_state = state.read().clone();
                                                            next_state.custom_providers.retain(|item| item.id != provider_id_for_delete);
                                                            if next_state.selected_provider_id == provider_id_for_delete {
                                                                next_state.selected_provider_id = shakespeare_provider().id;
                                                            }
                                                            persist_provider_state(next_state, state, is_saving, save_error, on_saved);
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

                    div { class: "rounded-xl border border-border bg-card p-4",
                        h3 { class: "text-base font-semibold",
                            if editing_provider_id.read().is_some() { "Edit Custom Provider" } else { "Add Custom Provider" }
                        }
                        p { class: "mt-1 text-sm text-muted-foreground",
                            "Configure a custom OpenAI-compatible provider with local-only credentials."
                        }
                        div { class: "mt-4 grid gap-4 md:grid-cols-2",
                            label { class: "block space-y-2",
                                span { class: "text-sm font-medium", "Name *" }
                                input {
                                    class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-hidden",
                                    placeholder: "e.g., My Custom API",
                                    value: "{name}",
                                    disabled: *is_saving.read(),
                                    oninput: move |evt| name.set(evt.value()),
                                }
                            }
                            label { class: "block space-y-2",
                                span { class: "text-sm font-medium", "ID *" }
                                input {
                                    class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-hidden",
                                    placeholder: "e.g., my-custom-api",
                                    value: "{provider_id}",
                                    disabled: *is_saving.read(),
                                    oninput: move |evt| provider_id.set(evt.value()),
                                }
                            }
                            label { class: "block space-y-2 md:col-span-2",
                                span { class: "text-sm font-medium", "Base URL *" }
                                input {
                                    class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-hidden",
                                    placeholder: "https://api.example.com/v1",
                                    value: "{base_url}",
                                    disabled: *is_saving.read(),
                                    oninput: move |evt| base_url.set(evt.value()),
                                }
                            }
                            div { class: "space-y-2 md:col-span-2",
                                span { class: "text-sm font-medium", "Authentication" }
                                p { class: "text-sm text-muted-foreground", "API Key" }
                            }
                            label { class: "block space-y-2 md:col-span-2",
                                span { class: "text-sm font-medium", "API Key *" }
                                input {
                                    r#type: "password",
                                    class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-hidden",
                                    value: "{api_key}",
                                    disabled: *is_saving.read(),
                                    oninput: move |evt| api_key.set(evt.value()),
                                }
                            }
                        }

                        if let Some(err) = save_error.read().as_ref() {
                            p { class: "mt-4 text-sm text-red-600 dark:text-red-400", "{err}" }
                        }

                        div { class: "mt-5 flex flex-wrap items-center gap-3",
                            button {
                                class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60",
                                disabled: *is_saving.read(),
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
                                            if let Some(original_id) = editing.as_deref() {
                                                if let Some(existing) = next_state
                                                    .custom_providers
                                                    .iter_mut()
                                                    .find(|item| item.id == original_id)
                                                {
                                                    *existing = provider.clone();
                                                }
                                            } else {
                                                next_state.custom_providers.push(provider.clone());
                                            }
                                            next_state.selected_provider_id = provider.id.clone();
                                            editing_provider_id.set(None);
                                            name.set(String::new());
                                            provider_id.set(String::new());
                                            base_url.set(String::new());
                                            api_key.set(String::new());
                                            persist_provider_state(next_state, state, is_saving, save_error, on_saved);
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
                                    class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent",
                                    disabled: *is_saving.read(),
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
    }
}

fn persist_provider_state(
    next_state: AiProviderState,
    mut state: Signal<AiProviderState>,
    mut is_saving: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    on_saved: EventHandler<AiProviderState>,
) {
    is_saving.set(true);
    save_error.set(None);
    spawn(async move {
        match ai_provider_store::save_provider_state(&next_state).await {
            Ok(()) => {
                state.set(next_state.clone());
                on_saved.call(next_state);
            }
            Err(e) => save_error.set(Some(e)),
        }
        is_saving.set(false);
    });
}

fn current_provider(providers: &[AiProviderConfig], state: &AiProviderState) -> AiProviderConfig {
    providers
        .iter()
        .find(|provider| provider.id == state.selected_provider_id)
        .cloned()
        .unwrap_or_else(shakespeare_provider)
}

fn build_api_messages(messages: &[DisplayMessage]) -> Vec<ChatMessage> {
    let mut api_messages = vec![ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    }];
    for message in messages {
        api_messages.push(ChatMessage {
            role: match message.role {
                DisplayRole::User => ChatRole::User,
                DisplayRole::Assistant => ChatRole::Assistant,
            },
            content: message.content.clone(),
        });
    }
    api_messages
}

fn submit_message(
    mut input: Signal<String>,
    selected_model: Signal<String>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut messages: Signal<Vec<DisplayMessage>>,
    provider: AiProviderConfig,
) {
    if *loading.read() {
        return;
    }
    let text = input.read().trim().to_string();
    let model = selected_model.read().clone();
    if text.is_empty() || model.is_empty() {
        return;
    }

    let user_message = DisplayMessage {
        id: format!("user-{}", crate::platform::timestamp::now_millis()),
        role: DisplayRole::User,
        content: text,
        tool_calls: Vec::new(),
    };
    let mut next_messages = messages.read().clone();
    next_messages.push(user_message);
    messages.set(next_messages.clone());
    input.set(String::new());
    error.set(None);
    loading.set(true);

    spawn(async move {
        let base_request = ChatCompletionRequest {
            model: model.clone(),
            messages: build_api_messages(&next_messages),
            tools: provider.supports_tools().then(theme_tool_definitions),
        };

        match send_chat_message(&provider, &base_request).await {
            Ok(response) => {
                apply_chat_response(response, next_messages, model, provider, messages, error)
                    .await;
            }
            Err(e) => {
                error.set(Some(e));
            }
        }
        loading.set(false);
    });
}

fn theme_tool_definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: THEME_TOOL_NAME.to_string(),
            description: "Switch the app theme. Supported values are: light, dark, system."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "theme": {
                        "type": "string",
                        "enum": ["light", "dark", "system"],
                        "description": "Theme mode to apply."
                    }
                },
                "required": ["theme"]
            }),
        },
    }]
}

async fn apply_chat_response(
    response: ChatCompletionResponse,
    prior_messages: Vec<DisplayMessage>,
    model: String,
    provider: AiProviderConfig,
    mut messages: Signal<Vec<DisplayMessage>>,
    mut error: Signal<Option<String>>,
) {
    let Some(choice) = response.choices.into_iter().next() else {
        error.set(Some(
            "Chat response did not include any choices".to_string(),
        ));
        return;
    };

    let assistant_content = choice.message.content.unwrap_or_default();
    if !provider.supports_tools() {
        let mut next_messages = prior_messages;
        next_messages.push(DisplayMessage {
            id: format!("assistant-{}", crate::platform::timestamp::now_millis()),
            role: DisplayRole::Assistant,
            content: assistant_content,
            tool_calls: Vec::new(),
        });
        messages.set(next_messages);
        return;
    }

    if choice.message.tool_calls.is_empty() {
        let mut next_messages = prior_messages;
        next_messages.push(DisplayMessage {
            id: format!("assistant-{}", crate::platform::timestamp::now_millis()),
            role: DisplayRole::Assistant,
            content: assistant_content,
            tool_calls: Vec::new(),
        });
        messages.set(next_messages);
        return;
    }

    let executed = execute_tool_calls(&choice.message.tool_calls);
    let mut intermediate_messages = prior_messages.clone();
    intermediate_messages.push(DisplayMessage {
        id: format!(
            "assistant-tool-{}",
            crate::platform::timestamp::now_millis()
        ),
        role: DisplayRole::Assistant,
        content: assistant_content.clone(),
        tool_calls: executed.clone(),
    });
    messages.set(intermediate_messages.clone());

    let mut follow_up_messages = build_api_messages(&prior_messages);
    follow_up_messages.push(ChatMessage {
        role: ChatRole::Assistant,
        content: assistant_content,
    });
    for tool in executed {
        follow_up_messages.push(ChatMessage {
            role: ChatRole::User,
            content: format!("[Tool \"{}\" returned: {}]", tool.name, tool.result),
        });
    }

    let follow_up_request = ChatCompletionRequest {
        model,
        messages: follow_up_messages,
        tools: None,
    };

    match send_chat_message(&provider, &follow_up_request).await {
        Ok(follow_up_response) => {
            if let Some(follow_up_choice) = follow_up_response.choices.into_iter().next() {
                let mut final_messages = intermediate_messages;
                final_messages.push(DisplayMessage {
                    id: format!(
                        "assistant-final-{}",
                        crate::platform::timestamp::now_millis()
                    ),
                    role: DisplayRole::Assistant,
                    content: follow_up_choice.message.content.unwrap_or_default(),
                    tool_calls: Vec::new(),
                });
                messages.set(final_messages);
            } else {
                error.set(Some(
                    "Follow-up response did not include any choices".to_string(),
                ));
            }
        }
        Err(e) => {
            error.set(Some(e));
        }
    }
}

fn execute_tool_calls(tool_calls: &[ToolCall]) -> Vec<ExecutedToolCall> {
    tool_calls
        .iter()
        .map(|call| ExecutedToolCall {
            id: call.id.clone(),
            name: call.function.name.clone(),
            result: execute_tool_call(&call.function.name, &call.function.arguments),
        })
        .collect()
}

fn execute_tool_call(name: &str, arguments: &str) -> String {
    match name {
        THEME_TOOL_NAME => match serde_json::from_str::<ThemeToolArgs>(arguments) {
            Ok(args) => {
                let theme = match args.theme.trim().to_lowercase().as_str() {
                    "light" => theme_store::Theme::Light,
                    "dark" => theme_store::Theme::Dark,
                    "system" => theme_store::Theme::System,
                    other => {
                        return format!(
                            "{{\"error\":\"Unsupported theme '{}'. Supported values: light, dark, system.\"}}",
                            other
                        );
                    }
                };
                theme_store::set_theme(theme);
                format!("{{\"success\":true,\"theme\":\"{}\"}}", theme.as_str())
            }
            Err(e) => format!("{{\"error\":\"Invalid tool arguments: {}\"}}", e),
        },
        other => format!("{{\"error\":\"Unknown tool: {}\"}}", other),
    }
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
    if provider_id == shakespeare_provider().id {
        return Err("ID is reserved for the built-in Shakespeare provider".to_string());
    }
    if base_url.is_empty() {
        return Err("Base URL is required".to_string());
    }
    Url::parse(&base_url).map_err(|_| "Base URL must be an absolute URL".to_string())?;
    if api_key.is_empty() {
        return Err("API key is required".to_string());
    }

    let duplicate = state.custom_providers.iter().any(|provider| {
        provider.id == provider_id
            && editing_provider_id
                .map(|original| original != provider.id)
                .unwrap_or(true)
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_provider_input() {
        let state = AiProviderState::default();
        let provider = build_custom_provider(
            "My API",
            "my-api",
            "https://api.example.com/v1/",
            "secret",
            &state,
            None,
        )
        .unwrap();
        assert_eq!(provider.base_url, "https://api.example.com/v1");
    }

    #[test]
    fn rejects_duplicate_provider_ids() {
        let state = AiProviderState {
            selected_provider_id: "existing".to_string(),
            custom_providers: vec![CustomAiProvider {
                id: "existing".to_string(),
                name: "Existing".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                api_key: "secret".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
        };

        let error = build_custom_provider(
            "Other",
            "existing",
            "https://api.other.com/v1",
            "secret",
            &state,
            None,
        )
        .unwrap_err();
        assert!(error.contains("unique"));
    }
}
