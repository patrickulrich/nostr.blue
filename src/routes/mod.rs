use dioxus::prelude::*;

pub mod home;
pub mod profile;
pub mod note;
pub mod settings;
pub mod settings_blocklist;
pub mod settings_muted;
pub mod notifications;
pub mod bookmarks;
pub mod dms;
pub mod explore;
pub mod trending;
pub mod hashtag;
pub mod nip19;
pub mod videos;
pub mod video_detail;
pub mod videos_live;
pub mod videos_live_tag;
pub mod live_stream_detail;
pub mod live_stream_new;
pub mod articles;
pub mod article_detail;
pub mod music;
pub mod note_new;
pub mod article_new;
pub mod photo_new;
pub mod video_new_landscape;
pub mod video_new_portrait;
pub mod search;

// Placeholder modules for missing routes
mod lists;
pub mod dvm;
pub mod photos;
pub mod photo_detail;
pub mod voicemessages;
pub mod voice_message_new;
pub mod voice_message_detail;
pub mod webbookmarks;
pub mod polls;
pub mod poll_view;
pub mod poll_new;
pub mod cashu_wallet;
pub mod terms;
pub mod privacy;
pub mod cookies;
pub mod about;

use home::Home;
use profile::Profile;
use note::Note;
use settings::Settings;
use settings_blocklist::SettingsBlocklist;
use settings_muted::SettingsMuted;
use notifications::Notifications;
use bookmarks::Bookmarks;
use dms::DMs;
use explore::Explore;
use trending::Trending;
use hashtag::Hashtag;
use nip19::Nip19Handler;
use videos::Videos;
use video_detail::VideoDetail;
use videos_live::VideosLive;
use videos_live_tag::VideosLiveTag;
use live_stream_detail::LiveStreamDetail;
use live_stream_new::LiveStreamNew;
use articles::Articles;
use article_detail::ArticleDetail;
use music::{MusicHome, MusicRadio, MusicLeaderboard, MusicArtist, MusicAlbum, MusicSearch, MusicTrackNew, MusicPlaylistNew, MusicPlaylistDetail};
use photos::Photos;
use photo_detail::PhotoDetail;
use voicemessages::VoiceMessages;
use voice_message_new::VoiceMessageNew;
use voice_message_detail::VoiceMessageDetail;
use webbookmarks::WebBookmarks;
use polls::Polls;
use poll_view::PollView;
use poll_new::PollNew;
use cashu_wallet::CashuWallet;
use note_new::NoteNew;
use article_new::ArticleNew;
use photo_new::PhotoNew;
use video_new_landscape::VideoNewLandscape;
use video_new_portrait::VideoNewPortrait;
use lists::Lists;
use dvm::DVM;
use terms::Terms;
use privacy::Privacy;
use cookies::Cookies;
use about::About;
use search::Search;

/// App routes
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Layout)]
        #[route("/")]
        Home {},

        #[route("/explore")]
        Explore {},

        #[route("/trending")]
        Trending {},

        #[route("/search?:q")]
        Search { q: String },

        #[route("/articles")]
        Articles {},

        #[route("/articles/:naddr")]
        ArticleDetail { naddr: String },

        #[route("/videos")]
        Videos {},

        #[route("/videos/:video_id")]
        VideoDetail { video_id: String },

        #[route("/videos/live")]
        VideosLive {},

        #[route("/videos/live/tag/:tag")]
        VideosLiveTag { tag: String },

        #[route("/videos/live/new")]
        LiveStreamNew {},

        #[route("/videos/live/:note_id")]
        LiveStreamDetail { note_id: String },

        #[route("/music")]
        MusicHome {},

        #[route("/music/radio")]
        MusicRadio {},

        #[route("/music/leaderboard")]
        MusicLeaderboard {},

        #[route("/music/artist/:artist_id")]
        MusicArtist { artist_id: String },

        #[route("/music/album/:album_id")]
        MusicAlbum { album_id: String },

        #[route("/music/search?:q")]
        MusicSearch { q: String },

        #[route("/music/track/new")]
        MusicTrackNew {},

        #[route("/music/playlist/new")]
        MusicPlaylistNew {},

        #[route("/music/playlist/:naddr")]
        MusicPlaylistDetail { naddr: String },

        #[route("/notifications")]
        Notifications {},

        #[route("/bookmarks")]
        Bookmarks {},

        #[route("/dms")]
        DMs {},

        #[route("/photos")]
        Photos {},

        #[route("/photos/:photo_id")]
        PhotoDetail { photo_id: String },

        #[route("/voicemessages")]
        VoiceMessages {},

        #[route("/voicemessages/new")]
        VoiceMessageNew {},

        #[route("/voicemessages/:voice_id")]
        VoiceMessageDetail { voice_id: String },

        #[route("/webbookmarks")]
        WebBookmarks {},

        #[route("/polls")]
        Polls {},

        #[route("/polls/new")]
        PollNew {},

        #[route("/polls/:noteid")]
        PollView { noteid: String },

        #[route("/cashuwallet")]
        CashuWallet {},

        #[route("/notes/new?:quote")]
        NoteNew { quote: Option<String> },

        #[route("/articles/new")]
        ArticleNew {},

        #[route("/photos/new")]
        PhotoNew {},

        #[route("/videos/new/landscape")]
        VideoNewLandscape {},

        #[route("/videos/new/portrait")]
        VideoNewPortrait {},

        #[route("/lists")]
        Lists {},

        #[route("/dvm")]
        DVM {},

        #[route("/profile/:pubkey")]
        Profile { pubkey: String },

        #[route("/note/:note_id?:from_voice")]
        Note { note_id: String, from_voice: Option<String> },

        #[route("/t/:tag")]
        Hashtag { tag: String },

        #[route("/id/:identifier")]
        Nip19Handler { identifier: String },

        #[route("/settings")]
        Settings {},

        #[route("/settings/blocklist")]
        SettingsBlocklist {},

        #[route("/settings/muted")]
        SettingsMuted {},

        #[route("/terms")]
        Terms {},

        #[route("/privacy")]
        Privacy {},

        #[route("/cookies")]
        Cookies {},

        #[route("/about")]
        About {},
}

#[component]
fn Layout() -> Element {
    use crate::stores::{auth_store, notifications as notif_store};

    let auth = auth_store::AUTH_STATE.read();
    let notif_count = use_memo(move || notif_store::get_unread_count());
    let mut sidebar_open = use_signal(|| false);
    let mut more_menu_open = use_signal(|| false);
    let mut radial_menu_open = use_signal(|| false);
    let current_route = use_route::<Route>();
    let navigator = navigator();

    // Check if we're on the DMs, Videos, Wallet, or Music pages (hide right sidebar)
    let is_dms_page = matches!(current_route, Route::DMs {});
    let is_videos_page = matches!(current_route, Route::Videos {} | Route::VideoDetail { .. } | Route::VideosLive {} | Route::VideosLiveTag { .. } | Route::LiveStreamDetail { .. });
    let is_wallet_page = matches!(current_route, Route::CashuWallet {});
    let is_music_page = matches!(current_route, Route::MusicHome {} | Route::MusicRadio {} | Route::MusicLeaderboard {} | Route::MusicSearch { .. } | Route::MusicArtist { .. } | Route::MusicAlbum { .. } | Route::MusicTrackNew {} | Route::MusicPlaylistNew {} | Route::MusicPlaylistDetail { .. });

    // Check if we're on any creation pages (hide right sidebar for better editor space)
    let is_creation_page = matches!(
        current_route,
        Route::NoteNew { .. }
        | Route::ArticleNew {}
        | Route::PhotoNew {}
        | Route::VideoNewLandscape {}
        | Route::VideoNewPortrait {}
        | Route::LiveStreamNew {}
    );

    // Check if we're on home page for home button styling
    let is_home_page = matches!(current_route, Route::Home {});
    let home_font_weight = if is_home_page { "font-bold" } else { "" };

    rsx! {
        div {
            class: "min-h-screen bg-background transition-colors",
            // Close more menu when clicking outside
            onclick: move |_| {
                if *more_menu_open.read() {
                    more_menu_open.set(false);
                }
            },

            // 3-Column Layout Container
            div {
                class: "flex justify-center max-w-[1600px] mx-auto",

                // Left Sidebar (Navigation)
                aside {
                    class: "w-[275px] flex-shrink-0 border-r border-border sticky top-0 h-screen hidden lg:block bg-background",
                    div {
                        class: "h-full flex flex-col p-4 overflow-y-auto",

                        // Logo
                        div {
                            class: "flex items-center gap-2 hover:opacity-80 transition mb-6 cursor-pointer",
                            onclick: move |_| {
                                if is_home_page {
                                    // Already on home page, scroll to top
                                    if let Some(window) = web_sys::window() {
                                        let _ = window.scroll_to_with_x_and_y(0.0, 0.0);
                                    }
                                } else {
                                    // Navigate to home
                                    navigator.push(Route::Home {});
                                }
                            },
                            div {
                                class: "w-12 h-12 bg-blue-500 hover:bg-blue-600 rounded-full flex items-center justify-center text-white font-bold text-xl transition",
                                "N"
                            }
                        }

                        // Navigation Menu
                        nav {
                            class: "flex flex-col gap-1",

                            // Home button with scroll-to-top functionality
                            div {
                                class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                onclick: move |_| {
                                    if is_home_page {
                                        // Already on home page, scroll to top
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.scroll_to_with_x_and_y(0.0, 0.0);
                                        }
                                    } else {
                                        // Navigate to home
                                        navigator.push(Route::Home {});
                                    }
                                },
                                crate::components::icons::HomeIcon { class: "w-7 h-7" }
                                span {
                                    "Home"
                                }
                            }

                            NavLink {
                                to: Route::Explore {},
                                icon: rsx! { crate::components::icons::CompassIcon { class: "w-7 h-7" } },
                                label: "Explore"
                            }

                            NavLink {
                                to: Route::Articles {},
                                icon: rsx! { crate::components::icons::BookOpenIcon { class: "w-7 h-7" } },
                                label: "Articles"
                            }

                            NavLink {
                                to: Route::MusicHome {},
                                icon: rsx! {
                                    svg {
                                        class: "w-7 h-7",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        width: "24",
                                        height: "24",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M9 18V5l12-2v13" }
                                        circle { cx: "6", cy: "18", r: "3" }
                                        circle { cx: "18", cy: "16", r: "3" }
                                    }
                                },
                                label: "Music"
                            }

                            // Show authenticated nav items
                            if auth.is_authenticated {
                                NavLink {
                                    to: Route::Photos {},
                                    icon: rsx! { crate::components::icons::CameraIcon { class: "w-7 h-7" } },
                                    label: "Photos"
                                }
                                NavLink {
                                    to: Route::Videos {},
                                    icon: rsx! { crate::components::icons::VideoIcon { class: "w-7 h-7" } },
                                    label: "Videos"
                                }
                                NavLink {
                                    to: Route::VideosLive {},
                                    icon: rsx! {
                                        svg {
                                            class: "w-7 h-7",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            fill: "none",
                                            view_box: "0 0 24 24",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            path { d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" }
                                        }
                                    },
                                    label: "Live"
                                }
                                NavLink {
                                    to: Route::Notifications {},
                                    icon: rsx! { crate::components::icons::BellIcon { class: "w-7 h-7" } },
                                    label: "Notifications",
                                    badge: Some(*notif_count.read())
                                }
                                NavLink {
                                    to: Route::DMs {},
                                    icon: rsx! { crate::components::icons::MailIcon { class: "w-7 h-7" } },
                                    label: "Messages"
                                }
                                NavLink {
                                    to: Route::Bookmarks {},
                                    icon: rsx! { crate::components::icons::BookmarkIcon { class: "w-7 h-7" } },
                                    label: "Bookmarks"
                                }

                                // Profile link with pubkey
                                if let Some(pubkey) = &auth.pubkey {
                                    NavLink {
                                        to: Route::Profile { pubkey: pubkey.clone() },
                                        icon: rsx! { crate::components::icons::UserIcon { class: "w-7 h-7" } },
                                        label: "Profile"
                                    }
                                }

                                NavLink {
                                    to: Route::Settings {},
                                    icon: rsx! { crate::components::icons::SettingsIcon { class: "w-7 h-7" } },
                                    label: "Settings"
                                }
                            }

                            // More button
                            div {
                                class: "relative",
                                button {
                                    class: "flex items-center justify-start gap-4 px-4 py-6 rounded-full hover:bg-accent transition text-xl w-full",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        let is_open = *more_menu_open.read();
                                        more_menu_open.set(!is_open);
                                    },
                                    crate::components::icons::MoreHorizontalIcon { class: "w-7 h-7" }
                                    span {
                                        "More"
                                    }
                                }

                                // Popup menu
                                if *more_menu_open.read() {
                                    div {
                                        class: "absolute left-0 bottom-full mb-2 bg-card border border-border rounded-lg shadow-lg min-w-[240px] overflow-hidden z-50",
                                        div {
                                            class: "flex flex-col",
                                            Link {
                                                to: Route::VoiceMessages {},
                                                onclick: move |_| more_menu_open.set(false),
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                svg {
                                                    class: "w-5 h-5",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                                                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                                                    line { x1: "12", x2: "12", y1: "19", y2: "22" }
                                                }
                                                span {
                                                    "Voice Messages"
                                                }
                                            }
                                            Link {
                                                to: Route::Polls {},
                                                onclick: move |_| more_menu_open.set(false),
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                svg {
                                                    class: "w-5 h-5",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                                                    line { x1: "3", y1: "9", x2: "21", y2: "9" }
                                                    line { x1: "9", y1: "21", x2: "9", y2: "9" }
                                                }
                                                span {
                                                    "Polls"
                                                }
                                            }
                                            Link {
                                                to: Route::WebBookmarks {},
                                                onclick: move |_| more_menu_open.set(false),
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                svg {
                                                    class: "w-5 h-5",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z" }
                                                }
                                                span {
                                                    "Web Bookmarks"
                                                }
                                            }
                                            Link {
                                                to: Route::CashuWallet {},
                                                onclick: move |_| more_menu_open.set(false),
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                svg {
                                                    class: "w-5 h-5",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    // Wallet icon
                                                    path { d: "M21 12V7H5a2 2 0 0 1 0-4h14v4" }
                                                    path { d: "M3 5v14a2 2 0 0 0 2 2h16v-5" }
                                                    path { d: "M18 12a2 2 0 0 0 0 4h4v-4Z" }
                                                }
                                                span {
                                                    "Wallet"
                                                }
                                            }
                                            a {
                                                href: "https://nostrcal.com",
                                                target: "_blank",
                                                rel: "noopener noreferrer",
                                                onclick: move |_| more_menu_open.set(false),
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                crate::components::icons::CalendarIcon { class: "w-5 h-5" }
                                                span {
                                                    "nostrcal"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Post Button (if authenticated)
                        if auth.is_authenticated {
                            div {
                                class: "relative w-full mt-4",

                                button {
                                    class: "w-full py-6 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-full transition text-lg flex items-center justify-center gap-2 relative z-50",
                                    onclick: move |_| {
                                        let is_open = *radial_menu_open.read();
                                        radial_menu_open.set(!is_open);
                                    },
                                    crate::components::icons::PenSquareIcon { class: "w-6 h-6" }
                                    span { "Post" }
                                }

                                // Radial Menu
                                crate::components::RadialMenu {
                                    is_open: *radial_menu_open.read(),
                                    on_close: move |_| radial_menu_open.set(false),
                                    on_note_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::NoteNew { quote: None });
                                    },
                                    on_article_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::ArticleNew {});
                                    },
                                    on_photo_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::PhotoNew {});
                                    },
                                    on_video_landscape_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::VideoNewLandscape {});
                                    },
                                    on_video_portrait_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::VideoNewPortrait {});
                                    },
                                    on_voice_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::VoiceMessageNew {});
                                    },
                                    on_poll_click: move |_| {
                                        radial_menu_open.set(false);
                                        navigator.push(Route::PollNew {});
                                    },
                                }
                            }
                        }
                    }
                }

                // Mobile Sidebar Overlay
                if *sidebar_open.read() {
                    div {
                        class: "fixed inset-0 bg-black/50 z-40 lg:hidden",
                        onclick: move |_| sidebar_open.set(false),

                        aside {
                            class: "w-64 bg-white dark:bg-gray-900 h-full",
                            onclick: move |e| e.stop_propagation(),
                            div {
                                class: "p-4 space-y-6",

                                // Close button
                                button {
                                    class: "mb-4 p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800",
                                    onclick: move |_| sidebar_open.set(false),
                                    "✕ Close"
                                }

                                // Same navigation as desktop
                                div {
                                    class: "flex items-center gap-2 hover:opacity-80 transition mb-8 cursor-pointer",
                                    onclick: move |_| {
                                        sidebar_open.set(false);
                                        if is_home_page {
                                            // Already on home page, scroll to top
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.scroll_to_with_x_and_y(0.0, 0.0);
                                            }
                                        } else {
                                            // Navigate to home
                                            navigator.push(Route::Home {});
                                        }
                                    },
                                    div {
                                        class: "w-10 h-10 bg-blue-600 rounded-full flex items-center justify-center text-white font-bold text-xl",
                                        "N"
                                    }
                                    span {
                                        class: "text-2xl font-bold text-gray-900 dark:text-white",
                                        "nostr.blue"
                                    }
                                }

                                nav {
                                    class: "flex flex-col gap-2",
                                    // Home button with scroll-to-top functionality
                                    div {
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                        onclick: move |_| {
                                            sidebar_open.set(false);
                                            if is_home_page {
                                                // Already on home page, scroll to top
                                                if let Some(window) = web_sys::window() {
                                                    let _ = window.scroll_to_with_x_and_y(0.0, 0.0);
                                                }
                                            } else {
                                                // Navigate to home
                                                navigator.push(Route::Home {});
                                            }
                                        },
                                        crate::components::icons::HomeIcon { class: "w-7 h-7" }
                                        span {
                                            "Home"
                                        }
                                    }

                                    div {
                                        onclick: move |_| sidebar_open.set(false),
                                        NavLink {
                                            to: Route::Explore {},
                                            icon: rsx! { crate::components::icons::CompassIcon { class: "w-7 h-7" } },
                                            label: "Explore"
                                        }
                                    }

                                    div {
                                        onclick: move |_| sidebar_open.set(false),
                                        NavLink {
                                            to: Route::Articles {},
                                            icon: rsx! { crate::components::icons::BookOpenIcon { class: "w-7 h-7" } },
                                            label: "Articles"
                                        }
                                    }

                                    if auth.is_authenticated {
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::Photos {},
                                                icon: rsx! { crate::components::icons::CameraIcon { class: "w-7 h-7" } },
                                                label: "Photos"
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::Videos {},
                                                icon: rsx! { crate::components::icons::VideoIcon { class: "w-7 h-7" } },
                                                label: "Videos"
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::VideosLive {},
                                                icon: rsx! {
                                                    svg {
                                                        class: "w-7 h-7",
                                                        xmlns: "http://www.w3.org/2000/svg",
                                                        fill: "none",
                                                        view_box: "0 0 24 24",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        path { d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" }
                                                    }
                                                },
                                                label: "Live"
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::Notifications {},
                                                icon: rsx! { crate::components::icons::BellIcon { class: "w-7 h-7" } },
                                                label: "Notifications",
                                                badge: Some(*notif_count.read())
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::DMs {},
                                                icon: rsx! { crate::components::icons::MailIcon { class: "w-7 h-7" } },
                                                label: "Messages"
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::MusicHome {},
                                                icon: rsx! {
                                                    svg {
                                                        class: "w-7 h-7",
                                                        xmlns: "http://www.w3.org/2000/svg",
                                                        width: "24",
                                                        height: "24",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        path { d: "M9 18V5l12-2v13" }
                                                        circle { cx: "6", cy: "18", r: "3" }
                                                        circle { cx: "18", cy: "16", r: "3" }
                                                    }
                                                },
                                                label: "Music"
                                            }
                                        }
                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::Bookmarks {},
                                                icon: rsx! { crate::components::icons::BookmarkIcon { class: "w-7 h-7" } },
                                                label: "Bookmarks"
                                            }
                                        }

                                        if let Some(pubkey) = &auth.pubkey {
                                            div {
                                                onclick: move |_| sidebar_open.set(false),
                                                NavLink {
                                                    to: Route::Profile { pubkey: pubkey.clone() },
                                                    icon: rsx! { crate::components::icons::UserIcon { class: "w-7 h-7" } },
                                                    label: "Profile"
                                                }
                                            }
                                        }

                                        div {
                                            onclick: move |_| sidebar_open.set(false),
                                            NavLink {
                                                to: Route::Settings {},
                                                icon: rsx! { crate::components::icons::SettingsIcon { class: "w-7 h-7" } },
                                                label: "Settings"
                                            }
                                        }
                                    }

                                    // More button (mobile)
                                    div {
                                        class: "relative",
                                        button {
                                            class: "flex items-center gap-4 px-4 py-3 rounded-full hover:bg-accent transition text-xl w-full",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                let is_open = *more_menu_open.read();
                                                more_menu_open.set(!is_open);
                                            },
                                            crate::components::icons::MoreHorizontalIcon {
                                                class: "w-7 h-7".to_string()
                                            }
                                            span {
                                                "More"
                                            }
                                        }

                                        // Popup menu (mobile)
                                        if *more_menu_open.read() {
                                            div {
                                                class: "absolute left-0 top-full mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[240px] overflow-hidden z-50",
                                                div {
                                                    class: "flex flex-col",
                                                    if auth.is_authenticated {
                                                        Link {
                                                            to: Route::CashuWallet {},
                                                            onclick: move |_| {
                                                                more_menu_open.set(false);
                                                                sidebar_open.set(false);
                                                            },
                                                            class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                            svg {
                                                                class: "w-5 h-5",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "M21 12V7H5a2 2 0 0 1 0-4h14v4" }
                                                                path { d: "M3 5v14a2 2 0 0 0 2 2h16v-5" }
                                                                path { d: "M18 12a2 2 0 0 0 0 4h4v-4Z" }
                                                            }
                                                            span {
                                                                "Wallet"
                                                            }
                                                        }
                                                        Link {
                                                            to: Route::VoiceMessages {},
                                                            onclick: move |_| {
                                                                more_menu_open.set(false);
                                                                sidebar_open.set(false);
                                                            },
                                                            class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                            svg {
                                                                class: "w-5 h-5",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                                                                path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                                                                line { x1: "12", x2: "12", y1: "19", y2: "22" }
                                                            }
                                                            span {
                                                                "Voice Messages"
                                                            }
                                                        }
                                                        Link {
                                                            to: Route::Polls {},
                                                            onclick: move |_| {
                                                                more_menu_open.set(false);
                                                                sidebar_open.set(false);
                                                            },
                                                            class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                            svg {
                                                                class: "w-5 h-5",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                                                                line { x1: "3", y1: "9", x2: "21", y2: "9" }
                                                                line { x1: "9", y1: "21", x2: "9", y2: "9" }
                                                            }
                                                            span {
                                                                "Polls"
                                                            }
                                                        }
                                                        Link {
                                                            to: Route::WebBookmarks {},
                                                            onclick: move |_| {
                                                                more_menu_open.set(false);
                                                                sidebar_open.set(false);
                                                            },
                                                            class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                            svg {
                                                                class: "w-5 h-5",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z" }
                                                            }
                                                            span {
                                                                "Web Bookmarks"
                                                            }
                                                        }
                                                    }
                                                    a {
                                                        href: "https://nostrcal.com",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        onclick: move |_| {
                                                            more_menu_open.set(false);
                                                            sidebar_open.set(false);
                                                        },
                                                        class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                        crate::components::icons::CalendarIcon {
                                                            class: "w-5 h-5".to_string()
                                                        }
                                                        span {
                                                            "nostrcal"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Center Content Area
                main {
                    class: if is_dms_page || is_videos_page || is_wallet_page || is_music_page || is_creation_page {
                        "w-full flex-1 border-r border-border"
                    } else {
                        "w-full max-w-[600px] flex-shrink flex-grow border-r border-border"
                    },

                    // Mobile header
                    div {
                        class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border p-4 lg:hidden",
                        div {
                            class: "flex items-center justify-between",
                            button {
                                class: "p-2 hover:bg-accent rounded-lg",
                                onclick: move |_| sidebar_open.set(true),
                                "☰ Menu"
                            }
                            div {
                                class: "text-lg font-bold",
                                "nostr.blue"
                            }
                            div {
                                class: "w-10"
                            }
                        }
                    }

                    // Page Content
                    Outlet::<Route> {}
                }

                // Right Sidebar (Trending & Search) - Hidden on DMs, Videos, and Wallet pages
                if !is_dms_page && !is_videos_page && !is_wallet_page && !is_music_page && !is_creation_page {
                    aside {
                        class: "w-[350px] flex-shrink-0 hidden xl:block",
                    div {
                        class: "flex flex-col gap-4 sticky top-0 pt-4 pb-4 h-screen overflow-hidden px-4 z-0",

                        // Search Input
                        div {
                            class: "flex-shrink-0",
                            crate::components::SearchInput {}
                        }

                        // Trending Notes
                        div {
                            class: "flex-1 overflow-hidden",
                            crate::components::TrendingNotes {}
                        }

                        // Footer Links
                        div {
                            class: "text-xs text-muted-foreground flex flex-wrap gap-2 mt-auto flex-shrink-0",
                            Link {
                                to: Route::Terms {},
                                class: "hover:underline",
                                "Terms of Service"
                            }
                            span { "·" }
                            Link {
                                to: Route::Privacy {},
                                class: "hover:underline",
                                "Privacy Policy"
                            }
                            span { "·" }
                            Link {
                                to: Route::Cookies {},
                                class: "hover:underline",
                                "Cookie Policy"
                            }
                            span { "·" }
                            Link {
                                to: Route::About {},
                                class: "hover:underline",
                                "About"
                            }
                            div {
                                class: "w-full mt-1",
                                "2025 nostr.blue - {env!(\"CARGO_PKG_VERSION\")}"
                            }
                        }
                    }
                    }
                }
            }

            // Global persistent music player
            crate::components::PersistentMusicPlayer {}

            // Global zap dialog (rendered at layout level to escape music player's stacking context)
            crate::components::MusicZapDialog {}
        }
    }
}

// Navigation Link Component
#[component]
fn NavLink(
    to: Route,
    icon: Element,
    label: &'static str,
    #[props(default = None)] badge: Option<usize>
) -> Element {
    let current_route = use_route::<Route>();

    // Check if this is the active route
    let is_active = match (&to, &current_route) {
        (Route::Home {}, Route::Home {}) => true,
        (Route::Explore {}, Route::Explore {}) => true,
        (Route::Articles {}, Route::Articles {}) => true,
        (Route::Notifications {}, Route::Notifications {}) => true,
        (Route::DMs {}, Route::DMs {}) => true,
        (Route::Photos {}, Route::Photos {}) => true,
        (Route::PhotoDetail { photo_id: p1 }, Route::PhotoDetail { photo_id: p2 }) => p1 == p2,
        (Route::MusicHome {}, Route::MusicHome {}) |
        (Route::MusicHome {}, Route::MusicRadio {}) |
        (Route::MusicHome {}, Route::MusicLeaderboard {}) |
        (Route::MusicHome {}, Route::MusicSearch { .. }) |
        (Route::MusicHome {}, Route::MusicArtist { .. }) |
        (Route::MusicHome {}, Route::MusicAlbum { .. }) |
        (Route::MusicHome {}, Route::MusicTrackNew {}) |
        (Route::MusicHome {}, Route::MusicPlaylistNew {}) |
        (Route::MusicHome {}, Route::MusicPlaylistDetail { .. }) => true,
        (Route::Bookmarks {}, Route::Bookmarks {}) => true,
        (Route::Videos {}, Route::Videos {}) => true,
        (Route::VideoDetail { video_id: v1 }, Route::VideoDetail { video_id: v2 }) => v1 == v2,
        (Route::VideosLive {}, Route::VideosLive {}) |
        (Route::VideosLive {}, Route::VideosLiveTag { .. }) |
        (Route::VideosLive {}, Route::LiveStreamDetail { .. }) => true,
        (Route::VideosLiveTag { tag: t1 }, Route::VideosLiveTag { tag: t2 }) => t1 == t2,
        (Route::LiveStreamDetail { note_id: n1 }, Route::LiveStreamDetail { note_id: n2 }) => n1 == n2,
        (Route::LiveStreamNew {}, Route::LiveStreamNew {}) => true,
        (Route::CashuWallet {}, Route::CashuWallet {}) => true,
        (Route::Settings {}, Route::Settings {}) => true,
        (Route::Profile { pubkey: p1 }, Route::Profile { pubkey: p2 }) => p1 == p2,
        _ => false,
    };

    let font_class = if is_active { "font-bold" } else { "" };

    rsx! {
        Link {
            to: to,
            class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full {font_class}",
            {icon}
            span {
                "{label}"
            }
            if let Some(count) = badge {
                if count > 0 {
                    span {
                        class: "ml-auto min-w-[24px] h-6 px-2 bg-blue-500 text-white rounded-full text-sm font-bold flex items-center justify-center",
                        "{count}"
                    }
                }
            }
        }
    }
}
