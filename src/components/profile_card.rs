use dioxus::prelude::*;

#[component]
pub fn ProfileCard() -> Element {
    rsx! {
        div {
            class: "profile-card-placeholder",
            "Profile Card placeholder"
        }
    }
}
