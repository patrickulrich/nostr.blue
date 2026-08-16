use crate::components::ClientInitializing;
use crate::components::icons::{
    ArrowLeftIcon, ClockIcon, ExternalLinkIcon, GlobeIcon, MapPinIcon, ShareIcon,
};
use crate::routes::Route;
use crate::services::places::{self, OsmEnrichment, Place};
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::stores::profiles;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::validation::is_valid_http_url;
use crate::utils::nip19::parse_naddr;
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

fn merge_osm_enrichment(place: &mut Place, enrichment: &OsmEnrichment) {
    if place.phone.is_none() {
        place.phone = enrichment.phone.clone();
    }
    if place.website.is_none() {
        place.website = enrichment.website.clone();
    }
    if place.opening_hours.is_none() {
        place.opening_hours = enrichment.opening_hours.clone();
    }
    if place.amenity.is_none() {
        place.amenity = enrichment.amenity.clone();
    }
    let addr = place.address.get_or_insert(places::PlaceAddress {
        street: None,
        city: None,
        state: None,
        postcode: None,
        country: None,
    });
    if addr.street.is_none() {
        addr.street = enrichment.street.clone();
    }
    if addr.city.is_none() {
        addr.city = enrichment.city.clone();
    }
    if addr.state.is_none() {
        addr.state = enrichment.state.clone();
    }
    if addr.postcode.is_none() {
        addr.postcode = enrichment.postcode.clone();
    }
    if addr.country.is_none() {
        addr.country = enrichment.country.clone();
    }
}

#[component]
pub fn PlaceViewer(naddr: String) -> Element {
    let mut place = use_signal(|| None::<Place>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let navigator = use_navigator();

    use_effect(use_reactive(&naddr, move |addr| {
        let client_ready = *CLIENT_INITIALIZED.read();
        if !client_ready {
            return;
        }
        let parsed_val = match parse_naddr(&addr) {
            Ok(p) => p,
            Err(_) => {
                error.set(Some("Invalid address".to_string()));
                loading.set(false);
                return;
            }
        };
        spawn(async move {
            loading.set(true);
            error.set(None);
            match nostr_client::fetch_event_by_coordinate_with_relays(
                parsed_val.kind,
                parsed_val.pubkey.clone(),
                parsed_val.identifier,
                parsed_val.relay_hints,
            )
            .await
            {
                Ok(Some(event)) => {
                    if let Some(mut pl) = places::parse_place(&event) {
                        let pk = pl.pubkey.clone();
                        let is_sparse = pl.address.is_none()
                            && pl.phone.is_none()
                            && pl.website.is_none()
                            && pl.opening_hours.is_none();
                        if let Some(ref osm_ref) = pl.osm_ref {
                            if is_sparse {
                                if let Some(enrichment) =
                                    places::fetch_osm_enrichment(osm_ref).await
                                {
                                    merge_osm_enrichment(&mut pl, &enrichment);
                                }
                            }
                        }
                        place.set(Some(pl));
                        let _ = profiles::fetch_profile(pk).await;
                    } else {
                        error.set(Some("Failed to parse place".to_string()));
                    }
                }
                Ok(None) => {
                    error.set(Some("Place not found".to_string()));
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));

    let creator_profile = use_memo(move || {
        place
            .read()
            .as_ref()
            .and_then(|p| profiles::get_profile(&p.pubkey))
    });

    let creator_name = use_memo(move || {
        creator_profile
            .read()
            .as_ref()
            .and_then(crate::stores::profiles::display_name_or_name)
            .unwrap_or_else(|| {
                place
                    .read()
                    .as_ref()
                    .map(|p| truncate_pubkey(&p.pubkey))
                    .unwrap_or_default()
            })
    });

    if !*CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    let place_data = place.read().clone();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center gap-4 px-4 py-3",
                    button {
                        class: "p-2 rounded-full hover:bg-accent transition",
                        onclick: move |_| {
                            navigator.go_back();
                        },
                        ArrowLeftIcon { class: "w-5 h-5".to_string() }
                    }
                    h1 { class: "text-xl font-bold", "Place" }
                }
            }

            if *loading.read() {
                div { class: "p-4 space-y-4",
                    div { class: "flex justify-center py-8",
                        div { class: "w-16 h-16 rounded-xl bg-muted animate-pulse" }
                    }
                    div { class: "h-8 bg-muted rounded w-1/2 mx-auto animate-pulse" }
                    div { class: "h-4 bg-muted rounded w-3/4 mx-auto animate-pulse" }
                }
            } else if let Some(e) = error.read().as_ref() {
                div { class: "flex flex-col items-center justify-center py-16 text-center",
                    p { class: "text-6xl mb-4", "📍" }
                    p { class: "text-muted-foreground", "Failed to load place" }
                    p { class: "text-sm text-destructive mt-2", "{e}" }
                }
            } else if let Some(p) = place_data {
                PlaceContent {
                    place: p,
                    creator_name: creator_name.read().clone(),
                    naddr: naddr.clone(),
                }
            }
        }
    }
}

#[component]
fn PlaceContent(place: Place, creator_name: String, naddr: String) -> Element {
    let mut show_share_toast = use_signal(|| false);

    let amenity_label = place.amenity.as_deref().map(|a| {
        let mut c = a.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    });

    let address_parts: Vec<String> = [
        place.address.as_ref().and_then(|a| a.street.clone()),
        place.address.as_ref().and_then(|a| a.city.clone()),
        place.address.as_ref().and_then(|a| a.state.clone()),
        place.address.as_ref().and_then(|a| a.postcode.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let address_str = address_parts.join(", ");

    let directions_url = format!(
        "https://www.openstreetmap.org/directions?from=&to={},{}",
        place.coordinates[1], place.coordinates[0]
    );

    rsx! {
        div { class: "max-w-2xl mx-auto p-4 space-y-6",

            div { class: "text-center space-y-3",
                h2 { class: "text-2xl font-bold", "{place.name}" }
                if let Some(amenity) = &amenity_label {
                    span { class: "inline-block px-3 py-1 text-sm rounded-full bg-purple-600/20 text-purple-400 border border-purple-600/30",
                        "{amenity}"
                    }
                }
                if let Some(desc) = &place.description {
                    if !desc.is_empty() {
                        p { class: "text-muted-foreground mt-2", "{desc}" }
                    }
                }
            }

            div { class: "bg-card border border-border rounded-xl overflow-hidden",
                div { class: "p-4 space-y-3",
                    div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                        MapPinIcon { class: "w-4 h-4".to_string() }
                        if !address_str.is_empty() {
                            span { "{address_str}" }
                        } else {
                            span { "{place.coordinates[0]:.4}, {place.coordinates[1]:.4}" }
                        }
                    }
                }
            }

            div { class: "bg-card border border-border rounded-xl divide-y divide-border",
                if let Some(hours) = &place.opening_hours {
                    div { class: "flex items-center gap-3 p-4",
                        ClockIcon { class: "w-5 h-5".to_string() }
                        div { class: "flex-1 min-w-0",
                            p { class: "text-sm text-muted-foreground", "Hours" }
                            p { class: "text-sm truncate", "{hours}" }
                        }
                    }
                }
                if let Some(phone) = &place.phone {
                    a {
                        href: "tel:{phone}",
                        class: "flex items-center gap-3 p-4 hover:bg-accent transition",
                        span { class: "text-xl", "📞" }
                        div { class: "flex-1 min-w-0",
                            p { class: "text-sm text-muted-foreground", "Phone" }
                            p { class: "text-sm truncate", "{phone}" }
                        }
                    }
                }
                if let Some(website) = place.website.as_ref().filter(|w| is_valid_http_url(w)) {
                    {
                        let display_url = website.trim_start_matches("https://")
                            .trim_start_matches("http://")
                            .trim_end_matches('/')
                            .to_string();
                        rsx! {
                            a {
                                href: "{website}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-3 p-4 hover:bg-accent transition",
                                GlobeIcon { class: "w-5 h-5".to_string() }
                                div { class: "flex-1 min-w-0",
                                    p { class: "text-sm text-muted-foreground", "Website" }
                                    p { class: "text-sm text-primary truncate",
                                        "{display_url}"
                                    }
                                }
                                ExternalLinkIcon { class: "w-4 h-4".to_string() }
                            }
                        }
                    }
                }
            }

            if place.btcmap_match.is_some() {
                div { class: "flex items-center gap-3 p-4 bg-orange-500/10 border border-orange-500/20 rounded-xl",
                    span { class: "text-2xl", "₿" }
                    div {
                        p { class: "font-medium text-orange-500", "Accepts Bitcoin" }
                        if let Some(btc) = &place.btcmap_match {
                            if let Some(name) = &btc.name {
                                p { class: "text-sm text-muted-foreground", "Listed on BTCMap as {name}" }
                            }
                        }
                    }
                }
            }

            div { class: "bg-card border border-border rounded-xl p-4",
                div { class: "flex items-center justify-between",
                    span { class: "text-muted-foreground text-sm", "Created by" }
                    Link {
                        to: Route::Profile {
                            pubkey: crate::utils::nip19_urls::profile_route_id(&place.pubkey),
                        },
                        class: "flex items-center gap-2 text-primary hover:underline",
                        span { class: "font-medium text-sm", "@{creator_name}" }
                    }
                }
                div { class: "flex items-center justify-between mt-3 pt-3 border-t border-border",
                    span { class: "text-muted-foreground text-sm", "Created" }
                    span { class: "text-sm",
                        { format_relative_time(Timestamp::from(place.created_at)) }
                    }
                }
            }

            div { class: "flex gap-3",
                a {
                    href: "{directions_url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-xl border border-border hover:bg-accent transition",
                    MapPinIcon { class: "w-5 h-5".to_string() }
                    "Directions"
                }
                button {
                    class: "flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-xl border border-border hover:bg-accent transition",
                    onclick: {
                        let naddr = naddr.clone();
                        move |_| {
                            let naddr = naddr.clone();
                            spawn(async move {
                                let uri = format!("nostr:{}", naddr);
                                if let Err(e) = copy_to_clipboard(&uri).await {
                                    log::error!("Failed to copy: {:?}", e);
                                }
                            });
                            show_share_toast.set(true);
                        }
                    },
                    ShareIcon { class: "w-5 h-5".to_string() }
                    "Share"
                }
            }

            if *show_share_toast.read() {
                div { class: "fixed bottom-20 left-1/2 -translate-x-1/2 z-50 bg-foreground text-background px-4 py-2 rounded-lg text-sm",
                    "Link copied!"
                }
            }
        }
    }
}
