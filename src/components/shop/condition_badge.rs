//! ConditionBadge component - displays product condition

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ConditionBadgeProps {
    pub condition: String,
}

/// Badge showing product condition (New, Used, etc.)
#[component]
pub fn ConditionBadge(props: ConditionBadgeProps) -> Element {
    let condition = props.condition.to_lowercase();

    let (bg_class, text_class, label): (&str, &str, String) = match condition.as_str() {
        "new" => ("bg-green-100 dark:bg-green-900/30", "text-green-700 dark:text-green-400", "New".to_string()),
        "like_new" | "like new" => ("bg-blue-100 dark:bg-blue-900/30", "text-blue-700 dark:text-blue-400", "Like New".to_string()),
        "used" | "good" => ("bg-amber-100 dark:bg-amber-900/30", "text-amber-700 dark:text-amber-400", "Used".to_string()),
        "fair" => ("bg-orange-100 dark:bg-orange-900/30", "text-orange-700 dark:text-orange-400", "Fair".to_string()),
        "refurbished" => ("bg-purple-100 dark:bg-purple-900/30", "text-purple-700 dark:text-purple-400", "Refurbished".to_string()),
        _ => ("bg-muted", "text-muted-foreground", props.condition.clone()),
    };

    rsx! {
        span { class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {bg_class} {text_class}",
            "{label}"
        }
    }
}
