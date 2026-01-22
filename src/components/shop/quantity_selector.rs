//! QuantitySelector component - increment/decrement quantity input

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct QuantitySelectorProps {
    pub quantity: u32,
    pub on_change: EventHandler<u32>,
    #[props(default = 1)]
    pub min: u32,
    #[props(default = 99)]
    pub max: u32,
}

/// Quantity selector with +/- buttons
#[component]
pub fn QuantitySelector(props: QuantitySelectorProps) -> Element {
    let can_decrease = props.quantity > props.min;
    let can_increase = props.quantity < props.max;

    rsx! {
        div { class: "flex items-center gap-2",
            button {
                r#type: "button",
                class: "w-8 h-8 rounded-full border border-border flex items-center justify-center hover:bg-accent transition disabled:opacity-50 disabled:cursor-not-allowed",
                disabled: !can_decrease,
                onclick: {
                    let current = props.quantity;
                    let min = props.min;
                    let on_change = props.on_change;
                    move |_| {
                        if current > min {
                            on_change.call(current - 1);
                        }
                    }
                },
                "-"
            }

            span { class: "w-10 text-center font-medium",
                "{props.quantity}"
            }

            button {
                r#type: "button",
                class: "w-8 h-8 rounded-full border border-border flex items-center justify-center hover:bg-accent transition disabled:opacity-50 disabled:cursor-not-allowed",
                disabled: !can_increase,
                onclick: {
                    let current = props.quantity;
                    let max = props.max;
                    let on_change = props.on_change;
                    move |_| {
                        if current < max {
                            on_change.call(current + 1);
                        }
                    }
                },
                "+"
            }
        }
    }
}
