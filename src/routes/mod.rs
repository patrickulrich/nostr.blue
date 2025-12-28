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
mod list_detail;
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
pub mod podcast;
pub mod podcast_nostr_detail;
pub mod podcast_rss_detail;
pub mod podcast_episode_detail;
pub mod podcast_trending;

// Internet Radio (Kind 31237)
pub mod radio;

pub mod nips;
pub mod nip_detail;

// Badges (NIP-58)
pub mod badges;
pub mod badge_detail;
pub mod badge_new;

// Citations (NKBIP-03 Kinds 30-33)
pub mod citations;

// Code/Git hosting (NIP-34 + NIP-C0)
pub mod code;
pub mod code_explore;

// P2P Trading (NIP-69)
pub mod p2p;
pub mod p2p_order_detail;

// Communities (NIP-72)
pub mod communities;
pub mod community;
pub mod community_new;

// Recipes
pub mod recipes;
pub mod recipes_all;
pub mod recipe_detail;
pub mod recipe_new;
pub mod recipe_fork;
pub mod recipes_by_tag;
pub mod recipe_chef;

// Pin Boards (Kind 33889 Pinstr-compatible)
pub mod pin_boards;
pub mod pin_board_detail;
pub mod pin_board_new;

// Wiki (NIP-54 Kind 30818)
pub mod wiki;
pub mod wiki_author;
pub mod wiki_detail;
pub mod wiki_new;

// Publications (NKBIP-01 Kind 30040/30041)
pub mod publications;
pub mod publication_detail;
pub mod publication_new;
pub mod publication_search;

// Calendar/Events (NIP-52 + NIP-53)
pub mod events;
pub mod event_detail;
pub mod calendar;
pub mod calendar_event_new;

// Shop/Marketplace (NIP-99)
pub mod shop;
pub mod shop_product;
pub mod shop_product_new;
pub mod shop_product_edit;
pub mod shop_cart;
pub mod shop_checkout;
pub mod shop_orders;
pub mod shop_merchant;
pub mod shop_merchant_orders;
pub mod shop_collection;
pub mod shop_collection_new;
pub mod shop_search;
pub mod code_repositories;
pub mod code_snippets;
pub mod code_snippet_detail;
pub mod code_snippet_new;
pub mod code_import;
pub mod code_search;
pub mod code_repo;
pub mod code_repo_commits;
pub mod code_repo_issues;
pub mod code_repo_pulls;
pub mod code_issue_detail;
pub mod code_issue_new;
pub mod code_pull_detail;
pub mod code_pull_new;
pub mod code_repo_settings;
pub mod code_repo_tree;
pub mod code_repo_blob;

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
use music::{MusicHome, MusicRadio, MusicLeaderboard, MusicArtist, MusicAlbum, MusicSearch, MusicTrackNew, MusicPlaylistNew, MusicPlaylistDetail, MusicRssAlbum};
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
use list_detail::ListDetail;
use dvm::DVM;
use terms::Terms;
use privacy::Privacy;
use cookies::Cookies;
use about::About;
use search::Search;
use podcast::PodcastHome;
use podcast_nostr_detail::PodcastNostrDetail;
use podcast_rss_detail::PodcastRssFeedDetail;
use podcast_episode_detail::{PodcastNostrEpisodeDetail, PodcastRssEpisodeDetail};
use podcast_trending::PodcastTrending;
use radio::{RadioHome, RadioStation, RadioStationNew};
use nips::{NipsHome, NipNew};
use nip_detail::NipDetail;
use badges::BadgesHome;
use badge_detail::BadgeDetail;
use badge_new::BadgeNew;
use citations::{CitationsHome, CitationDetail};
use code::CodeHome;
use code_explore::CodeExplore;
use code_repositories::CodeRepositories;
use code_snippets::CodeSnippets;
use code_snippet_detail::CodeSnippetDetail;
use code_snippet_new::CodeSnippetNew;
use code_import::CodeImport;
use code_search::CodeSearch;
use code_repo::CodeRepo;
use code_repo_commits::CodeRepoCommits;
use code_repo_issues::CodeRepoIssues;
use code_repo_pulls::CodeRepoPulls;
use code_issue_detail::CodeIssueDetail;
use code_issue_new::CodeIssueNew;
use code_pull_detail::CodePullDetail;
use code_pull_new::CodePullNew;
use code_repo_settings::CodeRepoSettings;
use code_repo_tree::CodeRepoTree;
use code_repo_blob::CodeRepoBlob;
use p2p::P2PHome;
use p2p_order_detail::P2POrderDetail;
use communities::Communities;
use community::CommunityPage;
use community_new::CommunityNew;
use recipes::RecipesHome;
use recipes_all::RecipesAll;
use recipe_detail::RecipeDetail;
use recipe_new::RecipeNew;
use recipe_fork::RecipeFork;
use recipes_by_tag::RecipesByTag;
use recipe_chef::RecipeChef;
use pin_boards::PinBoardsHome;
use pin_board_detail::PinBoardDetail;
use pin_board_new::{PinBoardNew, PinBoardEdit};
use wiki::WikiHome;
use wiki_author::WikiAuthor;
use wiki_detail::WikiDetail;
use wiki_new::WikiNew;
use publications::PublicationsHome;
use publication_detail::PublicationDetail;
use publication_new::PublicationNew;
use publication_search::PublicationSearch;
use events::Events;
use event_detail::CalendarEventDetail;
use calendar::Calendar;
use calendar_event_new::CalendarEventNew;
use shop::ShopHome;
use shop_product::ShopProductDetail;
use shop_product_new::ShopProductNew;
use shop_product_edit::ShopProductEdit;
use shop_cart::ShopCart;
use shop_checkout::ShopCheckout;
use shop_orders::ShopOrders;
use shop_merchant::ShopMerchant;
use shop_merchant_orders::ShopMerchantOrders;
use shop_collection::ShopCollection;
use shop_collection_new::ShopCollectionNew;
use shop_search::ShopSearch;

/// App routes
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
#[allow(clippy::upper_case_acronyms)]
pub enum Route {
    #[layout(Layout)]
        #[route("/?:list")]
        Home { list: String },

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

        #[route("/music/rss/album/:feed_id")]
        MusicRssAlbum { feed_id: u64 },

        #[route("/podcast")]
        PodcastHome {},

        #[route("/podcast/trending")]
        PodcastTrending {},

        #[route("/podcast/nostr/:naddr")]
        PodcastNostrDetail { naddr: String },

        #[route("/podcast/rss/:podcast_id")]
        PodcastRssFeedDetail { podcast_id: String },

        #[route("/podcast/nostr/episode/:naddr")]
        PodcastNostrEpisodeDetail { naddr: String },

        #[route("/podcast/rss/episode/:podcast_id/:episode_id")]
        PodcastRssEpisodeDetail { podcast_id: String, episode_id: String },

        // Internet Radio routes (Kind 31237)
        #[route("/radio")]
        RadioHome {},

        #[route("/radio/new")]
        RadioStationNew {},

        #[route("/radio/:naddr")]
        RadioStation { naddr: String },

        #[route("/nips")]
        NipsHome {},

        #[route("/nips/new")]
        NipNew {},

        #[route("/nips/:nip_id")]
        NipDetail { nip_id: String },

        // Badges (NIP-58)
        #[route("/badges")]
        BadgesHome {},

        #[route("/badges/new")]
        BadgeNew {},

        #[route("/badges/:naddr")]
        BadgeDetail { naddr: String },

        // Citations (NKBIP-03 Kinds 30-33)
        #[route("/citations")]
        CitationsHome {},

        #[route("/citations/:naddr")]
        CitationDetail { naddr: String },

        // Code/Git hosting (NIP-34 + NIP-C0)
        #[route("/code")]
        CodeHome {},

        #[route("/code/explore")]
        CodeExplore {},

        #[route("/code/repositories")]
        CodeRepositories {},

        #[route("/code/snippets")]
        CodeSnippets {},

        #[route("/code/snippets/new")]
        CodeSnippetNew {},

        #[route("/code/snippet/:note_id")]
        CodeSnippetDetail { note_id: String },

        #[route("/code/import")]
        CodeImport {},

        #[route("/code/search?:q")]
        CodeSearch { q: String },

        #[route("/code/repo/:naddr")]
        CodeRepo { naddr: String },

        #[route("/code/repo/:naddr/commits")]
        CodeRepoCommits { naddr: String },

        #[route("/code/repo/:naddr/issues")]
        CodeRepoIssues { naddr: String },

        #[route("/code/repo/:naddr/issues/new")]
        CodeIssueNew { naddr: String },

        #[route("/code/repo/:naddr/pulls")]
        CodeRepoPulls { naddr: String },

        #[route("/code/repo/:naddr/pulls/new")]
        CodePullNew { naddr: String },

        #[route("/code/repo/:naddr/settings")]
        CodeRepoSettings { naddr: String },

        #[route("/code/repo/:naddr/tree/:git_ref/*path")]
        CodeRepoTree { naddr: String, git_ref: String, path: String },

        #[route("/code/repo/:naddr/blob/:git_ref/*path")]
        CodeRepoBlob { naddr: String, git_ref: String, path: String },

        #[route("/code/issue/:note_id")]
        CodeIssueDetail { note_id: String },

        #[route("/code/pull/:note_id")]
        CodePullDetail { note_id: String },

        // P2P Trading (NIP-69)
        #[route("/p2p")]
        P2PHome {},

        #[route("/p2p/order/:naddr")]
        P2POrderDetail { naddr: String },

        // Communities (NIP-72)
        #[route("/communities")]
        Communities {},

        #[route("/communities/new")]
        CommunityNew {},

        #[route("/community/:a_tag")]
        CommunityPage { a_tag: String },

        // Recipes
        #[route("/recipes")]
        RecipesHome {},

        #[route("/recipes/all")]
        RecipesAll {},

        #[route("/recipes/new")]
        RecipeNew {},

        #[route("/recipes/fork/:naddr")]
        RecipeFork { naddr: String },

        #[route("/recipes/tag/:tag")]
        RecipesByTag { tag: String },

        #[route("/recipes/chef/:npub")]
        RecipeChef { npub: String },

        #[route("/recipes/:naddr")]
        RecipeDetail { naddr: String },

        // Pin Boards (Kind 33889)
        #[route("/pinboards")]
        PinBoardsHome {},

        #[route("/pinboards/new")]
        PinBoardNew {},

        #[route("/pinboards/:naddr")]
        PinBoardDetail { naddr: String },

        #[route("/pinboards/:naddr/edit")]
        PinBoardEdit { naddr: String },

        // Wiki (NIP-54)
        #[route("/wiki")]
        WikiHome {},

        #[route("/wiki/new")]
        WikiNew {},

        #[route("/wiki/:identifier")]
        WikiDetail { identifier: String },

        #[route("/wiki/author/:pubkey")]
        WikiAuthor { pubkey: String },

        // Publications (NKBIP-01)
        #[route("/publications")]
        PublicationsHome {},

        #[route("/publications/new")]
        PublicationNew {},

        #[route("/publications/search?:query")]
        PublicationSearch { query: String },

        #[route("/publications/:naddr")]
        PublicationDetail { naddr: String },

        // Calendar/Events (NIP-52 + NIP-53)
        #[route("/events")]
        Events {},

        #[route("/calendar/:naddr?:from")]
        CalendarEventDetail { naddr: String, from: Option<String> },

        #[route("/calendar")]
        Calendar {},

        #[route("/calendar/new")]
        CalendarEventNew {},

        // Shop/Marketplace (NIP-99)
        #[route("/marketplace")]
        ShopHome {},

        #[route("/marketplace/product/:naddr")]
        ShopProductDetail { naddr: String },

        #[route("/marketplace/product/new")]
        ShopProductNew {},

        #[route("/marketplace/product/edit/:naddr")]
        ShopProductEdit { naddr: String },

        #[route("/marketplace/cart")]
        ShopCart {},

        #[route("/marketplace/checkout")]
        ShopCheckout {},

        #[route("/marketplace/orders")]
        ShopOrders {},

        #[route("/marketplace/merchant")]
        ShopMerchant {},

        #[route("/marketplace/merchant/orders")]
        ShopMerchantOrders {},

        #[route("/marketplace/collection/:naddr")]
        ShopCollection { naddr: String },

        #[route("/marketplace/collection/new")]
        ShopCollectionNew {},

        #[route("/marketplace/search?:q")]
        ShopSearch { q: String },

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

        #[route("/lists/:identifier")]
        ListDetail { identifier: String },

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
    let notif_count = use_memo(notif_store::get_unread_count);
    let mut sidebar_open = use_signal(|| false);
    let mut more_menu_open = use_signal(|| false);
    let mut radial_menu_open = use_signal(|| false);
    let mut sidebar_customizer_open = use_signal(|| false);
    let current_route = use_route::<Route>();
    let navigator = navigator();

    // Check if we're on the DMs, Videos, Wallet, or Music pages (hide right sidebar)
    let is_dms_page = matches!(current_route, Route::DMs {});
    let is_videos_page = matches!(current_route, Route::Videos {} | Route::VideoDetail { .. } | Route::VideosLive {} | Route::VideosLiveTag { .. } | Route::LiveStreamDetail { .. });
    let is_wallet_page = matches!(current_route, Route::CashuWallet {});
    let is_music_page = matches!(current_route, Route::MusicHome {} | Route::MusicRadio {} | Route::MusicLeaderboard {} | Route::MusicSearch { .. } | Route::MusicArtist { .. } | Route::MusicAlbum { .. } | Route::MusicRssAlbum { .. } | Route::MusicTrackNew {} | Route::MusicPlaylistNew {} | Route::MusicPlaylistDetail { .. });
    let is_podcast_page = matches!(current_route, Route::PodcastHome {} | Route::PodcastTrending {} | Route::PodcastNostrDetail { .. } | Route::PodcastRssFeedDetail { .. } | Route::PodcastNostrEpisodeDetail { .. } | Route::PodcastRssEpisodeDetail { .. });
    let is_radio_page = matches!(current_route, Route::RadioHome {} | Route::RadioStation { .. } | Route::RadioStationNew {});
    let is_nips_page = matches!(current_route, Route::NipsHome {} | Route::NipDetail { .. } | Route::NipNew {});
    let is_badges_page = matches!(current_route, Route::BadgesHome {} | Route::BadgeDetail { .. } | Route::BadgeNew {});
    let is_code_page = matches!(current_route,
        Route::CodeHome {} | Route::CodeExplore {} | Route::CodeRepositories {} |
        Route::CodeSnippets {} | Route::CodeSnippetDetail { .. } | Route::CodeSnippetNew {} |
        Route::CodeImport {} | Route::CodeSearch { .. } | Route::CodeRepo { .. } |
        Route::CodeRepoCommits { .. } | Route::CodeRepoIssues { .. } | Route::CodeRepoPulls { .. } |
        Route::CodeIssueNew { .. } | Route::CodePullNew { .. } | Route::CodeRepoSettings { .. } |
        Route::CodeIssueDetail { .. } | Route::CodePullDetail { .. } |
        Route::CodeRepoTree { .. } | Route::CodeRepoBlob { .. }
    );
    let is_p2p_page = matches!(current_route,
        Route::P2PHome {} | Route::P2POrderDetail { .. }
    );
    let is_community_page = matches!(current_route,
        Route::Communities {} | Route::CommunityPage { .. }
    );
    let is_events_page = matches!(current_route,
        Route::Events {} | Route::CalendarEventDetail { .. } | Route::Calendar {}
    );
    let is_recipes_page = matches!(current_route,
        Route::RecipesHome {} | Route::RecipesAll {} | Route::RecipeDetail { .. } | Route::RecipeNew {} |
        Route::RecipeFork { .. } | Route::RecipesByTag { .. } | Route::RecipeChef { .. }
    );
    let is_pin_boards_page = matches!(current_route,
        Route::PinBoardsHome {} | Route::PinBoardDetail { .. } | Route::PinBoardNew {} | Route::PinBoardEdit { .. }
    );
    let is_wiki_page = matches!(current_route,
        Route::WikiHome {} | Route::WikiDetail { .. } | Route::WikiNew {} | Route::WikiAuthor { .. }
    );
    let is_publications_page = matches!(current_route,
        Route::PublicationsHome {} | Route::PublicationDetail { .. } | Route::PublicationNew {} | Route::PublicationSearch { .. }
    );
    let is_shop_page = matches!(current_route,
        Route::ShopHome {} | Route::ShopProductDetail { .. } | Route::ShopProductNew {} |
        Route::ShopProductEdit { .. } | Route::ShopCart {} | Route::ShopCheckout {} | Route::ShopOrders {} |
        Route::ShopMerchant {} | Route::ShopMerchantOrders {} | Route::ShopCollection { .. } |
        Route::ShopCollectionNew {} | Route::ShopSearch { .. }
    );
    let is_settings_page = matches!(current_route, Route::Settings {});

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
    let is_home_page = matches!(current_route, Route::Home { .. });
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
                                        window.scroll_to_with_x_and_y(0.0, 0.0);
                                    }
                                } else {
                                    // Navigate to home
                                    navigator.push(Route::Home { list: String::new() });
                                }
                            },
                            div {
                                class: "w-12 h-12 bg-blue-500 hover:bg-blue-600 rounded-full flex items-center justify-center text-white font-bold text-xl transition",
                                "N"
                            }
                        }

                        // Navigation Menu - Dynamic based on user preferences
                        nav {
                            class: "flex flex-col gap-1",

                            // Render main sidebar items dynamically
                            for item in crate::stores::sidebar_store::get_main_sidebar_items(auth.is_authenticated) {
                                {
                                    use crate::stores::sidebar_store::SidebarItem;
                                    match item {
                                        // Home has special scroll-to-top functionality
                                        SidebarItem::Home => rsx! {
                                            div {
                                                key: "{item:?}",
                                                class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                                onclick: move |_| {
                                                    if is_home_page {
                                                        if let Some(window) = web_sys::window() {
                                                            window.scroll_to_with_x_and_y(0.0, 0.0);
                                                        }
                                                    } else {
                                                        navigator.push(Route::Home { list: String::new() });
                                                    }
                                                },
                                                {render_sidebar_icon(&SidebarItem::Home, "w-7 h-7")}
                                                span { "Home" }
                                            }
                                        },
                                        // Profile needs pubkey
                                        SidebarItem::Profile => {
                                            if let Some(pubkey) = &auth.pubkey {
                                                rsx! {
                                                    NavLink {
                                                        key: "{item:?}",
                                                        to: Route::Profile { pubkey: pubkey.clone() },
                                                        icon: render_sidebar_icon(&SidebarItem::Profile, "w-7 h-7"),
                                                        label: "Profile"
                                                    }
                                                }
                                            } else {
                                                rsx! {}
                                            }
                                        },
                                        // Notifications has badge
                                        SidebarItem::Notifications => rsx! {
                                            NavLink {
                                                key: "{item:?}",
                                                to: Route::Notifications {},
                                                icon: render_sidebar_icon(&SidebarItem::Notifications, "w-7 h-7"),
                                                label: "Notifications",
                                                badge: Some(*notif_count.read())
                                            }
                                        },
                                        // All other items use standard NavLink
                                        _ => {
                                            if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                rsx! {
                                                    NavLink {
                                                        key: "{item:?}",
                                                        to: route,
                                                        icon: render_sidebar_icon(&item, "w-7 h-7"),
                                                        label: item.label()
                                                    }
                                                }
                                            } else {
                                                rsx! {}
                                            }
                                        }
                                    }
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

                                // Popup menu - Dynamic based on user preferences
                                if *more_menu_open.read() {
                                    div {
                                        class: "absolute left-0 bottom-full mb-2 bg-card border border-border rounded-lg shadow-lg min-w-[240px] overflow-hidden z-50",
                                        div {
                                            class: "flex flex-col",

                                            // Render More menu items dynamically
                                            for item in crate::stores::sidebar_store::get_more_menu_items(auth.is_authenticated) {
                                                {
                                                    if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                        rsx! {
                                                            Link {
                                                                key: "{item:?}-more",
                                                                to: route,
                                                                onclick: move |_| more_menu_open.set(false),
                                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base",
                                                                {render_sidebar_icon(&item, "w-5 h-5")}
                                                                span { "{item.label()}" }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                }
                                            }

                                            // Divider before Edit Sidebar
                                            div { class: "border-t border-border my-1" }

                                            // Edit Sidebar button
                                            button {
                                                class: "flex items-center gap-4 px-4 py-4 hover:bg-accent transition text-base w-full text-left",
                                                onclick: move |_| {
                                                    more_menu_open.set(false);
                                                    sidebar_customizer_open.set(true);
                                                },
                                                crate::components::icons::SettingsIcon { class: "w-5 h-5" }
                                                span {
                                                    "Edit Sidebar"
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
                                                window.scroll_to_with_x_and_y(0.0, 0.0);
                                            }
                                        } else {
                                            // Navigate to home
                                            navigator.push(Route::Home { list: String::new() });
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

                                    // Render main sidebar items dynamically (mobile)
                                    for item in crate::stores::sidebar_store::get_main_sidebar_items(auth.is_authenticated) {
                                        {
                                            use crate::stores::sidebar_store::SidebarItem;
                                            match item {
                                                // Home has special scroll-to-top functionality
                                                SidebarItem::Home => rsx! {
                                                    div {
                                                        key: "{item:?}-mobile",
                                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                                        onclick: move |_| {
                                                            sidebar_open.set(false);
                                                            if is_home_page {
                                                                if let Some(window) = web_sys::window() {
                                                                    window.scroll_to_with_x_and_y(0.0, 0.0);
                                                                }
                                                            } else {
                                                                navigator.push(Route::Home { list: String::new() });
                                                            }
                                                        },
                                                        {render_sidebar_icon(&SidebarItem::Home, "w-7 h-7")}
                                                        span { "Home" }
                                                    }
                                                },
                                                // Profile needs pubkey
                                                SidebarItem::Profile => {
                                                    if let Some(pubkey) = &auth.pubkey {
                                                        rsx! {
                                                            div {
                                                                key: "{item:?}-mobile",
                                                                onclick: move |_| sidebar_open.set(false),
                                                                NavLink {
                                                                    to: Route::Profile { pubkey: pubkey.clone() },
                                                                    icon: render_sidebar_icon(&SidebarItem::Profile, "w-7 h-7"),
                                                                    label: "Profile"
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                },
                                                // Notifications has badge
                                                SidebarItem::Notifications => rsx! {
                                                    div {
                                                        key: "{item:?}-mobile",
                                                        onclick: move |_| sidebar_open.set(false),
                                                        NavLink {
                                                            to: Route::Notifications {},
                                                            icon: render_sidebar_icon(&SidebarItem::Notifications, "w-7 h-7"),
                                                            label: "Notifications",
                                                            badge: Some(*notif_count.read())
                                                        }
                                                    }
                                                },
                                                // All other items use standard NavLink
                                                _ => {
                                                    if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                        rsx! {
                                                            div {
                                                                key: "{item:?}-mobile",
                                                                onclick: move |_| sidebar_open.set(false),
                                                                NavLink {
                                                                    to: route,
                                                                    icon: render_sidebar_icon(&item, "w-7 h-7"),
                                                                    label: item.label()
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                }
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

                                        // Popup menu (mobile) - Dynamic based on user preferences
                                        if *more_menu_open.read() {
                                            div {
                                                class: "absolute left-0 top-full mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[240px] overflow-hidden z-50",
                                                div {
                                                    class: "flex flex-col",

                                                    // Render More menu items dynamically (mobile)
                                                    for item in crate::stores::sidebar_store::get_more_menu_items(auth.is_authenticated) {
                                                        {
                                                            if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                                rsx! {
                                                                    Link {
                                                                        key: "{item:?}-mobile-more",
                                                                        to: route,
                                                                        onclick: move |_| {
                                                                            more_menu_open.set(false);
                                                                            sidebar_open.set(false);
                                                                        },
                                                                        class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition",
                                                                        {render_sidebar_icon(&item, "w-5 h-5")}
                                                                        span { "{item.label()}" }
                                                                    }
                                                                }
                                                            } else {
                                                                rsx! {}
                                                            }
                                                        }
                                                    }

                                                    // Divider before Edit Sidebar
                                                    div { class: "border-t border-border my-1" }

                                                    // Edit Sidebar button (mobile)
                                                    button {
                                                        class: "flex items-center gap-3 px-4 py-3 hover:bg-accent transition w-full text-left",
                                                        onclick: move |_| {
                                                            more_menu_open.set(false);
                                                            sidebar_open.set(false);
                                                            sidebar_customizer_open.set(true);
                                                        },
                                                        crate::components::icons::SettingsIcon { class: "w-5 h-5" }
                                                        span { "Edit Sidebar" }
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
                    class: if is_dms_page || is_videos_page || is_wallet_page || is_music_page || is_podcast_page || is_radio_page || is_nips_page || is_badges_page || is_code_page || is_p2p_page || is_community_page || is_events_page || is_recipes_page || is_pin_boards_page || is_wiki_page || is_publications_page || is_shop_page || is_creation_page {
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

                // Right Sidebar (Trending & Search) - Hidden on DMs, Videos, Wallet, Music, Podcast, Radio, Code, P2P, Communities, Events, Wiki, Publications, Shop, and Settings pages
                if !is_dms_page && !is_videos_page && !is_wallet_page && !is_music_page && !is_podcast_page && !is_radio_page && !is_nips_page && !is_badges_page && !is_code_page && !is_p2p_page && !is_community_page && !is_events_page && !is_recipes_page && !is_pin_boards_page && !is_wiki_page && !is_publications_page && !is_shop_page && !is_settings_page && !is_creation_page {
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

            // PWA update notification banner
            crate::components::PwaUpdateBanner {}

            // Sidebar customization modal
            if *sidebar_customizer_open.read() {
                crate::components::SidebarCustomizerModal {
                    on_close: move |_| sidebar_customizer_open.set(false)
                }
            }
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
        (Route::Home { .. }, Route::Home { .. }) => true,
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
        (Route::MusicHome {}, Route::MusicRssAlbum { .. }) |
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

/// Helper function to render sidebar icons for dynamic sidebar
fn render_sidebar_icon(item: &crate::stores::sidebar_store::SidebarItem, class: &str) -> Element {
    use crate::stores::sidebar_store::SidebarItem;
    match item {
        SidebarItem::Home => rsx! { crate::components::icons::HomeIcon { class: class.to_string() } },
        SidebarItem::Explore => rsx! { crate::components::icons::CompassIcon { class: class.to_string() } },
        SidebarItem::Articles => rsx! { crate::components::icons::BookOpenIcon { class: class.to_string() } },
        SidebarItem::Music => rsx! {
            svg {
                class: "{class}",
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
        SidebarItem::Photos => rsx! { crate::components::icons::CameraIcon { class: class.to_string() } },
        SidebarItem::Videos => rsx! { crate::components::icons::VideoIcon { class: class.to_string() } },
        SidebarItem::Live => rsx! {
            svg {
                class: "{class}",
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
        SidebarItem::Notifications => rsx! { crate::components::icons::BellIcon { class: class.to_string() } },
        SidebarItem::Messages => rsx! { crate::components::icons::MailIcon { class: class.to_string() } },
        SidebarItem::Bookmarks => rsx! { crate::components::icons::BookmarkIcon { class: class.to_string() } },
        SidebarItem::Profile => rsx! { crate::components::icons::UserIcon { class: class.to_string() } },
        SidebarItem::Settings => rsx! { crate::components::icons::SettingsIcon { class: class.to_string() } },
        SidebarItem::VoiceMessages => rsx! {
            svg {
                class: "{class}",
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
        },
        SidebarItem::Polls => rsx! {
            svg {
                class: "{class}",
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
        },
        SidebarItem::WebBookmarks => rsx! {
            svg {
                class: "{class}",
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
        },
        SidebarItem::Podcasts => rsx! {
            svg {
                class: "{class}",
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
        },
        SidebarItem::Radio => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4.9 19.1C1 15.2 1 8.8 4.9 4.9" }
                path { d: "M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5" }
                circle { cx: "12", cy: "12", r: "2" }
                path { d: "M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5" }
                path { d: "M19.1 4.9C23 8.8 23 15.1 19.1 19" }
            }
        },
        SidebarItem::Wallet => rsx! {
            svg {
                class: "{class}",
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
        },
        SidebarItem::P2PTrading => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M16 3l4 4-4 4" }
                path { d: "M20 7H4" }
                path { d: "M8 21l-4-4 4-4" }
                path { d: "M4 17h16" }
            }
        },
        SidebarItem::Communities => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                circle { cx: "9", cy: "7", r: "4" }
                path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
            }
        },
        SidebarItem::Events => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" }
                circle { cx: "12", cy: "10", r: "3" }
            }
        },
        SidebarItem::Calendar => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "4", width: "18", height: "18", rx: "2", ry: "2" }
                line { x1: "16", y1: "2", x2: "16", y2: "6" }
                line { x1: "8", y1: "2", x2: "8", y2: "6" }
                line { x1: "3", y1: "10", x2: "21", y2: "10" }
            }
        },
        SidebarItem::Recipes => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M6 13.87A4 4 0 0 1 7.41 6a5.11 5.11 0 0 1 1.05-1.54 5 5 0 0 1 7.08 0A5.11 5.11 0 0 1 16.59 6 4 4 0 0 1 18 13.87V21H6Z" }
                line { x1: "6", y1: "17", x2: "18", y2: "17" }
            }
        },
        SidebarItem::PinBoards => rsx! {
            crate::components::icons::PinIcon { class: class.to_string(), filled: false }
        },
        SidebarItem::Trending => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "23 6 13.5 15.5 8.5 10.5 1 18" }
                polyline { points: "17 6 23 6 23 12" }
            }
        },
        SidebarItem::Nips => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                polyline { points: "14 2 14 8 20 8" }
                line { x1: "16", y1: "13", x2: "8", y2: "13" }
                line { x1: "16", y1: "17", x2: "8", y2: "17" }
                polyline { points: "10 9 9 9 8 9" }
            }
        },
        SidebarItem::Badges => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "12", cy: "8", r: "6" }
                path { d: "M15.477 12.89 17 22l-5-3-5 3 1.523-9.11" }
            }
        },
        SidebarItem::Citations => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                // Quote/citation icon
                path { d: "M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V21" }
                path { d: "M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3" }
            }
        },
        SidebarItem::Code => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "16 18 22 12 16 6" }
                polyline { points: "8 6 2 12 8 18" }
            }
        },
        SidebarItem::Lists => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                line { x1: "8", y1: "6", x2: "21", y2: "6" }
                line { x1: "8", y1: "12", x2: "21", y2: "12" }
                line { x1: "8", y1: "18", x2: "21", y2: "18" }
                line { x1: "3", y1: "6", x2: "3.01", y2: "6" }
                line { x1: "3", y1: "12", x2: "3.01", y2: "12" }
                line { x1: "3", y1: "18", x2: "3.01", y2: "18" }
            }
        },
        SidebarItem::Dvm => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "4", y: "4", width: "16", height: "16", rx: "2" }
                rect { x: "9", y: "9", width: "6", height: "6" }
                line { x1: "9", y1: "1", x2: "9", y2: "4" }
                line { x1: "15", y1: "1", x2: "15", y2: "4" }
                line { x1: "9", y1: "20", x2: "9", y2: "23" }
                line { x1: "15", y1: "20", x2: "15", y2: "23" }
                line { x1: "20", y1: "9", x2: "23", y2: "9" }
                line { x1: "20", y1: "14", x2: "23", y2: "14" }
                line { x1: "1", y1: "9", x2: "4", y2: "9" }
                line { x1: "1", y1: "14", x2: "4", y2: "14" }
            }
        },
        SidebarItem::Wiki => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" }
                path { d: "M8 7h6" }
                path { d: "M8 11h8" }
            }
        },
        SidebarItem::Publications => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
            }
        },
        SidebarItem::Shop => rsx! {
            crate::components::icons::ShoppingBagIcon { class: class.to_string() }
        },
    }
}
