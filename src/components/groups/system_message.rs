use crate::stores::profiles;
use crate::stores::social::group_store::{GroupMessage, SystemMessageType};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[component]
pub fn GroupSystemMessage(message: GroupMessage) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let author_pk = message.author.clone();
    let author_pk_for_effect = author_pk.clone();

    {
        let pk = author_pk_for_effect.clone();
        use_effect(move || {
            let pk = pk.clone();
            spawn(async move {
                if let Ok(p) = profiles::fetch_profile(pk).await {
                    profile.set(Some(p));
                }
            });
        });
    }

    let resolve_name = |pk: &str, prof: &Signal<Option<profiles::Profile>>| -> String {
        prof.read()
            .as_ref()
            .map(|p| p.get_display_name())
            .unwrap_or_else(|| truncate_pubkey(pk))
    };

    let text = match message.system_type.as_ref() {
        Some(SystemMessageType::UserJoined { pubkey }) => {
            let name = resolve_name(pubkey, &profile);
            format!("{} joined the group", name)
        }
        Some(SystemMessageType::UserLeft { pubkey }) => {
            let name = resolve_name(pubkey, &profile);
            format!("{} left the group", name)
        }
        Some(SystemMessageType::StatusChanged { by, details }) => {
            let name = resolve_name(by, &profile);
            format!("{} changed group status: {}", name, details)
        }
        Some(SystemMessageType::MessageDeleted { by }) => {
            let name = resolve_name(by, &profile);
            format!("{} deleted a message", name)
        }
        Some(SystemMessageType::GroupDeleted { by }) => {
            let name = resolve_name(by, &profile);
            format!("{} deleted the group", name)
        }
        _ => return rsx! {},
    };

    rsx! {
        div {
            class: "flex items-center justify-center py-2 px-4",
            div {
                class: "text-xs text-muted-foreground bg-muted/50 px-3 py-1 rounded-full",
                "{text}"
            }
        }
    }
}
