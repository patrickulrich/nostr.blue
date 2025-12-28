// Radio Station Creation Form
// Auth-gated form for creating new Kind 31237 radio station events
use dioxus::prelude::*;
use crate::routes::Route;
use crate::components::icons;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use nostr_sdk::prelude::{EventBuilder, Kind, Tag, TagKind, Coordinate, ToBech32};

/// A stream entry in the form
#[derive(Clone, Default)]
struct StreamEntry {
    url: String,
    format: String,
    bitrate: String,
    codec: String,
    sample_rate: String,
    is_primary: bool,
}

/// Form data for creating a radio station
#[derive(Clone, Default)]
struct StationFormData {
    name: String,
    description: String,
    thumbnail: String,
    website: String,
    location: String,
    country_code: String,
    genres: String,
    languages: String,
    streams: Vec<StreamEntry>,
}

#[component]
pub fn RadioStationNew() -> Element {
    let navigator = use_navigator();
    let is_logged_in = auth_store::get_pubkey().is_some();

    // Form state
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut thumbnail_url = use_signal(String::new);
    let mut website = use_signal(String::new);
    let mut location = use_signal(String::new);
    let mut country_code = use_signal(String::new);
    let mut genres = use_signal(String::new); // Comma-separated
    let mut languages = use_signal(String::new); // Comma-separated
    let mut streams = use_signal(|| vec![StreamEntry::default()]);

    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Redirect if not logged in
    if !is_logged_in {
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center",
                div {
                    class: "text-center space-y-4",
                    div {
                        class: "w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto",
                        span {
                            class: "text-muted-foreground",
                            dangerous_inner_html: icons::LOCK
                        }
                    }
                    h1 {
                        class: "text-xl font-bold",
                        "Authentication Required"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Please log in to add a radio station"
                    }
                    Link {
                        to: Route::RadioHome {},
                        class: "inline-block mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Back to Radio"
                    }
                }
            }
        };
    }

    let mut handle_submit = move |_| {
        // Guard against double submission
        if *is_submitting.read() {
            return;
        }

        let name_val = name.read().clone();
        let description_val = description.read().clone();
        let thumbnail_val = thumbnail_url.read().clone();
        let website_val = website.read().clone();
        let location_val = location.read().clone();
        let country_val = country_code.read().clone();
        let genres_val = genres.read().clone();
        let languages_val = languages.read().clone();
        let streams_val = streams.read().clone();

        // Validation
        if name_val.trim().is_empty() {
            error.set(Some("Station name is required".to_string()));
            return;
        }

        let valid_streams: Vec<_> = streams_val.iter()
            .filter(|s| !s.url.trim().is_empty())
            .collect();

        if valid_streams.is_empty() {
            error.set(Some("At least one stream URL is required".to_string()));
            return;
        }

        is_submitting.set(true);
        error.set(None);

        spawn(async move {
            let form_data = StationFormData {
                name: name_val,
                description: description_val,
                thumbnail: thumbnail_val,
                website: website_val,
                location: location_val,
                country_code: country_val,
                genres: genres_val,
                languages: languages_val,
                streams: streams_val,
            };
            match publish_station(form_data).await {
                Ok(naddr) => {
                    navigator.push(Route::RadioStation { naddr });
                }
                Err(e) => {
                    log::error!("Failed to publish radio station: {}", e);
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-40 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: Route::RadioHome {},
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        dangerous_inner_html: icons::ARROW_LEFT
                    }
                    h1 {
                        class: "text-lg font-bold",
                        "Add Radio Station"
                    }
                }
            }

            // Form
            div {
                class: "p-4 max-w-xl mx-auto",

                // Error display
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }

                form {
                    class: "space-y-6",
                    onsubmit: move |e| {
                        e.prevent_default();
                        handle_submit(());
                    },

                    // Station Name
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "name",
                            "Station Name *"
                        }
                        input {
                            id: "name",
                            r#type: "text",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "e.g., Nostr FM",
                            value: "{name}",
                            oninput: move |e| name.set(e.value())
                        }
                    }

                    // Description
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "description",
                            "Description"
                        }
                        textarea {
                            id: "description",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary min-h-[100px] resize-y",
                            placeholder: "Describe your radio station...",
                            value: "{description}",
                            oninput: move |e| description.set(e.value())
                        }
                    }

                    // Thumbnail URL
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "thumbnail",
                            "Thumbnail URL"
                        }
                        input {
                            id: "thumbnail",
                            r#type: "url",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "https://example.com/logo.png",
                            value: "{thumbnail_url}",
                            oninput: move |e| thumbnail_url.set(e.value())
                        }
                    }

                    // Website
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "website",
                            "Website"
                        }
                        input {
                            id: "website",
                            r#type: "url",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "https://yourstation.com",
                            value: "{website}",
                            oninput: move |e| website.set(e.value())
                        }
                    }

                    // Location & Country
                    div {
                        class: "grid grid-cols-2 gap-4",
                        div {
                            class: "space-y-2",
                            label {
                                class: "block text-sm font-medium",
                                r#for: "location",
                                "Location"
                            }
                            input {
                                id: "location",
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                placeholder: "e.g., New York",
                                value: "{location}",
                                oninput: move |e| location.set(e.value())
                            }
                        }
                        div {
                            class: "space-y-2",
                            label {
                                class: "block text-sm font-medium",
                                r#for: "country",
                                "Country Code"
                            }
                            input {
                                id: "country",
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                placeholder: "e.g., US",
                                maxlength: "2",
                                value: "{country_code}",
                                oninput: move |e| country_code.set(e.value().to_uppercase())
                            }
                        }
                    }

                    // Genres
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "genres",
                            "Genres (comma-separated)"
                        }
                        input {
                            id: "genres",
                            r#type: "text",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "e.g., rock, indie, alternative",
                            value: "{genres}",
                            oninput: move |e| genres.set(e.value())
                        }
                    }

                    // Languages
                    div {
                        class: "space-y-2",
                        label {
                            class: "block text-sm font-medium",
                            r#for: "languages",
                            "Languages (comma-separated)"
                        }
                        input {
                            id: "languages",
                            r#type: "text",
                            class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "e.g., en, es",
                            value: "{languages}",
                            oninput: move |e| languages.set(e.value())
                        }
                    }

                    // Streams section
                    div {
                        class: "space-y-3",
                        div {
                            class: "flex items-center justify-between",
                            label {
                                class: "block text-sm font-medium",
                                "Stream URLs *"
                            }
                            button {
                                r#type: "button",
                                class: "text-sm text-primary hover:underline",
                                onclick: move |_| {
                                    let mut current = streams.read().clone();
                                    current.push(StreamEntry::default());
                                    streams.set(current);
                                },
                                "+ Add Stream"
                            }
                        }

                        for (idx, stream) in streams.read().iter().enumerate() {
                            div {
                                key: "{idx}",
                                class: "p-3 bg-muted/50 rounded-lg space-y-3",

                                // Stream URL
                                div {
                                    class: "flex gap-2",
                                    input {
                                        r#type: "url",
                                        class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                                        placeholder: "https://stream.example.com/live.mp3",
                                        value: "{stream.url}",
                                        oninput: move |e| {
                                            let mut current = streams.read().clone();
                                            if let Some(s) = current.get_mut(idx) {
                                                s.url = e.value();
                                            }
                                            streams.set(current);
                                        }
                                    }
                                    if streams.read().len() > 1 {
                                        button {
                                            r#type: "button",
                                            class: "p-2 text-destructive hover:bg-destructive/10 rounded-lg transition",
                                            onclick: move |_| {
                                                    let mut current = streams.read().clone();
                                                    current.remove(idx);
                                                    streams.set(current);
                                                },
                                            dangerous_inner_html: icons::TRASH
                                        }
                                    }
                                }

                                // Format, Bitrate, Primary
                                div {
                                    class: "flex gap-2 items-center",
                                    select {
                                        class: "px-2 py-1.5 bg-background border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                                        value: "{stream.format}",
                                        onchange: move |e| {
                                                let mut current = streams.read().clone();
                                                if let Some(s) = current.get_mut(idx) {
                                                    s.format = e.value();
                                                }
                                                streams.set(current);
                                            },
                                        option { value: "", "Format" }
                                        option { value: "audio/mpeg", "MP3" }
                                        option { value: "audio/aac", "AAC" }
                                        option { value: "audio/ogg", "OGG" }
                                        option { value: "application/vnd.apple.mpegurl", "HLS" }
                                    }
                                    input {
                                        r#type: "number",
                                        class: "w-20 px-2 py-1.5 bg-background border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                                        placeholder: "kbps",
                                        value: "{stream.bitrate}",
                                        oninput: move |e| {
                                                let mut current = streams.read().clone();
                                                if let Some(s) = current.get_mut(idx) {
                                                    s.bitrate = e.value();
                                                }
                                                streams.set(current);
                                            }
                                    }
                                    label {
                                        class: "flex items-center gap-1.5 text-sm cursor-pointer",
                                        input {
                                            r#type: "checkbox",
                                            class: "w-4 h-4",
                                            checked: stream.is_primary,
                                            onchange: move |e| {
                                                    let mut current = streams.read().clone();
                                                    // Uncheck all others if this is being set to primary
                                                    if e.checked() {
                                                        for (i, s) in current.iter_mut().enumerate() {
                                                            s.is_primary = i == idx;
                                                        }
                                                    } else if let Some(s) = current.get_mut(idx) {
                                                        s.is_primary = false;
                                                    }
                                                    streams.set(current);
                                                }
                                        }
                                        "Primary"
                                    }
                                }
                            }
                        }
                    }

                    // Submit button
                    button {
                        r#type: "submit",
                        class: "w-full py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *is_submitting.read(),
                        if *is_submitting.read() {
                            "Publishing..."
                        } else {
                            "Publish Station"
                        }
                    }
                }
            }
        }
    }
}

/// Publish a radio station event (Kind 31237)
/// Uses wavefunc-compatible tag format for interoperability
async fn publish_station(form: StationFormData) -> std::result::Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let StationFormData {
        name,
        description,
        thumbnail,
        website,
        location,
        country_code,
        genres,
        languages,
        streams,
    } = form;

    // Generate d-tag from name
    let d_tag = name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .replace(' ', "-");

    // Add timestamp suffix to ensure uniqueness
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let d_tag = format!("{}-{}", d_tag, timestamp % 10000);

    // Build tags using wavefunc-compatible format
    let mut tags: Vec<Tag> = vec![
        Tag::custom(TagKind::d(), vec![d_tag.clone()]),
        // wavefunc uses "name" tag
        Tag::custom(TagKind::custom("name"), vec![name.clone()]),
    ];

    if !description.trim().is_empty() {
        // wavefunc uses "description" tag
        tags.push(Tag::custom(TagKind::custom("description"), vec![description]));
    }

    if !thumbnail.trim().is_empty() {
        // wavefunc uses "thumbnail" tag
        tags.push(Tag::custom(TagKind::custom("thumbnail"), vec![thumbnail]));
    }

    if !website.trim().is_empty() {
        tags.push(Tag::custom(TagKind::custom("website"), vec![website]));
    }

    if !location.trim().is_empty() {
        tags.push(Tag::custom(TagKind::custom("location"), vec![location]));
    }

    if !country_code.trim().is_empty() {
        // wavefunc uses "country" tag
        tags.push(Tag::custom(TagKind::custom("country"), vec![country_code.to_uppercase()]));
    }

    // Parse and add genres as category tags: ["c", "genre-name", "genre"]
    // This is wavefunc format
    for genre in genres.split(',') {
        let genre = genre.trim().to_lowercase();
        if !genre.is_empty() {
            tags.push(Tag::custom(TagKind::custom("c"), vec![genre, "genre".to_string()]));
        }
    }

    // Parse and add languages using lowercase "l" (wavefunc format)
    for lang in languages.split(',') {
        let lang = lang.trim().to_lowercase();
        if !lang.is_empty() {
            tags.push(Tag::custom(TagKind::custom("l"), vec![lang]));
        }
    }

    // Add stream tags with JSON quality object (wavefunc format)
    // Format: ["stream", "url", "format", JSON.stringify({bitrate, codec, sampleRate}), "primary"?]
    for stream in streams.iter() {
        if stream.url.trim().is_empty() {
            continue;
        }

        // Build quality JSON object
        let bitrate: u32 = stream.bitrate.parse().unwrap_or(128);
        let codec = if !stream.codec.is_empty() {
            stream.codec.clone()
        } else {
            // Infer codec from format
            match stream.format.as_str() {
                "audio/mpeg" => "mp3".to_string(),
                "audio/aac" => "aac".to_string(),
                "audio/ogg" => "vorbis".to_string(),
                "application/vnd.apple.mpegurl" => "aac".to_string(),
                _ => "mp3".to_string(),
            }
        };
        let sample_rate: u32 = stream.sample_rate.parse().unwrap_or(44100);

        let quality_json = format!(
            r#"{{"bitrate":{},"codec":"{}","sampleRate":{}}}"#,
            bitrate, codec, sample_rate
        );

        let mut stream_values = vec![
            stream.url.trim().to_string(),
            if !stream.format.is_empty() { stream.format.clone() } else { "audio/mpeg".to_string() },
            quality_json,
        ];

        if stream.is_primary {
            stream_values.push("primary".to_string());
        }

        tags.push(Tag::custom(TagKind::custom("stream"), stream_values));
    }

    // Client tag for attribution
    tags.push(Tag::custom(TagKind::custom("client"), vec!["nostr.blue".to_string()]));

    // Build and publish event
    let event_builder = EventBuilder::new(Kind::from(31237), "")
        .tags(tags);

    let client = nostr_client::get_client()
        .ok_or_else(|| "Failed to get client".to_string())?;

    let output = client.send_event_builder(event_builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = output.id().to_string();

    // Build naddr
    let pubkey = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?
        .get_public_key().await
        .map_err(|e| format!("Failed to get public key: {}", e))?;

    let coordinate = Coordinate::new(Kind::from(31237), pubkey)
        .identifier(d_tag);

    let naddr = coordinate.to_bech32()
        .map_err(|e| format!("Failed to encode naddr: {}", e))?;

    log::info!("Published radio station: {} ({})", event_id, naddr);
    Ok(naddr)
}
