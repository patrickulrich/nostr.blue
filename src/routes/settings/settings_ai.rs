use crate::components::AiSettingsPanel;
use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn SettingsAi() -> Element {
    rsx! {
        div { class: "space-y-6",
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between gap-4 flex-wrap",
                    div {
                        h2 { class: "text-2xl font-semibold text-gray-900 dark:text-white",
                            "AI Settings"
                        }
                        p { class: "text-gray-600 dark:text-gray-400 mt-2",
                            "Manage AI providers and local chat preferences for this device."
                        }
                    }
                    Link {
                        to: Route::Settings {},
                        class: "px-4 py-2 rounded-lg border border-border text-sm text-foreground hover:bg-accent transition",
                        "← Back to Settings"
                    }
                }
            }
            AiSettingsPanel {}
        }
    }
}
