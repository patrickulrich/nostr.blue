use crate::utils::url_metadata;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct LinkPreviewProps {
    pub url: String,
    #[props(default)]
    pub class: Option<String>,
}

#[component]
pub fn LinkPreview(props: LinkPreviewProps) -> Element {
    let mut metadata = use_signal(|| None::<url_metadata::UrlMetadata>);
    let mut fetching = use_signal(|| false);

    let url_key = props.url.clone();

    use_effect(use_reactive!(|(url_key,)| {
        if let Some(cached) = url_metadata::get_cached_url_metadata(&url_key) {
            if cached.title.is_some() || cached.description.is_some() || cached.image.is_some() {
                metadata.set(Some(cached));
                return;
            }
        }
        if *fetching.read() {
            return;
        }
        fetching.set(true);
        let url = url_key.clone();
        spawn(async move {
            if let Ok(meta) = url_metadata::fetch_url_metadata(url.clone()).await {
                if meta.title.is_some() || meta.description.is_some() || meta.image.is_some() {
                    metadata.set(Some(meta));
                }
            }
            fetching.set(false);
        });
    }));

    let meta_val = metadata.read();
    let meta = match &*meta_val {
        Some(m) => m,
        None => return rsx! {},
    };

    let domain = meta
        .url
        .strip_prefix("https://")
        .or_else(|| meta.url.strip_prefix("http://"))
        .unwrap_or(&meta.url)
        .split('/')
        .next()
        .unwrap_or(&meta.url)
        .to_string();

    let extra_class = props.class.clone().unwrap_or_default();
    let container_class = format!(
        "block mt-2 rounded-lg border border-border overflow-hidden bg-muted/30 hover:bg-muted/60 transition no-underline {extra_class}"
    );

    rsx! {
        a {
            href: "{meta.url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "{container_class}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if let Some(img) = &meta.image {
                div {
                    class: "w-full max-h-[200px] overflow-hidden border-b border-border",
                    img {
                        src: "{img}",
                        alt: meta.title.as_deref().unwrap_or(""),
                        loading: "lazy",
                        class: "w-full h-full object-cover",
                    }
                }
            }
            div {
                class: "p-3 space-y-1",
                if let Some(title) = &meta.title {
                    h4 { class: "text-sm font-semibold text-foreground line-clamp-2 leading-snug", "{title}" }
                }
                if let Some(desc) = &meta.description {
                    p { class: "text-xs text-muted-foreground line-clamp-2 leading-relaxed", "{desc}" }
                }
                div {
                    class: "flex items-center gap-1.5 text-xs text-muted-foreground/70 pt-0.5",
                    svg {
                        class: "h-3 w-3 shrink-0",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                        path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                    }
                    span { class: "truncate", "{domain}" }
                }
            }
        }
    }
}
