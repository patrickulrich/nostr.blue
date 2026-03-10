//! OrderStatusBadge component - displays order status with color
use crate::utils::nip99::OrderStatus;
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct OrderStatusBadgeProps {
    pub status: OrderStatus,
}
/// Badge showing order status
#[component]
pub fn OrderStatusBadge(props: OrderStatusBadgeProps) -> Element {
    let (bg_class, text_class, label) = match props.status {
        OrderStatus::Pending => (
            "bg-yellow-100 dark:bg-yellow-900/30",
            "text-yellow-700 dark:text-yellow-400",
            "Pending",
        ),
        OrderStatus::Confirmed => (
            "bg-blue-100 dark:bg-blue-900/30",
            "text-blue-700 dark:text-blue-400",
            "Confirmed",
        ),
        OrderStatus::Processing => (
            "bg-purple-100 dark:bg-purple-900/30",
            "text-purple-700 dark:text-purple-400",
            "Processing",
        ),
        OrderStatus::Completed => (
            "bg-green-100 dark:bg-green-900/30",
            "text-green-700 dark:text-green-400",
            "Completed",
        ),
        OrderStatus::Cancelled => (
            "bg-gray-100 dark:bg-gray-900/30",
            "text-gray-700 dark:text-gray-400",
            "Cancelled",
        ),
    };
    rsx! {
        span { class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {bg_class} {text_class}",
            "{label}"
        }
    }
}
