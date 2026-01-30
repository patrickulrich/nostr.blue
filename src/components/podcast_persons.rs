//! Podcast Persons Component
//!
//! Displays host and guest credits for podcast episodes with:
//! - Person avatars and names
//! - Role badges (host, guest, editor, etc.)
//! - Links to bios/social profiles
//! - Group organization (cast, crew)

use crate::components::icons;
use crate::utils::podcast::Person;
use dioxus::prelude::*;

// ============================================================================
// Persons List Component
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct PodcastPersonsProps {
    /// List of persons/contributors
    pub persons: Vec<Person>,
    /// Title for the section (e.g., "Hosts", "Guests")
    #[props(default)]
    pub title: Option<String>,
    /// Show in compact mode (avatars only)
    #[props(default = false)]
    pub compact: bool,
    /// Maximum persons to show before "more" (0 = all)
    #[props(default = 0)]
    pub max_visible: usize,
}

/// Podcast persons/contributors display component
#[component]
pub fn PodcastPersons(props: PodcastPersonsProps) -> Element {
    if props.persons.is_empty() {
        return rsx! {};
    }

    let mut show_all = use_signal(|| false);

    // Group by role/group if not compact (clone once, then move into categories)
    let (hosts, guests, crew) = categorize_persons(props.persons.clone());

    let display_persons: Vec<&Person> = if props.max_visible > 0 && !*show_all.read() {
        props.persons.iter().take(props.max_visible).collect()
    } else {
        props.persons.iter().collect()
    };

    let has_more = props.max_visible > 0 && props.persons.len() > props.max_visible;

    if props.compact {
        // Compact avatar row
        rsx! {
            div {
                class: "flex items-center",

                // Avatars
                div {
                    class: "flex -space-x-2",
                    for (idx, person) in display_persons.iter().enumerate() {
                        PersonAvatar {
                            key: "{idx}",
                            person: (*person).clone(),
                            size: AvatarSize::Small
                        }
                    }
                }

                // "More" indicator
                if has_more && !*show_all.read() {
                    button {
                        class: "ml-1 text-xs text-muted-foreground hover:text-foreground",
                        onclick: move |_| show_all.set(true),
                        "+{props.persons.len() - props.max_visible} more"
                    }
                }
            }
        }
    } else {
        // Full grouped display
        rsx! {
            div {
                class: "space-y-4",

                // Optional section title
                if let Some(ref title) = props.title {
                    h3 {
                        class: "font-semibold text-sm text-muted-foreground",
                        "{title}"
                    }
                }

                // Hosts section
                if !hosts.is_empty() {
                    PersonGroup {
                        title: "Hosts",
                        persons: hosts.clone(),
                        icon: icons::MIC
                    }
                }

                // Guests section
                if !guests.is_empty() {
                    PersonGroup {
                        title: "Guests",
                        persons: guests.clone(),
                        icon: icons::USER
                    }
                }

                // Crew section
                if !crew.is_empty() {
                    PersonGroup {
                        title: "Crew",
                        persons: crew.clone(),
                        icon: icons::SETTINGS
                    }
                }
            }
        }
    }
}

/// Categorize persons into hosts, guests, and crew
/// Takes ownership to avoid per-item cloning (O(1) clone at call site vs O(n) clones)
fn categorize_persons(persons: Vec<Person>) -> (Vec<Person>, Vec<Person>, Vec<Person>) {
    let mut hosts = Vec::new();
    let mut guests = Vec::new();
    let mut crew = Vec::new();

    for person in persons {
        let role = person.role.as_deref().unwrap_or("").to_lowercase();
        let group = person.group.as_deref().unwrap_or("").to_lowercase();

        if role == "host" || role.contains("host") {
            hosts.push(person);
        } else if role == "guest" || role.contains("guest") || group == "cast" {
            guests.push(person);
        } else if group == "crew" || !role.is_empty() {
            crew.push(person);
        } else {
            // Default to guests for unspecified
            guests.push(person);
        }
    }

    (hosts, guests, crew)
}

// ============================================================================
// Person Group
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct PersonGroupProps {
    title: &'static str,
    persons: Vec<Person>,
    icon: &'static str,
}

#[component]
fn PersonGroup(props: PersonGroupProps) -> Element {
    rsx! {
        div {
            class: "space-y-2",

            // Group header
            div {
                class: "flex items-center gap-2 text-xs font-medium text-muted-foreground uppercase tracking-wide",
                span {
                    class: "w-4 h-4",
                    dangerous_inner_html: props.icon
                }
                "{props.title}"
            }

            // Person cards
            div {
                class: "grid grid-cols-1 sm:grid-cols-2 gap-2",
                for (idx, person) in props.persons.iter().enumerate() {
                    PersonCard {
                        key: "{idx}",
                        person: person.clone()
                    }
                }
            }
        }
    }
}

// ============================================================================
// Person Card
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct PersonCardProps {
    /// The person to display
    pub person: Person,
    /// Show in compact mode
    #[props(default = false)]
    pub compact: bool,
}

/// Individual person card with avatar, name, and role
#[component]
pub fn PersonCard(props: PersonCardProps) -> Element {
    let person = &props.person;

    let has_link = person.href.is_some();
    let role_display = person.role.as_deref().unwrap_or("");

    if props.compact {
        // Compact inline display
        rsx! {
            div {
                class: "flex items-center gap-2",
                PersonAvatar {
                    person: person.clone(),
                    size: AvatarSize::Small
                }
                span {
                    class: "text-sm font-medium truncate",
                    "{person.name}"
                }
            }
        }
    } else {
        // Full card
        rsx! {
            div {
                class: "flex items-center gap-3 p-2 rounded-lg bg-muted/50 hover:bg-muted transition",

                // Avatar
                PersonAvatar {
                    person: person.clone(),
                    size: AvatarSize::Medium
                }

                // Info
                div {
                    class: "flex-1 min-w-0",

                    // Name (with optional link)
                    if has_link {
                        a {
                            href: "{person.href.as_ref().unwrap()}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "font-medium text-sm hover:text-primary hover:underline truncate block",
                            "{person.name}"
                        }
                    } else {
                        span {
                            class: "font-medium text-sm truncate block",
                            "{person.name}"
                        }
                    }

                    // Role badge
                    if !role_display.is_empty() {
                        span {
                            class: "inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-primary/10 text-primary capitalize",
                            "{role_display}"
                        }
                    }
                }

                // External link indicator
                if has_link {
                    a {
                        href: "{person.href.as_ref().unwrap()}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::EXTERNAL_LINK
                    }
                }
            }
        }
    }
}

// ============================================================================
// Person Avatar
// ============================================================================

#[derive(Clone, Copy, PartialEq)]
pub enum AvatarSize {
    Small,  // 24px
    Medium, // 40px
    #[allow(dead_code)]
    Large, // 64px - reserved for future use
}

impl AvatarSize {
    fn class(&self) -> &'static str {
        match self {
            AvatarSize::Small => "w-6 h-6",
            AvatarSize::Medium => "w-10 h-10",
            AvatarSize::Large => "w-16 h-16",
        }
    }

    fn text_class(&self) -> &'static str {
        match self {
            AvatarSize::Small => "text-xs",
            AvatarSize::Medium => "text-sm",
            AvatarSize::Large => "text-xl",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PersonAvatarProps {
    person: Person,
    #[props(default = AvatarSize::Medium)]
    size: AvatarSize,
}

/// Person avatar with fallback to initials
#[component]
pub fn PersonAvatar(props: PersonAvatarProps) -> Element {
    let person = &props.person;
    let size_class = props.size.class();
    let text_class = props.size.text_class();

    // Generate initials from name
    let initials: String = person
        .name
        .split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase();

    // Generate consistent color from name
    let color_idx = person.name.bytes().fold(0u8, |acc, b| acc.wrapping_add(b)) % 6;
    let bg_colors = [
        "bg-red-500/20 text-red-600",
        "bg-blue-500/20 text-blue-600",
        "bg-green-500/20 text-green-600",
        "bg-yellow-500/20 text-yellow-600",
        "bg-purple-500/20 text-purple-600",
        "bg-pink-500/20 text-pink-600",
    ];
    let color_class = bg_colors[color_idx as usize];

    rsx! {
        div {
            class: "{size_class} rounded-full overflow-hidden shrink-0 ring-2 ring-background",
            title: "{person.name}",

            if let Some(ref img_url) = person.img {
                img {
                    src: "{img_url}",
                    alt: "{person.name}",
                    class: "w-full h-full object-cover"
                }
            } else {
                // Fallback to initials
                div {
                    class: "w-full h-full flex items-center justify-center {color_class} {text_class} font-semibold",
                    "{initials}"
                }
            }
        }
    }
}

// ============================================================================
// Avatar Stack (for compact display)
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct AvatarStackProps {
    /// Persons to show
    pub persons: Vec<Person>,
    /// Maximum avatars to show
    #[props(default = 4)]
    pub max_visible: usize,
    /// Avatar size
    #[props(default = AvatarSize::Small)]
    pub size: AvatarSize,
}

/// Stacked avatars with overlap
#[component]
pub fn AvatarStack(props: AvatarStackProps) -> Element {
    if props.persons.is_empty() {
        return rsx! {};
    }

    let visible: Vec<_> = props.persons.iter().take(props.max_visible).collect();
    let overflow = props.persons.len().saturating_sub(props.max_visible);

    let size_class = props.size.class();
    let text_class = props.size.text_class();

    rsx! {
        div {
            class: "flex items-center",

            // Overlapping avatars
            div {
                class: "flex -space-x-2",
                for (idx, person) in visible.iter().enumerate() {
                    PersonAvatar {
                        key: "{idx}",
                        person: (*person).clone(),
                        size: props.size
                    }
                }

                // Overflow indicator
                if overflow > 0 {
                    div {
                        class: "{size_class} rounded-full flex items-center justify-center bg-muted ring-2 ring-background {text_class} font-medium text-muted-foreground",
                        "+{overflow}"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Featured Person (for show page hero)
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct FeaturedPersonProps {
    /// The person to feature
    pub person: Person,
    /// Show extended bio
    #[props(default = false)]
    pub show_bio: bool,
}

/// Large featured person card for podcast show pages
#[component]
pub fn FeaturedPerson(props: FeaturedPersonProps) -> Element {
    let person = &props.person;
    let role_display = person.role.as_deref().unwrap_or("Host");

    rsx! {
        div {
            class: "flex items-start gap-4 p-4 rounded-xl bg-muted/50",

            // Large avatar
            PersonAvatar {
                person: person.clone(),
                size: AvatarSize::Large
            }

            // Info
            div {
                class: "flex-1",

                // Name
                h3 {
                    class: "font-bold text-lg",
                    "{person.name}"
                }

                // Role badge
                span {
                    class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs bg-primary/10 text-primary capitalize mb-2",
                    "{role_display}"
                }

                // Links
                if let Some(ref href) = person.href {
                    a {
                        href: "{href}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "flex items-center gap-1 text-sm text-primary hover:underline",
                        "View Profile"
                        span {
                            class: "w-3 h-3",
                            dangerous_inner_html: icons::EXTERNAL_LINK
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Inline Credits (for player)
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct InlineCreditsProps {
    /// Persons to display
    pub persons: Vec<Person>,
}

/// Compact inline credits display for the music player
#[component]
pub fn InlineCredits(props: InlineCreditsProps) -> Element {
    if props.persons.is_empty() {
        return rsx! {};
    }

    // Show just hosts/main contributors inline
    let main_persons: Vec<_> = props
        .persons
        .iter()
        .filter(|p| {
            let role = p.role.as_deref().unwrap_or("").to_lowercase();
            role.contains("host") || role.is_empty()
        })
        .take(2)
        .collect();

    if main_persons.is_empty() {
        return rsx! {};
    }

    let names: Vec<&str> = main_persons.iter().map(|p| p.name.as_str()).collect();
    let display = names.join(" & ");

    rsx! {
        div {
            class: "flex items-center gap-2 text-xs text-muted-foreground",
            span {
                class: "w-3 h-3",
                dangerous_inner_html: icons::MIC
            }
            span {
                class: "truncate",
                "with {display}"
            }
        }
    }
}
