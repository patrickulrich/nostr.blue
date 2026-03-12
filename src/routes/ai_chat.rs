use crate::components::icons::{SendIcon, SparklesIcon, TrashIcon};
use crate::components::ClientInitializing;
use crate::services::ai_chat::{
    get_available_models, send_chat_message, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, ChatRole, Model, ToolCall, ToolDefinition, ToolFunction,
};
use crate::stores::{nostr_client, theme_store};
use crate::utils::markdown::render_markdown;
use dioxus::document;
use dioxus::prelude::*;
use serde_json::json;

const SYSTEM_PROMPT: &str = "You are Dork, an AI assistant inside nostr.blue. Be concise and helpful. You can use the set_theme tool to switch the app theme between light, dark, and system. If a user asks for a theme change, pick the most appropriate of those three options and briefly describe the choice.";
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

#[component]
pub fn AIChat() -> Element {
    let mut messages = use_signal(Vec::<DisplayMessage>::new);
    let mut input = use_signal(String::new);
    let loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut models = use_signal(Vec::<Model>::new);
    let mut selected_model = use_signal(String::new);
    let messages_container_id = use_signal(|| "ai-chat-messages".to_string());

    use_effect(move || {
        if !*nostr_client::CLIENT_INITIALIZED.read() || !*nostr_client::HAS_SIGNER.read() {
            return;
        }
        if !models.read().is_empty() {
            return;
        }
        spawn(async move {
            match get_available_models().await {
                Ok(available_models) => {
                    if selected_model.read().is_empty() {
                        if let Some(first) = available_models.first() {
                            selected_model.set(first.id.clone());
                        }
                    }
                    models.set(available_models);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
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

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    if !*nostr_client::HAS_SIGNER.read() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center p-6",
                div { class: "max-w-md w-full rounded-2xl border border-border bg-card p-8 text-center shadow-sm",
                    div { class: "mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary",
                        SparklesIcon { class: "w-7 h-7".to_string() }
                    }
                    h2 { class: "text-2xl font-semibold", "Sign in to use AI Chat" }
                    p { class: "mt-2 text-sm text-muted-foreground",
                        "AI Chat uses NIP-98 authenticated requests, so you need a connected signer before sending prompts."
                    }
                }
            }
        };
    }

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
                            p { class: "text-sm text-muted-foreground", "Chat with Shakespeare-backed models inside nostr.blue" }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        select {
                            class: "h-10 rounded-lg border border-border bg-card px-3 text-sm text-foreground focus:outline-hidden",
                            value: "{selected_model}",
                            disabled: models.read().is_empty() || *loading.read(),
                            onchange: move |evt| selected_model.set(evt.value()),
                            if models.read().is_empty() {
                                option { value: "", "Loading models..." }
                            } else {
                                for model in models.read().iter() {
                                    {
                                        let total_cost = parse_total_cost(model);
                                        let label = if total_cost == 0.0 {
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
                    }
                }
            }

            div {
                id: "{messages_container_id}",
                class: "flex-1 overflow-y-auto",
                div { class: "mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6",
                    if messages.read().is_empty() {
                        EmptyState {}
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
                            class: "min-h-[96px] w-full resize-none bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-hidden",
                            placeholder: if selected_model.read().is_empty() { "Select a model first..." } else { "Send a message..." },
                            value: "{input}",
                            disabled: selected_model.read().is_empty() || *loading.read(),
                            oninput: move |evt| input.set(evt.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                    evt.prevent_default();
                                    submit_message(input, selected_model, loading, error, messages);
                                }
                            },
                        }
                        div { class: "mt-3 flex items-center justify-between gap-3",
                            p { class: "text-xs text-muted-foreground", "Enter to send. Shift+Enter for newline." }
                            button {
                                class: if input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read() {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-muted text-muted-foreground cursor-not-allowed"
                                } else {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-primary text-primary-foreground transition hover:bg-primary/90"
                                },
                                disabled: input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read(),
                                onclick: move |_| submit_message(input, selected_model, loading, error, messages),
                                SendIcon { class: "w-4 h-4".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center",
            div { class: "max-w-2xl text-center",
                div { class: "mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-3xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-8 h-8".to_string() }
                }
                h2 { class: "text-3xl font-semibold tracking-tight", "AI Chat" }
                p { class: "mt-3 text-base text-muted-foreground",
                    "Ask questions, iterate on ideas, or switch the app between light, dark, and system theme with the built-in tool."
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
        content: text.clone(),
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
            tools: Some(theme_tool_definitions()),
        };

        match send_chat_message(&base_request).await {
            Ok(response) => {
                apply_chat_response(response, next_messages, model, messages, error).await;
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

    match send_chat_message(&follow_up_request).await {
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

fn parse_total_cost(model: &Model) -> f64 {
    model.pricing.prompt.parse::<f64>().unwrap_or(f64::MAX)
        + model.pricing.completion.parse::<f64>().unwrap_or(f64::MAX)
}
