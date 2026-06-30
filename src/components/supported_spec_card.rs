use crate::routes::nips::registry::SupportedSpec;
use crate::routes::Route;
use dioxus::prelude::*;

/// Card for a single supported protocol spec on the "Our NIPs" grid.
#[component]
pub fn SupportedSpecCard(spec: SupportedSpec) -> Element {
    let route_id = spec.route_id();
    let badge = spec.badge();
    let title = spec.title;
    // Custom NIPs (naddr-backed) live at the root URL; registry specs at /nips/.
    let target = if spec.naddr.is_some() {
        Route::AddressViewer { address: route_id }
    } else {
        Route::NipDetail { nip_id: route_id }
    };
    rsx! {
        Link {
            to: target,
            class: "block group",
            div { class: "bg-card rounded-lg border border-border p-4 hover:border-primary/50 transition-all duration-200 hover:shadow-md h-full",
                div { class: "flex items-center justify-between mb-2 gap-2",
                    span { class: "text-sm font-mono text-primary font-bold shrink-0",
                        "{badge}"
                    }
                }
                h3 { class: "text-base font-medium text-foreground group-hover:text-primary transition-colors line-clamp-2",
                    "{title}"
                }
            }
        }
    }
}
