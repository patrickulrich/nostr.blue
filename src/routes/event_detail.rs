//! Event Detail Page
//!
//! Display detailed view of a calendar event or live activity

use dioxus::prelude::*;

use crate::routes::Route;
use crate::stores::{nostr_client, calendar_store, profiles, auth_store};
use crate::stores::calendar_store::UnifiedEvent;
use crate::utils::nip52::{RsvpStatus, is_online_location};
use crate::utils::nip53::{RoomPresence, LiveActivityEvent};
use crate::utils::ics::{export_event_to_ics, download_ics};
use crate::components::ClientInitializing;
use crate::utils::truncate_pubkey;

#[component]
pub fn CalendarEventDetail(naddr: String, from: Option<String>) -> Element {
    // State
    let mut event = use_signal(|| None::<UnifiedEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut rsvp_status = use_signal(|| None::<RsvpStatus>);
    let mut rsvp_loading = use_signal(|| false);
    let mut rsvp_count = use_signal(|| 0usize);
    let mut room_presence = use_signal(Vec::<RoomPresence>::new);
    let mut parent_space_url = use_signal(|| None::<String>);

    // Determine back route based on where we came from
    let back_route = match from.as_deref() {
        Some("events") => Route::Events {},
        _ => Route::Calendar {}, // Default to calendar
    };

    // Load event on mount
    use_effect(move || {
        let naddr_clone = naddr.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        log::info!("[CalendarEventDetail] Effect triggered, client_initialized={}, naddr={}", client_initialized, naddr_clone);

        if !client_initialized {
            log::info!("[CalendarEventDetail] Client not initialized, waiting...");
            return;
        }

        spawn(async move {
            log::info!("[CalendarEventDetail] Starting fetch for naddr: {}", naddr_clone);
            loading.set(true);
            error.set(None);

            // Fetch unified event (handles both calendar events and meetings)
            match calendar_store::fetch_unified_event_by_naddr(&naddr_clone).await {
                Ok(Some(unified_event)) => {
                    log::info!("[CalendarEventDetail] Successfully fetched event: {}", unified_event.title());
                    let coord = unified_event.coordinate().to_string();
                    event.set(Some(unified_event.clone()));

                    // Load RSVPs (only for calendar events)
                    if unified_event.is_calendar_event() {
                        if let Ok(rsvps) = calendar_store::fetch_event_rsvps(&coord).await {
                            rsvp_count.set(rsvps.len());
                        }

                        // Check user's RSVP
                        if let Some(my_rsvp) = calendar_store::get_my_rsvp(&coord) {
                            rsvp_status.set(Some(my_rsvp.status));
                        }
                    } else {
                        // For meetings, fetch room presence (users in the last 5 minutes)
                        if let Ok(presence) = calendar_store::fetch_room_presence(&coord, 300).await {
                            log::info!("[CalendarEventDetail] Found {} users present", presence.len());
                            room_presence.set(presence);
                        }

                        // For MeetingRoom (30313), fetch parent space to get service_url
                        if let UnifiedEvent::Live(LiveActivityEvent::Meeting(ref meeting)) = unified_event {
                            if let Some(ref space_coord) = meeting.space_coordinate {
                                log::info!("[CalendarEventDetail] Fetching parent space: {}", space_coord);
                                // Convert coordinate to naddr format for fetching
                                let parts: Vec<&str> = space_coord.split(':').collect();
                                if parts.len() >= 3 {
                                    // Build naddr from coordinate parts using nostr prelude
                                    use nostr::prelude::*;
                                    if let Ok(pk) = PublicKey::from_hex(parts[1]) {
                                        let coordinate = Coordinate::new(Kind::Custom(30312), pk).identifier(parts[2]);
                                        let nip19 = Nip19Coordinate::new(coordinate, vec![]);
                                        if let Ok(naddr_str) = nip19.to_bech32() {
                                            if let Ok(Some(UnifiedEvent::Live(LiveActivityEvent::Space(space)))) =
                                                calendar_store::fetch_unified_event_by_naddr(&naddr_str).await {
                                                log::info!("[CalendarEventDetail] Found parent space service_url: {}", space.service_url);
                                                parent_space_url.set(Some(space.service_url));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    log::warn!("[CalendarEventDetail] Event not found for naddr: {}", naddr_clone);
                    error.set(Some("Event not found".to_string()));
                }
                Err(e) => {
                    log::error!("[CalendarEventDetail] Fetch error: {}", e);
                    error.set(Some(e));
                }
            }

            loading.set(false);
        });
    });

    // Handle RSVP
    let handle_rsvp = move |status: RsvpStatus| {
        let Some(evt) = event.read().as_ref().cloned() else { return };

        spawn(async move {
            rsvp_loading.set(true);

            let (coord, author) = match &evt {
                UnifiedEvent::Calendar(e) => (e.coordinate.clone(), e.pubkey.clone()),
                UnifiedEvent::Live(_) => {
                    rsvp_loading.set(false);
                    return;
                }
            };

            match calendar_store::publish_rsvp(&coord, &author, status, "").await {
                Ok(_) => {
                    rsvp_status.set(Some(status));
                    // Update count
                    let current = *rsvp_count.read();
                    if rsvp_status.read().is_none() {
                        rsvp_count.set(current + 1);
                    }
                }
                Err(e) => {
                    log::error!("Failed to publish RSVP: {}", e);
                }
            }

            rsvp_loading.set(false);
        });
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: back_route.clone(),
                        class: "p-2 -ml-2 hover:bg-accent rounded-lg transition",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M10 19l-7-7m0 0l7-7m-7 7h18"
                            }
                        }
                    }
                    h1 {
                        class: "text-lg font-bold",
                        "Event Details"
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if *loading.read() {
                // Loading skeleton
                div {
                    class: "animate-pulse",
                    // Image placeholder
                    div { class: "h-48 bg-muted" }
                    div {
                        class: "p-4",
                        div { class: "h-8 bg-muted rounded w-3/4 mb-4" }
                        div { class: "h-4 bg-muted rounded w-1/2 mb-2" }
                        div { class: "h-4 bg-muted rounded w-1/3 mb-4" }
                        div { class: "h-20 bg-muted rounded mb-4" }
                        div { class: "h-12 bg-muted rounded" }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                // Error state
                div {
                    class: "p-8 text-center",
                    div {
                        class: "text-4xl mb-4",
                        "❌"
                    }
                    h3 {
                        class: "text-lg font-medium mb-2",
                        "Event not found"
                    }
                    p {
                        class: "text-muted-foreground mb-4",
                        "{err}"
                    }
                    Link {
                        to: back_route.clone(),
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg inline-block",
                        if from.as_deref() == Some("events") {
                            "Back to Events"
                        } else {
                            "Back to Calendar"
                        }
                    }
                }
            } else if let Some(evt) = event.read().as_ref() {
                // Event content
                div {
                    // Event image
                    if let Some(image_url) = evt.image() {
                        div {
                            class: "relative h-48 sm:h-64 overflow-hidden",
                            img {
                                src: "{image_url}",
                                alt: "{evt.title()}",
                                class: "w-full h-full object-cover",
                            }

                            // Live badge
                            if evt.is_live() {
                                div {
                                    class: "absolute top-4 left-4 px-3 py-1.5 bg-red-500 text-white font-bold rounded animate-pulse",
                                    "LIVE NOW"
                                }
                            }

                            // Private badge
                            if evt.is_private() {
                                div {
                                    class: "absolute top-4 right-4 px-3 py-1.5 bg-purple-600 text-white rounded flex items-center gap-2",
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                                        }
                                    }
                                    "Private Event"
                                }
                            }
                        }
                    }

                    // Event details
                    div {
                        class: "p-4",

                        // Title
                        h1 {
                            class: "text-2xl font-bold mb-4",
                            "{evt.title()}"
                        }

                        // Time
                        div {
                            class: "flex items-center gap-3 text-muted-foreground mb-3",
                            svg {
                                class: "w-5 h-5 flex-shrink-0",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
                                }
                            }
                            div {
                                div {
                                    class: "font-medium text-foreground",
                                    "{format_event_datetime(evt)}"
                                }
                                if evt.is_all_day() {
                                    span {
                                        class: "text-sm",
                                        "All day event"
                                    }
                                }
                            }
                        }

                        // Location(s) - for calendar events
                        if !evt.locations().is_empty() {
                            div {
                                class: "mb-4",
                                for location in evt.locations().iter() {
                                    div {
                                        class: "flex items-center gap-3 text-muted-foreground mb-2",
                                        if is_online_location(location) {
                                            svg {
                                                class: "w-5 h-5 flex-shrink-0 text-blue-500",
                                                xmlns: "http://www.w3.org/2000/svg",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                                }
                                            }
                                        } else {
                                            svg {
                                                class: "w-5 h-5 flex-shrink-0 text-green-500",
                                                xmlns: "http://www.w3.org/2000/svg",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z"
                                                }
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M15 11a3 3 0 11-6 0 3 3 0 016 0z"
                                                }
                                            }
                                        }
                                        if location.starts_with("http") {
                                            a {
                                                href: "{location}",
                                                target: "_blank",
                                                rel: "noopener noreferrer",
                                                class: "text-primary hover:underline",
                                                "{location}"
                                            }
                                        } else {
                                            span {
                                                "{location}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Join URL - for meetings (direct URL or parent space URL)
                        {
                            let effective_join_url = evt.join_url()
                                .map(|s| s.to_string())
                                .or_else(|| parent_space_url.read().clone());

                            if let Some(join_url) = effective_join_url {
                                rsx! {
                                    div {
                                        class: "mb-4",
                                        a {
                                            href: "{join_url}",
                                            target: "_blank",
                                            rel: "noopener noreferrer",
                                            class: "w-full flex items-center justify-center gap-2 px-4 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition font-medium",
                                            svg {
                                                class: "w-5 h-5",
                                                xmlns: "http://www.w3.org/2000/svg",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                                }
                                            }
                                            "Join Meeting"
                                        }
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }

                        // RSVP count (for calendar events)
                        if *rsvp_count.read() > 0 {
                            div {
                                class: "flex items-center gap-2 text-muted-foreground mb-4",
                                svg {
                                    class: "w-5 h-5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
                                    }
                                }
                                span {
                                    "{rsvp_count} attending"
                                }
                            }
                        }

                        // Room presence (for meetings)
                        if !room_presence.read().is_empty() {
                            div {
                                class: "mb-4",
                                div {
                                    class: "flex items-center gap-2 text-muted-foreground mb-2",
                                    svg {
                                        class: "w-5 h-5 text-green-500",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M5.636 18.364a9 9 0 010-12.728m12.728 0a9 9 0 010 12.728m-9.9-2.829a5 5 0 010-7.07m7.072 0a5 5 0 010 7.07M13 12a1 1 0 11-2 0 1 1 0 012 0z"
                                        }
                                    }
                                    span {
                                        class: "text-sm font-medium",
                                        "{room_presence.read().len()} in room now"
                                    }
                                }
                                // Avatar stack of present users
                                div {
                                    class: "flex flex-wrap gap-2",
                                    for presence in room_presence.read().iter().take(10) {
                                        PresenceAvatar {
                                            key: "{presence.pubkey}",
                                            pubkey: presence.pubkey.clone(),
                                            hand_raised: presence.hand_raised
                                        }
                                    }
                                    if room_presence.read().len() > 10 {
                                        div {
                                            class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-xs font-medium",
                                            "+{room_presence.read().len() - 10}"
                                        }
                                    }
                                }
                            }
                        }

                        // Hashtags
                        if !evt.hashtags().is_empty() {
                            div {
                                class: "flex flex-wrap gap-2 mb-4",
                                for tag in evt.hashtags() {
                                    Link {
                                        to: Route::Events {},
                                        class: "px-3 py-1 text-sm bg-muted text-muted-foreground hover:bg-accent rounded-full transition",
                                        "#{tag}"
                                    }
                                }
                            }
                        }

                        // Divider
                        hr { class: "my-4 border-border" }

                        // Description/Summary
                        if let Some(summary) = evt.summary() {
                            if !summary.is_empty() {
                                div {
                                    class: "mb-4",
                                    h3 {
                                        class: "text-sm font-medium text-muted-foreground mb-2",
                                        "About this event"
                                    }
                                    p {
                                        class: "text-foreground whitespace-pre-wrap",
                                        "{summary}"
                                    }
                                }
                            }
                        }

                        // Full content/details
                        {
                            let content = evt.content();
                            if !content.is_empty() {
                                rsx! {
                                    div {
                                        class: "mb-4",
                                        h3 {
                                            class: "text-sm font-medium text-muted-foreground mb-2",
                                            "Details"
                                        }
                                        div {
                                            class: "prose prose-sm dark:prose-invert max-w-none",
                                            dangerous_inner_html: "{content}"
                                        }
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }

                        // Organizer/Author
                        div {
                            class: "mb-6",
                            h3 {
                                class: "text-sm font-medium text-muted-foreground mb-2",
                                "Organized by"
                            }
                            OrganizerCard { pubkey: evt.pubkey().to_string() }
                        }

                        // RSVP buttons (for calendar events, if logged in)
                        if let UnifiedEvent::Calendar(_) = evt {
                            if auth_store::get_pubkey().is_some() {
                                div {
                                    class: "border-t border-border pt-4",
                                    h3 {
                                        class: "text-sm font-medium text-muted-foreground mb-3",
                                        "Your response"
                                    }
                                    div {
                                        class: "flex gap-2",

                                        // Going button
                                        button {
                                            class: if *rsvp_status.read() == Some(RsvpStatus::Accepted) {
                                                "flex-1 py-2 px-4 bg-green-600 text-white rounded-lg font-medium"
                                            } else {
                                                "flex-1 py-2 px-4 bg-muted hover:bg-green-600/20 text-foreground rounded-lg font-medium transition"
                                            },
                                            disabled: *rsvp_loading.read(),
                                            onclick: move |_| handle_rsvp(RsvpStatus::Accepted),
                                            if *rsvp_loading.read() {
                                                "..."
                                            } else {
                                                "Going"
                                            }
                                        }

                                        // Maybe button
                                        button {
                                            class: if *rsvp_status.read() == Some(RsvpStatus::Tentative) {
                                                "flex-1 py-2 px-4 bg-yellow-600 text-white rounded-lg font-medium"
                                            } else {
                                                "flex-1 py-2 px-4 bg-muted hover:bg-yellow-600/20 text-foreground rounded-lg font-medium transition"
                                            },
                                            disabled: *rsvp_loading.read(),
                                            onclick: move |_| handle_rsvp(RsvpStatus::Tentative),
                                            if *rsvp_loading.read() {
                                                "..."
                                            } else {
                                                "Maybe"
                                            }
                                        }

                                        // Not going button
                                        button {
                                            class: if *rsvp_status.read() == Some(RsvpStatus::Declined) {
                                                "flex-1 py-2 px-4 bg-red-600 text-white rounded-lg font-medium"
                                            } else {
                                                "flex-1 py-2 px-4 bg-muted hover:bg-red-600/20 text-foreground rounded-lg font-medium transition"
                                            },
                                            disabled: *rsvp_loading.read(),
                                            onclick: move |_| handle_rsvp(RsvpStatus::Declined),
                                            if *rsvp_loading.read() {
                                                "..."
                                            } else {
                                                "Can't go"
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Not logged in
                                div {
                                    class: "border-t border-border pt-4 text-center",
                                    p {
                                        class: "text-muted-foreground mb-3",
                                        "Sign in to RSVP to this event"
                                    }
                                    Link {
                                        to: Route::Settings {},
                                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg inline-block",
                                        "Sign In"
                                    }
                                }
                            }
                        }

                        // Export to Calendar button (available to all users)
                        if let UnifiedEvent::Calendar(ref cal_event) = evt {
                            div {
                                class: "border-t border-border pt-4 mt-4",
                                button {
                                    class: "w-full flex items-center justify-center gap-2 px-4 py-2 text-sm rounded-lg border border-border hover:bg-accent transition",
                                    onclick: {
                                        let cal_event = cal_event.clone();
                                        move |_| {
                                            let ics_content = export_event_to_ics(&cal_event);
                                            let filename = format!("{}.ics", cal_event.title.replace(" ", "_").replace("/", "-"));
                                            download_ics(&filename, &ics_content);
                                        }
                                    },
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3"
                                        }
                                    }
                                    "Export to Calendar"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Organizer card component
#[component]
fn OrganizerCard(pubkey: String) -> Element {
    // Get profile from cache
    let profile = profiles::get_cached_profile(&pubkey);

    let display_name = profile
        .as_ref()
        .and_then(|p| p.display_name.as_deref().or(p.name.as_deref()))
        .unwrap_or("Anonymous");

    let avatar = profile.as_ref().and_then(|p| p.picture.as_deref());

    let npub = nostr::PublicKey::from_hex(&pubkey)
        .ok()
        .and_then(|pk| nostr::nips::nip19::ToBech32::to_bech32(&pk).ok())
        .map(|s| format!("{}...{}", &s[..12], &s[s.len()-8..]))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));

    rsx! {
        Link {
            to: Route::Profile { pubkey: pubkey.clone() },
            class: "flex items-center gap-3 p-3 bg-muted rounded-lg hover:bg-accent transition",

            // Avatar
            if let Some(url) = avatar {
                img {
                    src: "{url}",
                    alt: "{display_name}",
                    class: "w-10 h-10 rounded-full object-cover",
                }
            } else {
                div {
                    class: "w-10 h-10 rounded-full bg-primary/20 flex items-center justify-center",
                    span {
                        class: "text-lg font-bold text-primary",
                        "{display_name.chars().next().unwrap_or('?')}"
                    }
                }
            }

            // Name and npub
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium truncate",
                    "{display_name}"
                }
                div {
                    class: "text-sm text-muted-foreground truncate",
                    "{npub}"
                }
            }

            // Arrow
            svg {
                class: "w-5 h-5 text-muted-foreground",
                xmlns: "http://www.w3.org/2000/svg",
                fill: "none",
                view_box: "0 0 24 24",
                stroke: "currentColor",
                stroke_width: "2",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    d: "M9 5l7 7-7 7"
                }
            }
        }
    }
}

/// Format event datetime for display
fn format_event_datetime(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return "Date TBD".to_string();
    }

    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());

    let month_names = ["January", "February", "March", "April", "May", "June",
                       "July", "August", "September", "October", "November", "December"];
    let weekday_names = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

    let weekday = date.get_day() as usize;
    let month = date.get_month() as usize;
    let day = date.get_date();
    let year = date.get_full_year();
    let weekday_str = weekday_names.get(weekday).unwrap_or(&"");
    let month_str = month_names.get(month).unwrap_or(&"");

    if event.is_all_day() {
        format!("{}, {} {}, {}", weekday_str, month_str, day, year)
    } else {
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 { 12 } else if hours > 12 { hours - 12 } else { hours };

        format!("{}, {} {}, {} at {}:{:02} {}",
                weekday_str, month_str, day, year, hour_12, minutes, am_pm)
    }
}

/// Presence avatar component for room presence display
#[component]
fn PresenceAvatar(pubkey: String, hand_raised: bool) -> Element {
    let profile = profiles::get_cached_profile(&pubkey);

    let display_name = profile
        .as_ref()
        .and_then(|p| p.display_name.as_deref().or(p.name.as_deref()))
        .unwrap_or("?");

    let avatar = profile.as_ref().and_then(|p| p.picture.as_deref());

    rsx! {
        Link {
            to: Route::Profile { pubkey: pubkey.clone() },
            class: "relative group",
            title: "{display_name}",

            // Avatar
            if let Some(url) = avatar {
                img {
                    src: "{url}",
                    alt: "{display_name}",
                    class: "w-8 h-8 rounded-full object-cover ring-2 ring-green-500/50",
                }
            } else {
                div {
                    class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center ring-2 ring-green-500/50",
                    span {
                        class: "text-xs font-bold text-primary",
                        "{display_name.chars().next().unwrap_or('?')}"
                    }
                }
            }

            // Hand raised indicator
            if hand_raised {
                div {
                    class: "absolute -top-1 -right-1 w-4 h-4 bg-yellow-500 rounded-full flex items-center justify-center text-xs",
                    title: "Hand raised",
                    "✋"
                }
            }

            // Online indicator dot
            div {
                class: "absolute bottom-0 right-0 w-2.5 h-2.5 bg-green-500 rounded-full border-2 border-background"
            }
        }
    }
}
