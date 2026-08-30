use crate::components::MediaUploader;
use crate::stores::{auth_store, nostr_client, payto_targets_cache, profiles};
use crate::utils::nips::{nip39, nipa3::PayToTarget};
use dioxus::prelude::*;
use nostr::nips::nip39::Identity;
use nostr_sdk::Metadata;

/// One editable row in the payment-addresses section.
#[derive(Clone, Debug, PartialEq)]
struct PayToEditorRow {
    key: u32,
    /// Canonical type key, `custom`, a free-text type, or empty.
    payto_type: String,
    address: String,
}

#[derive(Props, Clone, PartialEq)]
pub struct ProfileEditorModalProps {
    /// Signal to control modal visibility
    pub show: Signal<bool>,
}
#[component]
pub fn ProfileEditorModal(mut props: ProfileEditorModalProps) -> Element {
    let mut name = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut about = use_signal(String::new);
    let mut picture = use_signal(String::new);
    let mut banner = use_signal(String::new);
    let mut website = use_signal(String::new);
    let mut nip05 = use_signal(String::new);
    let mut lud16 = use_signal(String::new);
    let mut is_bot = use_signal(|| false);
    let mut birthday_year = use_signal(|| None::<u16>);
    let mut birthday_month = use_signal(|| None::<u8>);
    let mut birthday_day = use_signal(|| None::<u8>);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut modal_session = use_signal(|| 0u64);
    let mut show_picture_uploader = use_signal(|| false);
    let mut show_banner_uploader = use_signal(|| false);
    let mut github_proof = use_signal(String::new);
    let mut twitter_proof = use_signal(String::new);
    let mut mastodon_proof = use_signal(String::new);
    let mut original_identities = use_signal(Vec::<Identity>::new);
    let mut payto_rows = use_signal(Vec::<PayToEditorRow>::new);
    let mut original_payto = use_signal(Vec::<PayToTarget>::new);
    let mut payto_row_seq = use_signal(|| 0u32);
    use_effect(use_reactive(&*props.show.read(), move |is_shown| {
        if is_shown {
            modal_session.with_mut(|s| *s = s.wrapping_add(1));
            let session = *modal_session.read();
            spawn(async move {
                if let Some(pubkey) = auth_store::get_pubkey() {
                    match profiles::fetch_profile(pubkey.clone()).await {
                        Ok(profile) => {
                            if *modal_session.read() != session {
                                return;
                            }
                            name.set(profile.name.unwrap_or_default());
                            display_name.set(profile.display_name.unwrap_or_default());
                            about.set(profile.about.unwrap_or_default());
                            picture.set(profile.picture.unwrap_or_default());
                            banner.set(profile.banner.unwrap_or_default());
                            website.set(profile.website.unwrap_or_default());
                            nip05.set(profile.nip05.unwrap_or_default());
                            lud16.set(profile.lud16.unwrap_or_default());
                            is_bot.set(profile.bot.unwrap_or(false));
                            if let Some(bday) = profile.birthday {
                                birthday_year.set(bday.year);
                                birthday_month.set(bday.month);
                                birthday_day.set(bday.day);
                            } else {
                                birthday_year.set(None);
                                birthday_month.set(None);
                                birthday_day.set(None);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to load profile for editing: {}", e);
                        }
                    }
                    if let Ok(identities) = nip39::fetch_external_identities(&pubkey).await {
                        if *modal_session.read() != session {
                            return;
                        }
                        let mut parsed_originals: Vec<Identity> = Vec::new();
                        for info in &identities {
                            match info.platform.as_str() {
                                "github" => {
                                    let url = info.proof_url();
                                    github_proof.set(url.clone());
                                    if let Some(id) = nip39::parse_github_proof_url(&url) {
                                        parsed_originals.push(id);
                                    }
                                }
                                "twitter" => {
                                    let url = info.proof_url();
                                    twitter_proof.set(url.clone());
                                    if let Some(id) = nip39::parse_twitter_proof_url(&url) {
                                        parsed_originals.push(id);
                                    }
                                }
                                "mastodon" => {
                                    let url = info.proof_url();
                                    mastodon_proof.set(url.clone());
                                    if let Some(id) = nip39::parse_mastodon_proof_url(&url) {
                                        parsed_originals.push(id);
                                    }
                                }
                                _ => {}
                            }
                        }
                        original_identities.set(parsed_originals);
                    }
                }
            });
            // Load payment targets (NIP-A3 kind 10133) in parallel; seeded
            // from cache and refreshed from the user's relays.
            spawn(async move {
                if let Some(pubkey) = auth_store::get_pubkey() {
                    payto_targets_cache::fetch_targets(pubkey.clone()).await;
                    if *modal_session.read() != session {
                        return;
                    }
                    let targets =
                        payto_targets_cache::peek_targets(&pubkey).unwrap_or_default();
                    payto_row_seq.set(targets.len() as u32);
                    payto_rows.set(
                        targets
                            .iter()
                            .enumerate()
                            .map(|(i, t)| PayToEditorRow {
                                key: i as u32,
                                payto_type: t.payto_type.clone(),
                                address: t.address.clone(),
                            })
                            .collect(),
                    );
                    original_payto.set(targets);
                }
            });
        }
    }));
    let handle_save = move |_| {
        saving.set(true);
        error.set(None);
        success.set(false);
        spawn(async move {
            let mut metadata = Metadata::new()
                .name(name.read().clone())
                .display_name(display_name.read().clone())
                .about(about.read().clone())
                .nip05(nip05.read().clone())
                .lud16(lud16.read().clone());
            if let Ok(url) = nostr_sdk::Url::parse(&picture.read().clone()) {
                metadata = metadata.picture(url);
            }
            if let Ok(url) = nostr_sdk::Url::parse(&banner.read().clone()) {
                metadata = metadata.banner(url);
            }
            if let Ok(url) = nostr_sdk::Url::parse(&website.read().clone()) {
                metadata = metadata.website(url);
            }
            if *is_bot.read() {
                metadata
                    .custom
                    .insert("bot".to_string(), serde_json::Value::Bool(true));
            }
            let year = *birthday_year.read();
            let month = *birthday_month.read();
            let day = *birthday_day.read();
            if year.is_some() || month.is_some() || day.is_some() {
                let mut birthday_obj = serde_json::Map::new();
                if let Some(y) = year {
                    birthday_obj.insert("year".to_string(), serde_json::Value::Number(y.into()));
                }
                if let Some(m) = month {
                    birthday_obj.insert("month".to_string(), serde_json::Value::Number(m.into()));
                }
                if let Some(d) = day {
                    birthday_obj.insert("day".to_string(), serde_json::Value::Number(d.into()));
                }
                metadata.custom.insert(
                    "birthday".to_string(),
                    serde_json::Value::Object(birthday_obj),
                );
            }
            match nostr_client::publish_metadata(metadata).await {
                Ok(_) => {
                    log::info!("Profile updated successfully");
                    let mut identity_errors: Vec<String> = Vec::new();
                    let mut identities_to_publish: Vec<Identity> = Vec::new();
                    if !github_proof.read().is_empty() {
                        match nip39::parse_github_proof_url(&github_proof.read()) {
                            Some(id) => identities_to_publish.push(id),
                            None => {
                                if github_proof.read().trim().starts_with("http") {
                                    identity_errors.push("Invalid GitHub proof URL".to_string());
                                }
                            }
                        }
                    }
                    if !twitter_proof.read().is_empty() {
                        match nip39::parse_twitter_proof_url(&twitter_proof.read()) {
                            Some(id) => identities_to_publish.push(id),
                            None => {
                                if twitter_proof.read().trim().starts_with("http") {
                                    identity_errors.push("Invalid Twitter proof URL".to_string());
                                }
                            }
                        }
                    }
                    if !mastodon_proof.read().is_empty() {
                        match nip39::parse_mastodon_proof_url(&mastodon_proof.read()) {
                            Some(id) => identities_to_publish.push(id),
                            None => {
                                if mastodon_proof.read().trim().starts_with("http") {
                                    identity_errors.push("Invalid Mastodon proof URL".to_string());
                                }
                            }
                        }
                    }
                    let mut current_sorted = identities_to_publish.clone();
                    current_sorted.sort();
                    let mut original_sorted = original_identities.read().clone();
                    original_sorted.sort();
                    if current_sorted != original_sorted {
                        if let Err(e) =
                            nip39::publish_external_identities(identities_to_publish).await
                        {
                            identity_errors.push(e);
                        }
                    }
                    // Payment targets (NIP-A3 kind 10133): validate rows,
                    // skip fully-empty ones, and publish on change.
                    let mut payto_targets: Vec<PayToTarget> = Vec::new();
                    for row in payto_rows.read().iter() {
                        let address = row.address.trim().to_string();
                        if address.is_empty() && row.payto_type.trim().is_empty() {
                            continue;
                        }
                        let payto_type = crate::utils::nips::nipa3::normalize_type(&row.payto_type);
                        if payto_type.is_empty() || payto_type == "custom" {
                            identity_errors.push("Payment method needs a type".to_string());
                            continue;
                        }
                        if address.is_empty() {
                            identity_errors.push(format!(
                                "Payment method {} needs an address",
                                crate::utils::nips::nipa3::method_for(&payto_type)
                                    .map(|m| m.label)
                                    .unwrap_or("entry")
                            ));
                            continue;
                        }
                        if let Some(method) =
                            crate::utils::nips::nipa3::method_for(&payto_type)
                        {
                            if !method.validate(&address) {
                                identity_errors.push(format!(
                                    "Invalid {} address",
                                    method.label
                                ));
                                continue;
                            }
                        }
                        let target = PayToTarget { payto_type, address };
                        if !payto_targets.contains(&target) {
                            payto_targets.push(target);
                        }
                    }
                    if identity_errors.is_empty() {
                        let mut current_sorted = payto_targets.clone();
                        current_sorted.sort();
                        let mut original_sorted = original_payto.read().clone();
                        original_sorted.sort();
                        if current_sorted != original_sorted {
                            match crate::utils::nips::nipa3::publish_payment_targets(
                                payto_targets.clone(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    if let Some(pubkey) = auth_store::get_pubkey() {
                                        payto_targets_cache::store_targets(
                                            pubkey,
                                            payto_targets,
                                        )
                                        .await;
                                    }
                                }
                                Err(e) => identity_errors.push(e),
                            }
                        }
                    }
                    if identity_errors.is_empty() {
                        success.set(true);
                    } else {
                        error.set(Some(identity_errors.join(", ")));
                    }
                    let session = *modal_session.read();
                    spawn(async move {
                        crate::platform::timer::sleep_ms(1500).await;
                        if *modal_session.read() != session || !*props.show.read() {
                            return;
                        }
                        props.show.set(false);
                        success.set(false);
                    });
                }
                Err(e) => {
                    log::error!("Failed to update profile: {}", e);
                    error.set(Some(e));
                }
            }
            saving.set(false);
        });
    };
    let handle_picture_uploaded = move |url: String| {
        picture.set(url);
        show_picture_uploader.set(false);
    };
    let add_payto_row = move |_| {
        let used: std::collections::HashSet<String> = payto_rows
            .read()
            .iter()
            .map(|r| r.payto_type.clone())
            .collect();
        let next_type = crate::utils::nips::nipa3::PAYMENT_METHODS
            .iter()
            .map(|m| m.type_key)
            .find(|key| !used.contains(*key))
            .map(|s| s.to_string())
            .unwrap_or_default();
        payto_row_seq.with_mut(|s| *s += 1);
        let key = *payto_row_seq.read();
        payto_rows.with_mut(|rows| {
            rows.push(PayToEditorRow {
                key,
                payto_type: next_type,
                address: String::new(),
            })
        });
    };
    let handle_banner_uploaded = move |url: String| {
        banner.set(url);
        show_banner_uploader.set(false);
    };
    let close_modal = move |_| {
        props.show.set(false);
        error.set(None);
        success.set(false);
        show_picture_uploader.set(false);
        show_banner_uploader.set(false);
    };
    if !*props.show.read() {
        return rsx! {
            div {}
        };
    }
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: close_modal,
            div {
                class: "bg-white dark:bg-gray-800 rounded-xl shadow-2xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 p-6 flex items-center justify-between z-10",
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white",
                        "✏️ Edit Profile"
                    }
                    button {
                        class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 text-2xl",
                        onclick: close_modal,
                        "✕"
                    }
                }
                div { class: "p-6 space-y-6",
                    div { class: "space-y-3",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Profile Picture"
                        }
                        if !picture.read().is_empty() {
                            div { class: "flex items-center gap-4",
                                img {
                                    class: "w-24 h-24 rounded-full object-cover",
                                    src: "{picture}",
                                    alt: "Profile picture",
                                    loading: "lazy",
                                }
                                button {
                                    class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400",
                                    onclick: move |_| {
                                        picture.set(String::new());
                                        show_picture_uploader.set(true);
                                    },
                                    "Remove"
                                }
                            }
                        }
                        if *show_picture_uploader.read() || picture.read().is_empty() {
                            MediaUploader {
                                accept: "image/*".to_string(),
                                on_upload: handle_picture_uploaded,
                                button_label: "Upload Profile Picture",
                            }
                        } else {
                            button {
                                class: "p-2 bg-muted text-foreground hover:bg-accent rounded-lg text-sm transition",
                                onclick: move |_| show_picture_uploader.set(true),
                                "Change Picture"
                            }
                        }
                    }
                    div { class: "space-y-3",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Banner Image"
                        }
                        if !banner.read().is_empty() {
                            div { class: "space-y-2",
                                img {
                                    class: "w-full h-32 rounded-lg object-cover",
                                    src: "{banner}",
                                    alt: "Banner",
                                    loading: "lazy",
                                }
                                button {
                                    class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400",
                                    onclick: move |_| {
                                        banner.set(String::new());
                                        show_banner_uploader.set(true);
                                    },
                                    "Remove"
                                }
                            }
                        }
                        if *show_banner_uploader.read() || banner.read().is_empty() {
                            MediaUploader {
                                accept: "image/*".to_string(),
                                on_upload: handle_banner_uploaded,
                                button_label: "Upload Banner",
                            }
                        } else {
                            button {
                                class: "p-2 bg-muted text-foreground hover:bg-accent rounded-lg text-sm transition",
                                onclick: move |_| show_banner_uploader.set(true),
                                "Change Banner"
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Name"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "Your name",
                            value: "{name}",
                            oninput: move |evt| name.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Display Name"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "Display name",
                            value: "{display_name}",
                            oninput: move |evt| display_name.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "About"
                        }
                        textarea {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white resize-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            rows: "4",
                            placeholder: "Tell us about yourself...",
                            value: "{about}",
                            oninput: move |evt| about.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Website"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "url",
                            placeholder: "https://example.com",
                            value: "{website}",
                            oninput: move |evt| website.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "NIP-05 Identifier"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "user@domain.com",
                            value: "{nip05}",
                            oninput: move |evt| nip05.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Lightning Address"
                        }
                        input {
                            class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "user@getalby.com",
                            value: "{lud16}",
                            oninput: move |evt| lud16.set(evt.value()),
                        }
                    }
                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Payment Addresses (NIP-A3)"
                        }
                        p { class: "text-xs text-gray-500 dark:text-gray-400 mb-2",
                            "Declare payment addresses for other networks (Bitcoin, Monero, Ethereum, Cash App, …) so others can pay you directly."
                        }
                        for row in payto_rows.read().iter() {
                            {
                                let row = row.clone();
                                let row_key = row.key;
                                let is_custom = crate::utils::nips::nipa3::method_for(&row.payto_type).is_none();
                                let placeholder = crate::utils::nips::nipa3::method_for(&row.payto_type)
                                    .map(|m| m.placeholder)
                                    .unwrap_or("Address");
                                rsx! {
                                    div { key: "{row_key}", class: "flex flex-wrap items-center gap-2",
                                        select {
                                            class: "w-36 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500",
                                            aria_label: "Payment method",
                                            onchange: {
                                                let key = row_key;
                                                move |evt| {
                                                    let val = evt.value();
                                                    payto_rows.with_mut(|rows| {
                                                        if let Some(r) = rows.iter_mut().find(|r| r.key == key) {
                                                            r.payto_type = val;
                                                        }
                                                    });
                                                }
                                            },
                                            for method in crate::utils::nips::nipa3::PAYMENT_METHODS.iter() {
                                                option {
                                                    value: "{method.type_key}",
                                                    selected: row.payto_type == method.type_key,
                                                    "{method.label}"
                                                }
                                            }
                                            option {
                                                value: "",
                                                selected: is_custom,
                                                "Custom…"
                                            }
                                        }
                                        if is_custom {
                                            input {
                                                class: "w-28 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500",
                                                r#type: "text",
                                                placeholder: "e.g. bitcoin",
                                                value: "{row.payto_type}",
                                                oninput: {
                                                    let key = row_key;
                                                    move |evt| {
                                                        let val = evt.value();
                                                        payto_rows.with_mut(|rows| {
                                                            if let Some(r) = rows.iter_mut().find(|r| r.key == key) {
                                                                r.payto_type = val;
                                                            }
                                                        });
                                                    }
                                                },
                                            }
                                        }
                                        input {
                                            class: "flex-1 min-w-40 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 font-mono text-xs",
                                            r#type: "text",
                                            placeholder: "{placeholder}",
                                            value: "{row.address}",
                                            oninput: {
                                                let key = row_key;
                                                move |evt| {
                                                    let val = evt.value();
                                                    payto_rows.with_mut(|rows| {
                                                        if let Some(r) = rows.iter_mut().find(|r| r.key == key) {
                                                            r.address = val;
                                                        }
                                                    });
                                                }
                                            },
                                        }
                                        button {
                                            class: "p-2 text-gray-400 hover:text-red-500 transition",
                                            r#type: "button",
                                            title: "Remove payment method",
                                            aria_label: "Remove payment method",
                                            onclick: {
                                                let key = row_key;
                                                move |_| {
                                                    payto_rows.with_mut(|rows| {
                                                        rows.retain(|r| r.key != key);
                                                    });
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            class: "px-3 py-1.5 text-sm border border-dashed border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-blue-500 hover:text-blue-500 rounded-lg transition",
                            r#type: "button",
                            onclick: add_payto_row,
                            "+ Add payment method"
                        }
                    }
                    div { class: "flex items-center justify-between",
                        div {
                            label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                                "Bot Account"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400",
                                "Mark this account as a bot or automated account"
                            }
                        }
                        button {
                            class: if *is_bot.read() { "relative inline-flex h-6 w-11 items-center rounded-full bg-blue-600 transition" } else { "relative inline-flex h-6 w-11 items-center rounded-full bg-gray-300 dark:bg-gray-600 transition" },
                            r#type: "button",
                            onclick: move |_| {
                                let current = *is_bot.read();
                                is_bot.set(!current);
                            },
                            span { class: if *is_bot.read() { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-6" } else { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-1" } }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                            "Birthday (optional)"
                        }
                        div { class: "flex gap-2",
                            select {
                                class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500",
                                onchange: move |evt| {
                                    let val = evt.value();
                                    birthday_month.set(val.parse::<u8>().ok());
                                },
                                option {
                                    value: "",
                                    selected: birthday_month.read().is_none(),
                                    "Month"
                                }
                                option {
                                    value: "1",
                                    selected: *birthday_month.read() == Some(1),
                                    "January"
                                }
                                option {
                                    value: "2",
                                    selected: *birthday_month.read() == Some(2),
                                    "February"
                                }
                                option {
                                    value: "3",
                                    selected: *birthday_month.read() == Some(3),
                                    "March"
                                }
                                option {
                                    value: "4",
                                    selected: *birthday_month.read() == Some(4),
                                    "April"
                                }
                                option {
                                    value: "5",
                                    selected: *birthday_month.read() == Some(5),
                                    "May"
                                }
                                option {
                                    value: "6",
                                    selected: *birthday_month.read() == Some(6),
                                    "June"
                                }
                                option {
                                    value: "7",
                                    selected: *birthday_month.read() == Some(7),
                                    "July"
                                }
                                option {
                                    value: "8",
                                    selected: *birthday_month.read() == Some(8),
                                    "August"
                                }
                                option {
                                    value: "9",
                                    selected: *birthday_month.read() == Some(9),
                                    "September"
                                }
                                option {
                                    value: "10",
                                    selected: *birthday_month.read() == Some(10),
                                    "October"
                                }
                                option {
                                    value: "11",
                                    selected: *birthday_month.read() == Some(11),
                                    "November"
                                }
                                option {
                                    value: "12",
                                    selected: *birthday_month.read() == Some(12),
                                    "December"
                                }
                            }
                            select {
                                class: "w-20 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500",
                                onchange: move |evt| {
                                    let val = evt.value();
                                    birthday_day.set(val.parse::<u8>().ok());
                                },
                                option {
                                    value: "",
                                    selected: birthday_day.read().is_none(),
                                    "Day"
                                }
                                {(1..=31).map(|d| rsx! {
                                    option { value: "{d}", selected: *birthday_day.read() == Some(d), "{d}" }
                                })}
                            }
                            select {
                                class: "w-24 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500",
                                onchange: move |evt| {
                                    let val = evt.value();
                                    birthday_year.set(val.parse::<u16>().ok());
                                },
                                option {
                                    value: "",
                                    selected: birthday_year.read().is_none(),
                                    "Year"
                                }
                                {(1920..=2024).rev().map(|y| rsx! {
                                    option { value: "{y}", selected: *birthday_year.read() == Some(y), "{y}" }
                                })}
                            }
                        }
                    }
                    div { class: "space-y-3",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                            "External Identities"
                        }
                        p { class: "text-xs text-gray-500 dark:text-gray-400",
                            "Link your external accounts to verify your identity"
                        }
                        div {
                            label { class: "block text-xs text-gray-600 dark:text-gray-400 mb-1",
                                "GitHub Proof URL"
                            }
                            input {
                                class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "url",
                                placeholder: "https://gist.github.com/<user>/<gist>",
                                value: "{github_proof}",
                                oninput: move |evt| github_proof.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-xs text-gray-600 dark:text-gray-400 mb-1",
                                "X (Twitter) Proof URL"
                            }
                            input {
                                class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "url",
                                placeholder: "https://x.com/<user>/status/<proof>",
                                value: "{twitter_proof}",
                                oninput: move |evt| twitter_proof.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-xs text-gray-600 dark:text-gray-400 mb-1",
                                "Mastodon Proof URL"
                            }
                            input {
                                class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "url",
                                placeholder: "https://<server>/@<user>/<post>",
                                value: "{mastodon_proof}",
                                oninput: move |evt| mastodon_proof.set(evt.value()),
                            }
                        }
                    }
                    if let Some(err) = error.read().as_ref() {
                        div { class: "p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                    if *success.read() {
                        div { class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg",
                            "✅ Profile updated successfully!"
                        }
                    }
                }
                div { class: "sticky bottom-0 bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 p-6 flex gap-3 justify-end",
                    button {
                        class: "px-6 py-2 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium hover:bg-gray-50 dark:hover:bg-gray-700 transition",
                        onclick: close_modal,
                        disabled: *saving.read(),
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-3 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-lg font-medium transition",
                        disabled: *saving.read(),
                        onclick: handle_save,
                        if *saving.read() {
                            "Saving..."
                        } else {
                            "Save Profile"
                        }
                    }
                }
            }
        }
    }
}
