use crate::services::places::{self, Place};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PlaceCardProps {
    pub place: Place,
    pub user_lat: f64,
    pub user_lon: f64,
}

#[component]
pub fn PlaceCard(props: PlaceCardProps) -> Element {
    let p = &props.place;
    let dist = places::haversine_km(
        props.user_lat,
        props.user_lon,
        p.coordinates[1],
        p.coordinates[0],
    );
    let dist_str = places::format_distance_km(dist);

    let amenity_label = p.amenity.as_deref().map(places::amenity_display_name);

    let is_open = p.opening_hours.as_deref().and_then(places::is_place_open);

    let naddr = places::place_naddr(&p.pubkey, &p.d_tag);

    rsx! {
        Link {
            to: crate::routes::Route::AddressViewer { address: naddr },
            class: "block bg-card border border-border rounded-xl overflow-hidden hover:border-ring transition group",

            div { class: "flex gap-4 p-4",
                div { class: "w-20 h-20 flex-shrink-0 rounded-lg overflow-hidden bg-muted flex items-center justify-center",
                    if let Some(ref logo) = p.logo_url {
                        img {
                            src: "{logo}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                            alt: "{p.name}",
                        }
                    } else {
                        span { class: "text-3xl",
                            { amenity_label.as_ref().map(|l| amenity_emoji(l)).unwrap_or("📍").to_string() }
                        }
                    }
                }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-start justify-between gap-2",
                        h3 { class: "font-medium text-sm line-clamp-1 group-hover:text-blue-500 transition-colors",
                            "{p.name}"
                        }
                        span { class: "text-xs text-muted-foreground whitespace-nowrap flex-shrink-0",
                            "{dist_str}"
                        }
                    }

                    div { class: "flex items-center gap-2 mt-1",
                        if let Some(label) = &amenity_label {
                            span { class: "text-xs text-muted-foreground",
                                "{label}"
                            }
                        }
                        if let Some(open) = is_open {
                            span { class: "text-xs",
                                if open {
                                    span { class: "text-green-500", "Open" }
                                } else {
                                    span { class: "text-red-400", "Closed" }
                                }
                            }
                        }
                    }

                    if let Some(ref addr) = p.address {
                        if let Some(ref street) = addr.street {
                            p { class: "text-xs text-muted-foreground mt-1 line-clamp-1",
                                "{street}"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn amenity_emoji(amenity: &str) -> &'static str {
    match amenity {
        s if s == "Restaurant" || s == "Fast Food" => "🍽️",
        "Cafe" => "☕",
        s if s == "Bar" || s == "Pub" => "🍺",
        "Bakery" => "🥐",
        s if s == "Hotel" || s == "Hostel" => "🏨",
        s if s == "Supermarket" || s == "Convenience Store" || s == "Marketplace" => "🛒",
        "Gas Station" => "⛽",
        "Pharmacy" => "💊",
        s if s == "Bank" || s == "ATM" => "🏦",
        "Parking" => "🅿️",
        s if s == "Gym" || s == "Fitness" => "🏋️",
        s if s == "Hospital" || s == "Clinic" => "🏥",
        "Library" => "📚",
        s if s == "School" || s == "University" => "🎓",
        "Museum" => "🏛️",
        s if s == "Theater" || s == "Cinema" => "🎭",
        "Park" => "🌳",
        "Dentist" | "Doctor" | "Veterinary" => "🩺",
        "Police" => "🚔",
        "Fire Station" => "🚒",
        "Post Office" => "📮",
        "Laundry" => "🧺",
        "Hairdresser" => "💇",
        s if s == "Car Rental" || s == "Car Repair" => "🚗",
        "Place of Worship" => "⛪",
        _ => "📍",
    }
}
