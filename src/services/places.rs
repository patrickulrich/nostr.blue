use chrono::Datelike;
use crate::stores::nostr_client;
use nostr_sdk::{Alphabet, Event, Filter, Kind, SingleLetterTag, Tag, TagKind, Timestamp};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use instant::Instant;

const PLACES_KIND: u16 = 37515;

pub const AMENITY_CATEGORIES: &[(&str, &str)] = &[
    ("restaurant", "Restaurant"),
    ("cafe", "Cafe"),
    ("bar", "Bar"),
    ("fast_food", "Fast Food"),
    ("bakery", "Bakery"),
    ("pub", "Pub"),
    ("hotel", "Hotel"),
    ("hostel", "Hostel"),
    ("supermarket", "Supermarket"),
    ("convenience_store", "Convenience Store"),
    ("fuel", "Gas Station"),
    ("pharmacy", "Pharmacy"),
    ("bank", "Bank"),
    ("atm", "ATM"),
    ("parking", "Parking"),
    ("gym", "Gym"),
    ("fitness_centre", "Fitness"),
    ("hospital", "Hospital"),
    ("clinic", "Clinic"),
    ("library", "Library"),
    ("school", "School"),
    ("university", "University"),
    ("museum", "Museum"),
    ("theatre", "Theater"),
    ("cinema", "Cinema"),
    ("place_of_worship", "Place of Worship"),
    ("park", "Park"),
    ("post_office", "Post Office"),
    ("police", "Police"),
    ("fire_station", "Fire Station"),
    ("laundry", "Laundry"),
    ("hairdresser", "Hairdresser"),
    ("dentist", "Dentist"),
    ("doctors", "Doctor"),
    ("veterinary", "Veterinary"),
    ("car_rental", "Car Rental"),
    ("car_repair", "Car Repair"),
    ("marketplace", "Marketplace"),
];

pub fn amenity_display_name(amenity: &str) -> &str {
    AMENITY_CATEGORIES
        .iter()
        .find(|(key, _)| *key == amenity)
        .map(|(_, label)| *label)
        .unwrap_or(amenity)
}

pub fn format_distance_km(km: f64) -> String {
    if km < 1.0 {
        format!("{:.0} m", km * 1000.0)
    } else if km < 100.0 {
        format!("{:.1} km", km)
    } else {
        format!("{:.0} km", km)
    }
}

pub fn place_naddr(pubkey: &str, d_tag: &str) -> String {
    use nostr_sdk::nips::nip01::Coordinate;
    use nostr_sdk::nips::nip19::ToBech32;
    if let Ok(pk) = nostr_sdk::PublicKey::from_hex(pubkey) {
        let coord = Coordinate::new(nostr_sdk::Kind::Custom(PLACES_KIND), pk).identifier(d_tag);
        if let Ok(bech32) = coord.to_bech32() {
            return bech32;
        }
    }
    format!("{}:{}:{}", PLACES_KIND, pubkey, d_tag)
}

pub fn is_place_open(hours: &str) -> Option<bool> {
    let now = chrono::Utc::now();
    let weekday = now.weekday().num_days_from_monday() as usize;
    let day_prefixes = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let day = day_prefixes.get(weekday)?;
    let current_time = now.format("%H%M").to_string();
    let current_num: u32 = current_time.parse().ok()?;

    for segment in hours.split(&[',', ';'][..]) {
        let segment = segment.trim().replace('_', " ");
        if !segment.starts_with(day) {
            if segment.contains('-') {
                continue;
            }
            if !segment.contains("Mo") && !segment.contains("Tu") && !segment.contains("We")
                && !segment.contains("Th") && !segment.contains("Fr")
                && !segment.contains("Sa") && !segment.contains("Su")
            {
                continue;
            }
            continue;
        }
        if let Some(time_part) = segment.split_whitespace().nth(1) {
            let parts: Vec<&str> = time_part.split('-').collect();
            if parts.len() == 2 {
                let open: u32 = parts[0].replace(':', "").parse().ok()?;
                let close: u32 = parts[1].replace(':', "").parse().ok()?;
                let close = if close < open { close + 2400 } else { close };
                let current_adj = if current_num < open && close > 2400 {
                    current_num + 2400
                } else {
                    current_num
                };
                return Some(current_adj >= open && current_adj <= close);
            }
        }
    }
    None
}

pub fn geohash_prefix(lat: f64, lng: f64, precision: usize) -> String {
    geohash::encode(geohash::Coord { x: lng, y: lat }, precision)
        .unwrap_or_default()
        .chars()
        .take(precision)
        .collect()
}

pub fn geohash_precisions_for_zoom(zoom: f64) -> Vec<usize> {
    let mut precisions = vec![2];
    if zoom >= 7.0 {
        precisions.push(3);
    }
    if zoom >= 11.0 {
        precisions.push(4);
    }
    if zoom >= 14.0 {
        precisions.push(5);
    }
    precisions
}

fn dedup_places(places: Vec<Place>) -> Vec<Place> {
    let mut seen = HashMap::<String, Place>::new();
    for place in places {
        let key = format!("{}:{}", place.pubkey, place.d_tag);
        match seen.get(&key) {
            Some(existing) if existing.created_at >= place.created_at => {}
            _ => {
                seen.insert(key, place);
            }
        }
    }
    seen.into_values().collect()
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().asin()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlaceAddress {
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postcode: Option<String>,
    pub country: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Place {
    pub id: String,
    pub pubkey: String,
    pub d_tag: String,
    pub name: String,
    pub description: Option<String>,
    pub amenity: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub logo_url: Option<String>,
    pub opening_hours: Option<String>,
    pub wheelchair: Option<String>,
    pub address: Option<PlaceAddress>,
    pub osm_ref: Option<String>,
    pub geohashes: Vec<String>,
    pub coordinates: [f64; 2],
    pub geojson: String,
    pub created_at: u64,
    pub deleted: bool,
    pub btcmap_match: Option<BtcMapPlace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BtcMapPlace {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub address: Option<String>,
    pub opening_hours: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub osm_id: Option<String>,
    pub comments: Option<u32>,
    pub verified_at: Option<String>,
    pub boosted_until: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OsmEnrichment {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub opening_hours: Option<String>,
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postcode: Option<String>,
    pub country: Option<String>,
    pub amenity: Option<String>,
}

impl OsmEnrichment {
    #[allow(dead_code)]
    pub fn is_any_set(&self) -> bool {
        self.name.is_some()
            || self.phone.is_some()
            || self.website.is_some()
            || self.opening_hours.is_some()
            || self.street.is_some()
            || self.city.is_some()
            || self.state.is_some()
            || self.amenity.is_some()
    }
}

static OSM_CACHE: Lazy<Mutex<HashMap<String, (OsmEnrichment, Instant)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const OSM_CACHE_TTL: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OsmApiResponse {
    elements: Vec<OsmElement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OsmElement {
    tags: Option<HashMap<String, String>>,
}

pub async fn fetch_osm_enrichment(osm_ref: &str) -> Option<OsmEnrichment> {
    if osm_ref.is_empty() {
        return None;
    }

    let first = osm_ref.chars().next()?;
    if first != 'N' {
        log::debug!("Places: skipping non-node OSM ref '{}'", osm_ref);
        return None;
    }

    let numeric_id = &osm_ref[1..];
    if numeric_id.parse::<u64>().is_err() {
        log::warn!("Places: invalid OSM ref format '{}'", osm_ref);
        return None;
    }

    {
        let cache = OSM_CACHE.lock().ok()?;
        if let Some((enrichment, fetched_at)) = cache.get(osm_ref) {
            if fetched_at.elapsed() < OSM_CACHE_TTL {
                log::debug!("Places: OSM cache hit for '{}'", osm_ref);
                return Some(enrichment.clone());
            }
        }
    }

    let url = format!(
        "https://api.openstreetmap.org/api/0.6/node/{}.json",
        numeric_id
    );

    let client = match crate::platform::http::http_client() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Places: OSM HTTP client error: {}", e);
            return None;
        }
    };

    let response = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Places: OSM fetch failed for '{}': {}", osm_ref, e);
            return None;
        }
    };

    if !response.status().is_success() {
        log::warn!(
            "Places: OSM API returned {} for '{}'",
            response.status(),
            osm_ref
        );
        return None;
    }

    let api_resp: OsmApiResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Places: OSM parse failed for '{}': {}", osm_ref, e);
            return None;
        }
    };

    let element = api_resp.elements.into_iter().next()?;
    let tags = element.tags.unwrap_or_default();

    let amenity = ["amenity", "shop", "tourism", "leisure", "office"]
        .iter()
        .find_map(|key| tags.get(*key).cloned());

    let enrichment = OsmEnrichment {
        name: tags.get("name").cloned(),
        phone: tags.get("phone").cloned(),
        website: tags.get("website").cloned(),
        opening_hours: tags.get("opening_hours").cloned(),
        street: tags.get("addr:street").cloned(),
        city: tags.get("addr:city").cloned(),
        state: tags.get("addr:state").cloned(),
        postcode: tags.get("addr:postcode").cloned(),
        country: tags.get("addr:country").cloned(),
        amenity,
    };

    if let Ok(mut cache) = OSM_CACHE.lock() {
        cache.insert(osm_ref.to_string(), (enrichment.clone(), Instant::now()));
    }

    log::info!(
        "Places: OSM enrichment for '{}' → name={:?}, amenity={:?}, phone={:?}",
        osm_ref,
        enrichment.name,
        enrichment.amenity,
        enrichment.phone
    );

    Some(enrichment)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct GeoJsonFeature {
    #[serde(rename = "type")]
    type_field: Option<String>,
    geometry: Option<GeoJsonGeometry>,
    properties: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct GeoJsonFeatureCollection {
    features: Option<Vec<GeoJsonFeature>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct GeoJsonGeometry {
    #[serde(rename = "type")]
    type_field: Option<String>,
    coordinates: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct BtcMapSnapshotPlace {
    id: u64,
    lat: Option<f64>,
    lon: Option<f64>,
    name: Option<String>,
    icon: Option<String>,
}

pub fn parse_place(event: &Event) -> Option<Place> {
    let content = &event.content;
    let mut coordinates = None::<[f64; 2]>;

    if let Ok(fc) = serde_json::from_str::<GeoJsonFeatureCollection>(content) {
        if let Some(features) = &fc.features {
            for feature in features {
                if let Some(geom) = &feature.geometry {
                    if let Some(coords) = &geom.coordinates {
                        if let Some(arr) = coords.as_array() {
                            if arr.len() >= 2 {
                                let lng = arr[0].as_f64()?;
                                let lat = arr[1].as_f64()?;
                                coordinates = Some([lng, lat]);
                                break;
                            }
                        } else if let Some(obj) = coords.as_object() {
                            let lng = obj.get("lng")?.as_f64()?;
                            let lat = obj.get("lat")?.as_f64()?;
                            coordinates = Some([lng, lat]);
                            break;
                        }
                    }
                }
            }
        }
    }

    if coordinates.is_none() {
        if let Ok(feature) = serde_json::from_str::<GeoJsonFeature>(content) {
            if let Some(geom) = &feature.geometry {
                if let Some(coords) = &geom.coordinates {
                    if let Some(arr) = coords.as_array() {
                        if arr.len() >= 2 {
                            let lng = arr[0].as_f64()?;
                            let lat = arr[1].as_f64()?;
                            coordinates = Some([lng, lat]);
                        }
                    }
                }
            }
        }
    }

    coordinates?;

    let d_tag = event.tags.identifier().unwrap_or("").to_string();

    let mut name = get_tag_value(event, "name").unwrap_or_default();
    let mut amenity = get_tag_value(event, "amenity");
    if amenity.is_none() {
        amenity = get_tag_value(event, "shop");
    }
    if amenity.is_none() {
        amenity = get_tag_value(event, "tourism");
    }
    if amenity.is_none() {
        amenity = get_tag_value(event, "leisure");
    }
    if amenity.is_none() {
        amenity = get_tag_value(event, "office");
    }
    let mut phone = get_tag_value(event, "phone");
    let mut website = get_tag_value(event, "website");
    let mut opening_hours = get_tag_value(event, "opening_hours");
    let mut logo_url = get_tag_value(event, "logo_url");
    let wheelchair = get_tag_value(event, "wheelchair");
    let mut osm_ref = get_tag_value_with_third(event, "i", "osm_ref");
    if osm_ref.is_none() {
        osm_ref = get_tag_value_with_third(event, "r", "osm_ref");
    }

    let mut street = get_tag_value(event, "addr:street");
    let mut city = get_tag_value(event, "addr:city");
    let state = get_tag_value(event, "addr:state");
    let postcode = get_tag_value(event, "addr:postcode");
    let mut country = get_tag_value(event, "addr:country");
    if country.is_none() {
        country = get_tag_value(event, "country");
    }

    let description = get_tag_value(event, "alt");

    let g_kind = TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::G));
    let geohashes: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.kind() == g_kind)
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    if name.is_empty() {
        if let Ok(feature) = serde_json::from_str::<GeoJsonFeature>(content) {
            if let Some(props) = &feature.properties {
                if let Some(n) = props.get("name").and_then(|v| v.as_str()) {
                    name = n.to_string();
                }
                if amenity.is_none() {
                    amenity = props.get("type").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                if phone.is_none() {
                    phone = props.get("phone").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                if website.is_none() {
                    website = props.get("website").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                if opening_hours.is_none() {
                    opening_hours = props.get("hours").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                if logo_url.is_none() {
                    logo_url = props.get("logo_url").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                if street.is_none() || city.is_none() {
                    if let Some(addr) = props.get("address") {
                        if street.is_none() {
                            street = addr.get("street-address").and_then(|v| v.as_str()).map(|s| s.to_string());
                        }
                        if city.is_none() {
                            city = addr.get("locality").and_then(|v| v.as_str()).map(|s| s.to_string());
                        }
                    }
                }
            }
        }
    }

    let address = if street.is_some() || city.is_some() || state.is_some() || postcode.is_some() || country.is_some() {
        Some(PlaceAddress { street, city, state, postcode, country })
    } else {
        None
    };

    Some(Place {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        d_tag,
        name,
        description,
        amenity,
        phone,
        website,
        logo_url,
        opening_hours,
        wheelchair,
        address,
        osm_ref,
        geohashes,
        coordinates: coordinates?,
        geojson: content.clone(),
        created_at: event.created_at.as_secs(),
        deleted: false,
        btcmap_match: None,
    })
}

fn get_tag_value(event: &Event, key: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(key))
        .and_then(|t| {
            let slice = t.as_slice();
            if slice.len() > 1 && !slice[1].is_empty() {
                Some(slice[1].clone())
            } else {
                None
            }
        })
}

fn get_tag_value_with_third(event: &Event, key: &str, _third: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some(key) && slice.get(2).map(|s| s.as_str()) == Some(_third)
        })
        .and_then(|t| t.as_slice().get(1).cloned())
}

#[allow(dead_code)]
pub async fn fetch_places() -> Result<Vec<Place>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let filter = Filter::new().kind(Kind::Custom(PLACES_KIND)).limit(5000);
    let events = client
        .fetch_events(filter, Duration::from_secs(30))
        .await
        .map_err(|e| format!("Failed to fetch places: {}", e))?;

    let places: Vec<Place> = events.into_iter().filter_map(|e| parse_place(&e)).collect();
    Ok(dedup_places(places))
}

pub async fn fetch_places_for_geohash(prefix: &str) -> Result<Vec<Place>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::Custom(PLACES_KIND))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::G), prefix)
        .limit(5000);
    let events = client
        .fetch_events(filter, Duration::from_secs(15))
        .await
        .map_err(|e| format!("Failed to fetch places for geohash {}: {}", prefix, e))?;

    let count = events.len();
    let places: Vec<Place> = events.into_iter().filter_map(|e| parse_place(&e)).collect();
    log::info!(
        "Places: fetched {} events → {} valid places for geohash '{}'",
        count,
        places.len(),
        prefix
    );
    Ok(dedup_places(places))
}

#[allow(dead_code)]
pub async fn fetch_geochat_messages(geohash: &str, since: Timestamp) -> Result<Vec<Event>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::G), geohash)
        .since(since)
        .limit(100);
    client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| format!("Failed to fetch geochat: {}", e))
}

#[allow(clippy::too_many_arguments, dead_code)]
pub fn build_place_event_builder(
    d_tag: &str,
    name: &str,
    description: Option<&str>,
    amenity: Option<&str>,
    phone: Option<&str>,
    website: Option<&str>,
    logo_url: Option<&str>,
    opening_hours: Option<&str>,
    wheelchair: Option<&str>,
    address: Option<&PlaceAddress>,
    lat: f64,
    lng: f64,
) -> nostr_sdk::EventBuilder {
    let geojson = format!(
        r#"{{"type":"FeatureCollection","features":[{{"type":"Feature","properties":{{}},"geometry":{{"type":"Point","coordinates":[{},{}]}}}}]}}"#,
        lng, lat
    );

    let geohash = geohash::encode(geohash::Coord { x: lng, y: lat }, 8usize).unwrap_or_default();

    let mut tags: Vec<Tag> = Vec::new();

    tags.push(Tag::identifier(d_tag));
    tags.push(Tag::custom(TagKind::from("name"), [name]));

    if let Some(desc) = description {
        tags.push(Tag::custom(TagKind::from("alt"), [desc]));
    }
    if let Some(am) = amenity {
        tags.push(Tag::custom(TagKind::from("amenity"), [am]));
    }
    if let Some(ph) = phone {
        tags.push(Tag::custom(TagKind::from("phone"), [ph]));
    }
    if let Some(ws) = website {
        tags.push(Tag::custom(TagKind::from("website"), [ws]));
    }
    if let Some(lo) = logo_url {
        tags.push(Tag::custom(TagKind::from("logo_url"), [lo]));
    }
    if let Some(oh) = opening_hours {
        tags.push(Tag::custom(TagKind::from("opening_hours"), [oh]));
    }
    if let Some(wc) = wheelchair {
        tags.push(Tag::custom(TagKind::from("wheelchair"), [wc]));
    }

    if let Some(addr) = address {
        if let Some(ref s) = addr.street {
            tags.push(Tag::custom(TagKind::from("addr:street"), [s.as_str()]));
        }
        if let Some(ref c) = addr.city {
            tags.push(Tag::custom(TagKind::from("addr:city"), [c.as_str()]));
        }
        if let Some(ref s) = addr.state {
            tags.push(Tag::custom(TagKind::from("addr:state"), [s.as_str()]));
        }
        if let Some(ref p) = addr.postcode {
            tags.push(Tag::custom(TagKind::from("addr:postcode"), [p.as_str()]));
        }
        if let Some(ref c) = addr.country {
            tags.push(Tag::custom(TagKind::from("addr:country"), [c.as_str()]));
        }
    }

    let g_kind = TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::G));
    for i in (1..=geohash.len()).rev() {
        tags.push(Tag::custom(g_kind.clone(), [&geohash[..i]]));
    }

    nostr_sdk::EventBuilder::new(Kind::Custom(PLACES_KIND), &geojson).tags(tags)
}

#[allow(dead_code)]
pub async fn fetch_btcmap_snapshot() -> Result<Vec<BtcMapPlace>, String> {
    let url = "https://cdn.static.btcmap.org/api/v4/places.json";
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("BTCMap snapshot fetch failed: {}", e))?;

    let snapshot: Vec<BtcMapSnapshotPlace> = response
        .json()
        .await
        .map_err(|e| format!("BTCMap snapshot parse failed: {}", e))?;

    Ok(snapshot
        .into_iter()
        .filter_map(|s| {
            Some(BtcMapPlace {
                id: s.id,
                lat: s.lat?,
                lon: s.lon?,
                name: s.name,
                icon: s.icon,
                address: None,
                opening_hours: None,
                phone: None,
                website: None,
                osm_id: None,
                comments: None,
                verified_at: None,
                boosted_until: None,
                deleted_at: None,
            })
        })
        .collect())
}

#[allow(dead_code)]
pub async fn fetch_btcmap_place_detail(id: u64) -> Result<BtcMapPlace, String> {
    let url = format!(
        "https://api.btcmap.org/v4/places/{}?fields=id,lat,lon,name,address,phone,website,opening_hours,osm_id,icon,verified_at,boosted_until,comments",
        id
    );
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("BTCMap detail fetch failed: {}", e))?;

    response.json::<BtcMapPlace>().await.map_err(|e| format!("BTCMap detail parse failed: {}", e))
}

#[allow(dead_code)]
pub async fn fetch_btcmap_places_in_viewport(lat: f64, lon: f64, radius_km: f64) -> Result<Vec<BtcMapPlace>, String> {
    let url = format!(
        "https://api.btcmap.org/v4/places/search?lat={}&lon={}&radius_km={}&fields=id,lat,lon,name,address,phone,website,opening_hours,osm_id,icon,verified_at",
        lat, lon, radius_km
    );
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("BTCMap search failed: {}", e))?;

    response.json::<Vec<BtcMapPlace>>().await.map_err(|e| format!("BTCMap search parse failed: {}", e))
}
