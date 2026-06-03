use dioxus::prelude::*;
mod nav_link;
mod sidebar_icons;
use nav_link::NavLink;
use sidebar_icons::render_sidebar_icon;
pub mod about;
pub mod about_donate;
pub mod address_viewer;
pub mod ai_chat;
pub mod articles;
pub mod badges;
pub mod bible;
pub mod blossom;
pub mod quran;
pub mod bookmarks;
#[cfg(feature = "cashu")]
pub mod cashu_wallet;
pub mod chats;
pub mod citations;
pub mod code;
pub mod community;
pub mod cookies;
pub mod csae;
pub mod dms;
pub mod dvm;
pub mod events;
pub mod explore;
pub mod groups;
pub mod hashtag;
pub mod highlights;
pub mod home;
mod list_detail;
mod lists;
pub mod music;
pub mod nests;
pub mod games;
pub mod nips;
pub mod note;
pub mod note_new;
pub mod notifications;
pub mod p2p;
pub mod packs;
pub mod photo_new;
pub mod photos;
pub mod pin;
pub mod podcast;
pub mod polls;
pub mod publish_queue;
pub mod privacy;
pub mod profile;
pub mod radio;
pub mod recipes;
pub mod relay_detail;
pub mod relay_explorer;
pub mod search;
pub mod settings;
pub mod shop;
pub mod terms;
pub mod topics;
pub mod trending;
pub mod video;
pub mod video_new_landscape;
pub mod video_new_portrait;
pub mod voice;
pub mod weather;
pub mod webbookmarks;
pub mod wiki;
pub mod zapgoals;
pub mod blobbi;
pub mod places;
use about::About;
use about_donate::AboutDonate;
use address_viewer::AddressViewer;
use ai_chat::AIChat;
use articles::{
    ArticleDetail, ArticleNew, Articles, PublicationDetail, PublicationNew, PublicationSearch,
    PublicationsHome,
};
use badges::{BadgeDetail, BadgeNew, BadgesHome};
use bible::{BibleChapter, BibleHome, BibleSearch};
use quran::{QuranHome, QuranSearch, QuranSurah};
use blossom::BlossomPage;
use bookmarks::Bookmarks;
    #[cfg(feature = "cashu")]
    use cashu_wallet::CashuWallet;
use chats::{ChatDetail, ChatNew, Chats};
use citations::{CitationDetail, CitationsHome};
use code::{
    CodeBounties, CodeDiscussionDetail, CodeDiscussionNew, CodeExplore, CodeGlobalIssues,
    CodeGlobalPulls, CodeHome, CodeImport, CodeIssueDetail, CodeIssueNew, CodeNew,
    CodeNotifications, CodePages, CodePullDetail, CodePullNew, CodeRepo, CodeRepoArchitecture,
    CodeRepoBlame, CodeRepoBlob, CodeRepoCommit, CodeRepoCommits, CodeRepoCompare,
    CodeRepoDiscussions, CodeRepoEditFile, CodeRepoInsights, CodeRepoIssues, CodeRepoNewFile,
    CodeRepoPages, CodeRepoProjects, CodeRepoPulls, CodeRepoReleases, CodeRepoSettings,
    CodeRepoTree, CodeRepoUpload, CodeRepositories, CodeSearch, CodeSettings, CodeSnippetDetail,
    CodeSnippetNew, CodeSnippets, CodeStars, CodeUserProfile,
};
use community::{Communities, CommunityNew, CommunityPage};
use games::{ChessGameDetail, ChessGameNew, ChessHome, ChessPgnViewer, GamesHub};
use groups::{GroupDetail, Groups};
use cookies::Cookies;
use csae::Csae;
use dms::DMs;
use dvm::DVM;
use events::{Calendar, CalendarEventDetail, CalendarEventNew, Events};
use explore::Explore;
use hashtag::Hashtag;
use highlights::Highlights;
use home::Home;
use list_detail::ListDetail;
use lists::Lists;
use music::{
    MusicAlbum, MusicArtist, MusicHome, MusicLeaderboard, MusicPlaylistDetail, MusicPlaylistNew,
    MusicRadio, MusicRssAlbum, MusicRssArtist, MusicSearch, MusicTrackDetail, MusicTrackNew,
};
use nests::{NestCreate, NestDetail, NestServers, NestsHome};
use nips::{Nip19Handler, NipDetail, NipNew, NipsHome};
use note::Note;
use note_new::NoteNew;
use notifications::Notifications;
use p2p::{P2PHome, P2POrderDetail};
use packs::{PackDetail, PackNew, PacksHome};
use photo_new::PhotoNew;
use photos::{PhotoDetail, Photos};
use pin::{PinBoardDetail, PinBoardEdit, PinBoardNew, PinBoardsHome, PinNew, UserPins};
use podcast::{
    PodcastHome, PodcastNostrDetail, PodcastNostrEpisodeDetail, PodcastRssEpisodeDetail,
    PodcastRssFeedDetail, PodcastTrending,
};
use polls::{PollNew, PollView, Polls};
use publish_queue::PublishQueue;
use privacy::Privacy;
use profile::Profile;
use radio::{RadioHome, RadioStation, RadioStationNew};
use recipes::{
    RecipeChef, RecipeDetail, RecipeFork, RecipeNew, RecipesAll, RecipesByTag, RecipesHome,
};
use relay_detail::RelayDetail;
use relay_explorer::RelayExplorer;
use search::Search;
use settings::{Settings, SettingsAi, SettingsBlocklist, SettingsMuted, SettingsRelays};
use shop::{
    ShopCart, ShopCheckout, ShopCollection, ShopCollectionNew, ShopHome, ShopMerchant,
    ShopMerchantOrders, ShopOrders, ShopProductDetail, ShopProductEdit, ShopProductNew, ShopSearch,
};
use terms::Terms;
use topics::{TopicFeed, TopicNewPost, TopicPostDetail, TopicsBrowse, TopicsHome, TopicsPopular};
use trending::Trending;
use video::{
    LiveStreamDetail, LiveStreamNew, VideoDetail, Videos, VideosLive, VideosLiveTag, VideosVerts,
};
use video_new_landscape::VideoNewLandscape;
use video_new_portrait::VideoNewPortrait;
use voice::{VoiceMessageDetail, VoiceMessageNew, VoiceMessages};
use weather::{WeatherDetail, WeatherHome, WeatherSearch};
use webbookmarks::WebBookmarks;
use wiki::{WikiAuthor, WikiDetail, WikiHome, WikiNew, WikiSlug};
use zapgoals::{ZapGoalsHome, ZapGoalsNew};
use blobbi::BlobbiHome;
use places::{PlacesHome, PlacesMap};
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
    #[route("/trending?:source")]
    Trending { source: Option<String> },
    #[route("/search?:q")]
    Search { q: String },
    #[route("/articles")]
    Articles {},
    #[route("/articles/:naddr")]
    ArticleDetail { naddr: String },
    #[route("/videos")]
    Videos {},
    #[route("/videos/verts")]
    VideosVerts {},
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
    #[route("/music/rss/artist?:artist")]
    MusicRssArtist { artist: String },
    #[route("/music/track/:track_id")]
    MusicTrackDetail { track_id: String },
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
    #[route("/radio")]
    RadioHome {},
    #[route("/radio/new")]
    RadioStationNew {},
    #[route("/radio/:naddr")]
    RadioStation { naddr: String },
    #[route("/nests")]
    NestsHome {},
    #[route("/nests/new?:naddr")]
    NestCreate { naddr: Option<String> },
    #[route("/nests/servers")]
    NestServers {},
    #[route("/nests/:naddr")]
    NestDetail { naddr: String },
    #[route("/nips")]
    NipsHome {},
    #[route("/nips/new")]
    NipNew {},
    #[route("/nips/:nip_id")]
    NipDetail { nip_id: String },
    #[route("/badges")]
    BadgesHome {},
    #[route("/badges/new")]
    BadgeNew {},
    #[route("/badges/:naddr")]
    BadgeDetail { naddr: String },
    #[route("/packs")]
    PacksHome {},
    #[route("/packs/new")]
    PackNew {},
    #[route("/packs/:naddr")]
    PackDetail { naddr: String },
    #[route("/citations")]
    CitationsHome {},
    #[route("/citations/:naddr")]
    CitationDetail { naddr: String },
    #[route("/code")]
    CodeHome {},
    #[route("/code/new")]
    CodeNew {},
    #[route("/code/stars")]
    CodeStars {},
    #[route("/code/bounties")]
    CodeBounties {},
    #[route("/code/settings")]
    CodeSettings {},
    #[route("/code/issues")]
    CodeGlobalIssues {},
    #[route("/code/pulls")]
    CodeGlobalPulls {},
    #[route("/code/notifications")]
    CodeNotifications {},
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
    #[route("/code/repo/:naddr/commit/:sha")]
    CodeRepoCommit { naddr: String, sha: String },
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
    #[route("/code/repo/:naddr/insights")]
    CodeRepoInsights { naddr: String },
    #[route("/code/repo/:naddr/projects")]
    CodeRepoProjects { naddr: String },
    #[route("/code/repo/:naddr/blame/:git_ref/:..path")]
    CodeRepoBlame { naddr: String, git_ref: String, path: Vec<String> },
    #[route("/code/repo/:naddr/compare")]
    CodeRepoCompare { naddr: String },
    #[route("/code/repo/:naddr/upload")]
    CodeRepoUpload { naddr: String },
    #[route("/code/repo/:naddr/new-file")]
    CodeRepoNewFile { naddr: String },
    #[route("/code/repo/:naddr/edit/:git_ref/:..path")]
    CodeRepoEditFile { naddr: String, git_ref: String, path: Vec<String> },
    #[route("/code/repo/:naddr/architecture")]
    CodeRepoArchitecture { naddr: String },
    #[route("/code/repo/:naddr/releases")]
    CodeRepoReleases { naddr: String },
    #[route("/code/repo/:naddr/discussions")]
    CodeRepoDiscussions { naddr: String },
    #[route("/code/repo/:naddr/discussions/new")]
    CodeDiscussionNew { naddr: String },
    #[route("/code/repo/:naddr/tree/:git_ref/:..path")]
    CodeRepoTree { naddr: String, git_ref: String, path: Vec<String> },
    #[route("/code/repo/:naddr/blob/:git_ref/:..path")]
    CodeRepoBlob { naddr: String, git_ref: String, path: Vec<String> },
    #[route("/code/issue/:note_id")]
    CodeIssueDetail { note_id: String },
    #[route("/code/pull/:note_id")]
    CodePullDetail { note_id: String },
    #[route("/code/discussion/:note_id")]
    CodeDiscussionDetail { note_id: String },
    #[route("/code/profile/:pubkey")]
    CodeUserProfile { pubkey: String },
    #[route("/code/pages")]
    CodePages {},
    #[route("/code/repo/:naddr/pages")]
    CodeRepoPages { naddr: String },
    #[route("/p2p")]
    P2PHome {},
    #[route("/p2p/order/:naddr")]
    P2POrderDetail { naddr: String },
    #[route("/chats")]
    Chats {},
    #[route("/chats/new")]
    ChatNew {},
    #[route("/chats/:channel_id")]
    ChatDetail { channel_id: String },
    #[route("/communities")]
    Communities {},
    #[route("/communities/new")]
    CommunityNew {},
    #[route("/community/:naddr")]
    CommunityPage { naddr: String },
    #[route("/groups")]
    Groups {},
    #[route("/group/:encoded_relay/:group_id")]
    GroupDetail { encoded_relay: String, group_id: String },
    #[route("/topics")]
    TopicsHome {},
    #[route("/topics/popular")]
    TopicsPopular {},
    #[route("/topics/browse")]
    TopicsBrowse {},
    #[route("/topics/new")]
    TopicNewPost {},
    #[route("/topics/t/:topic")]
    TopicFeed { topic: String },
    #[route("/topics/t/:topic/post/:post_id")]
    TopicPostDetail { topic: String, post_id: String },
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
    #[route("/pinboards")]
    PinBoardsHome {},
    #[route("/pinboards/new")]
    PinBoardNew {},
    #[route("/pinboards/pin/new")]
    PinNew {},
    #[route("/pinboards/pins")]
    UserPins {},
    #[route("/pinboards/:naddr")]
    PinBoardDetail { naddr: String },
    #[route("/pinboards/:naddr/edit")]
    PinBoardEdit { naddr: String },
    #[route("/wiki")]
    WikiHome {},
    #[route("/wiki/new")]
    WikiNew {},
    #[route("/wiki/:npub/:identifier")]
    WikiDetail { npub: String, identifier: String },
    #[route("/wiki/:slug")]
    WikiSlug { slug: String },
    #[route("/wiki/author/:pubkey")]
    WikiAuthor { pubkey: String },
    #[route("/publications")]
    PublicationsHome {},
    #[route("/publications/new")]
    PublicationNew {},
    #[route("/publications/search?:query")]
    PublicationSearch { query: String },
    #[route("/publications/:naddr")]
    PublicationDetail { naddr: String },
    #[route("/events")]
    Events {},
    #[route("/calendar/:naddr?:from")]
    CalendarEventDetail { naddr: String, from: Option<String> },
    #[route("/calendar")]
    Calendar {},
    #[route("/calendar/new?:edit_naddr")]
    CalendarEventNew { edit_naddr: Option<String> },
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
    #[route("/pending")]
    PublishQueue {},
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
    #[cfg(feature = "cashu")]
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
    #[route("/blossom")]
    BlossomPage {},
    #[route("/bible")]
    BibleHome {},
    #[route("/bible/:translation/:book/:chapter")]
    BibleChapter { translation: String, book: String, chapter: u32 },
    #[route("/bible/search")]
    BibleSearch {},
    #[route("/quran")]
    QuranHome {},
    #[route("/quran/search")]
    QuranSearch {},
    #[route("/quran/:surah")]
    QuranSurah { surah: u32 },
    #[route("/highlights")]
    Highlights {},
    #[route("/ai-chat")]
    AIChat {},
    #[route("/blobbi")]
    BlobbiHome {},
    #[route("/settings")]
    Settings {},
    #[route("/settings/ai")]
    SettingsAi {},
    #[route("/settings/blocklist")]
    SettingsBlocklist {},
    #[route("/settings/muted")]
    SettingsMuted {},
    #[route("/settings/relays")]
    SettingsRelays {},
    #[route("/relays/explore")]
    RelayExplorer {},
    #[route("/relays/:relay_id")]
    RelayDetail { relay_id: String },
    #[route("/terms")]
    Terms {},
    #[route("/privacy")]
    Privacy {},
    #[route("/cookies")]
    Cookies {},
    #[route("/csae")]
    Csae {},
    #[route("/about")]
    About {},
    #[route("/about/donate")]
    AboutDonate {},
    #[route("/zapgoals")]
    ZapGoalsHome {},
    #[route("/zapgoals/new")]
    ZapGoalsNew {},
    #[route("/weather")]
    WeatherHome {},
    #[route("/weather/search")]
    WeatherSearch {},
    #[route("/weather/day/:date")]
    WeatherDetail { date: String },
    #[route("/places")]
    PlacesHome {},
    #[route("/places/map")]
    PlacesMap {},
    #[route("/games")]
    GamesHub {},
    #[route("/games/chess")]
    ChessHome {},
    #[route("/games/chess/new")]
    ChessGameNew {},
    #[route("/games/chess/:game_id")]
    ChessGameDetail { game_id: String },
    #[route("/games/chess/pgn/:note_id")]
    ChessPgnViewer { note_id: String },
    #[route("/:address")]
    AddressViewer { address: String },
}

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
fn note_back_target(current_route: &Route) -> Option<Route> {
    let Route::Note {
        note_id,
        from_voice,
    } = current_route
    else {
        return None;
    };

    let note_context = crate::stores::back_navigation::ACTIVE_NOTE_BACK_CONTEXT.read();
    let note_matches = note_context.note_id.as_deref().is_some_and(|ctx_id| {
        crate::stores::nostr_client::parse_event_id(ctx_id).map(|p| p.event_id)
            == crate::stores::nostr_client::parse_event_id(note_id).map(|p| p.event_id)
    });
    if note_matches {
        if let Some(parent_note_id) = note_context.parent_note_ids.last() {
            return Some(Route::AddressViewer {
                address: crate::utils::nip19_urls::note_route_id(parent_note_id, None),
            });
        }

        if note_context.is_voice_note {
            return Some(Route::VoiceMessages {});
        }
    }

    if from_voice.as_deref() == Some("true") {
        Some(Route::VoiceMessages {})
    } else {
        Some(Route::Home {
            list: String::new(),
        })
    }
}

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
fn fallback_route_for(current_route: &Route) -> Option<Route> {
    match current_route {
        Route::Home { .. }
        | Route::Explore {}
        | Route::Trending { .. }
        | Route::Articles {}
        | Route::Videos {}
        | Route::VideosLive {}
        | Route::MusicHome {}
        | Route::MusicRadio {}
        | Route::MusicLeaderboard {}
        | Route::PodcastHome {}
        | Route::RadioHome {}
        | Route::NestsHome {}
        | Route::NipsHome {}
        | Route::BadgesHome {}
        | Route::PacksHome {}
        | Route::CitationsHome {}
        | Route::CodeHome {}
        | Route::P2PHome {}
        | Route::Chats {}
        | Route::Communities {}
        | Route::Groups {}
        | Route::TopicsHome {}
        | Route::RecipesHome {}
        | Route::PinBoardsHome {}
        | Route::WikiHome {}
        | Route::PublicationsHome {}
        | Route::Events {}
        | Route::Calendar {}
        | Route::ShopHome {}
        | Route::Notifications {}
        | Route::PublishQueue {}
        | Route::Bookmarks {}
        | Route::DMs {}
        | Route::Photos {}
        | Route::VoiceMessages {}
        | Route::Polls {}
        | Route::Lists {}
        | Route::DVM {}
        | Route::BlossomPage {}
        | Route::BibleHome {}
        | Route::QuranHome {}
        | Route::Highlights {}
        | Route::AIChat {}
        | Route::BlobbiHome {}
        | Route::Settings {}
        | Route::WebBookmarks {}
        | Route::WeatherHome {}
        | Route::GamesHub {} => None,
        #[cfg(feature = "cashu")]
        Route::CashuWallet {} => None,
        Route::Search { .. }
        | Route::Hashtag { .. }
        | Route::Profile { .. }
        | Route::Terms {}
        | Route::Privacy {}
        | Route::Cookies {}
        | Route::Csae {}
        | Route::About {}
        | Route::Nip19Handler { .. } => Some(Route::Home {
            list: String::new(),
        }),
        Route::AboutDonate {} => Some(Route::About {}),
        Route::ZapGoalsHome {} => Some(Route::About {}),
        Route::ZapGoalsNew {} => Some(Route::ZapGoalsHome {}),
        Route::WeatherDetail { .. } => Some(Route::WeatherHome {}),
        Route::WeatherSearch {} => Some(Route::WeatherHome {}),
        Route::ChessGameDetail { .. }
        | Route::ChessGameNew {}
        | Route::ChessPgnViewer { .. } => Some(Route::ChessHome {}),
        Route::ChessHome {} => Some(Route::GamesHub {}),
        Route::ArticleDetail { .. } | Route::ArticleNew {} => Some(Route::Articles {}),
        Route::VideosVerts {}
        | Route::VideoDetail { .. }
        | Route::VideoNewLandscape {}
        | Route::VideoNewPortrait {} => Some(Route::Videos {}),
        Route::VideosLiveTag { .. } | Route::LiveStreamDetail { .. } | Route::LiveStreamNew {} => {
            Some(Route::VideosLive {})
        }
        Route::MusicSearch { .. }
        | Route::MusicArtist { .. }
        | Route::MusicAlbum { .. }
        | Route::MusicTrackNew {}
        | Route::MusicTrackDetail { .. }
        | Route::MusicPlaylistNew {}
        | Route::MusicPlaylistDetail { .. }
        | Route::MusicRssAlbum { .. }
        | Route::MusicRssArtist { .. } => Some(Route::MusicHome {}),
        Route::PodcastTrending {}
        | Route::PodcastNostrDetail { .. }
        | Route::PodcastRssFeedDetail { .. }
        | Route::PodcastNostrEpisodeDetail { .. }
        | Route::PodcastRssEpisodeDetail { .. } => Some(Route::PodcastHome {}),
        Route::RadioStationNew {} | Route::RadioStation { .. } => Some(Route::RadioHome {}),
        Route::NestCreate { .. } | Route::NestDetail { .. } | Route::NestServers {} => Some(Route::NestsHome {}),
        Route::NipNew {} | Route::NipDetail { .. } => Some(Route::NipsHome {}),
        Route::BadgeNew {} | Route::BadgeDetail { .. } => Some(Route::BadgesHome {}),
        Route::PackNew {} | Route::PackDetail { .. } => Some(Route::PacksHome {}),
        Route::CitationDetail { .. } => Some(Route::CitationsHome {}),
        Route::CodeNew {}
        | Route::CodeStars {}
        | Route::CodeBounties {}
        | Route::CodeSettings {}
        | Route::CodeGlobalIssues {}
        | Route::CodeGlobalPulls {}
        | Route::CodeNotifications {}
        | Route::CodeExplore {}
        | Route::CodeRepositories {}
        | Route::CodeSnippets {}
        | Route::CodeSnippetDetail { .. }
        | Route::CodeSnippetNew {}
        | Route::CodeImport {}
        | Route::CodeSearch { .. }
        | Route::CodeRepo { .. }
        | Route::CodeRepoCommits { .. }
        | Route::CodeRepoCommit { .. }
        | Route::CodeRepoIssues { .. }
        | Route::CodeIssueNew { .. }
        | Route::CodeRepoPulls { .. }
        | Route::CodePullNew { .. }
        | Route::CodeRepoSettings { .. }
        | Route::CodeRepoInsights { .. }
        | Route::CodeRepoProjects { .. }
        | Route::CodeRepoBlame { .. }
        | Route::CodeRepoCompare { .. }
        | Route::CodeRepoUpload { .. }
        | Route::CodeRepoNewFile { .. }
        | Route::CodeRepoEditFile { .. }
        | Route::CodeRepoArchitecture { .. }
        | Route::CodeRepoReleases { .. }
        | Route::CodeRepoDiscussions { .. }
        | Route::CodeDiscussionNew { .. }
        | Route::CodeRepoTree { .. }
        | Route::CodeRepoBlob { .. }
        | Route::CodeIssueDetail { .. }
        | Route::CodePullDetail { .. }
        | Route::CodeDiscussionDetail { .. }
        | Route::CodeUserProfile { .. }
        | Route::CodePages {}
        | Route::CodeRepoPages { .. } => Some(Route::CodeHome {}),
        Route::P2POrderDetail { .. } => Some(Route::P2PHome {}),
        Route::ChatNew {} | Route::ChatDetail { .. } => Some(Route::Chats {}),
        Route::CommunityNew {} | Route::CommunityPage { .. } => Some(Route::Communities {}),
        Route::GroupDetail { .. } => Some(Route::Groups {}),
        Route::TopicsPopular {}
        | Route::TopicsBrowse {}
        | Route::TopicNewPost {}
        | Route::TopicFeed { .. }
        | Route::TopicPostDetail { .. } => Some(Route::TopicsHome {}),
        Route::RecipesAll {}
        | Route::RecipeNew {}
        | Route::RecipeFork { .. }
        | Route::RecipesByTag { .. }
        | Route::RecipeChef { .. }
        | Route::RecipeDetail { .. } => Some(Route::RecipesHome {}),
        Route::PinBoardNew {}
        | Route::PinNew {}
        | Route::UserPins {}
        | Route::PinBoardDetail { .. }
        | Route::PinBoardEdit { .. } => Some(Route::PinBoardsHome {}),
        Route::WikiNew {} | Route::WikiDetail { .. } | Route::WikiAuthor { .. } | Route::WikiSlug { .. } => {
            Some(Route::WikiHome {})
        }
        Route::PublicationNew {}
        | Route::PublicationSearch { .. }
        | Route::PublicationDetail { .. } => Some(Route::PublicationsHome {}),
        Route::CalendarEventDetail { .. } | Route::CalendarEventNew { .. } => Some(Route::Calendar {}),
        Route::ShopProductDetail { .. }
        | Route::ShopProductNew {}
        | Route::ShopProductEdit { .. }
        | Route::ShopCart {}
        | Route::ShopCheckout {}
        | Route::ShopOrders {}
        | Route::ShopMerchant {}
        | Route::ShopMerchantOrders {}
        | Route::ShopCollection { .. }
        | Route::ShopCollectionNew {}
        | Route::ShopSearch { .. } => Some(Route::ShopHome {}),
        Route::PhotoDetail { .. } | Route::PhotoNew {} => Some(Route::Photos {}),
        Route::VoiceMessageNew {} | Route::VoiceMessageDetail { .. } => {
            Some(Route::VoiceMessages {})
        }
        Route::PollNew {} | Route::PollView { .. } => Some(Route::Polls {}),
        Route::NoteNew { .. } => Some(Route::Home {
            list: String::new(),
        }),
        Route::Note { .. } => note_back_target(current_route),
        Route::ListDetail { .. } => Some(Route::Lists {}),
        Route::BibleChapter { .. } | Route::BibleSearch {} => Some(Route::BibleHome {}),
        Route::QuranSurah { .. } | Route::QuranSearch {} => Some(Route::QuranHome {}),
        Route::SettingsAi {}
        | Route::SettingsBlocklist {}
        | Route::SettingsMuted {}
        | Route::SettingsRelays {} => Some(Route::Settings {}),
        Route::RelayExplorer {} => Some(Route::SettingsRelays {}),
        Route::RelayDetail { .. } => Some(Route::SettingsRelays {}),
        Route::AddressViewer { .. } => Some(Route::Home {
            list: String::new(),
        }),
        Route::PlacesHome {} => Some(Route::Explore {}),
        Route::PlacesMap {} => Some(Route::PlacesHome {}),
    }
}

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
fn handle_android_back(navigator: dioxus::router::Navigator, current_route: &Route) {
    if crate::stores::back_navigation::close_topmost_mobile_overlay() {
        return;
    }

    if navigator.can_go_back() {
        navigator.go_back();
        return;
    }

    if let Some(target) = fallback_route_for(current_route) {
        let _ = navigator.replace(target);
    } else {
        #[cfg(feature = "mobile_platform")]
        {
            if let Err(error) = crate::platform::mobile::finish_app() {
                log::error!("Failed to finish Android activity: {}", error);
            }
        }
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_MainActivity_handleAndroidBackPressed(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) {
    crate::stores::back_navigation::request_android_back_from_platform();
}

#[component]
fn Layout() -> Element {
    use crate::stores::{
        auth_store, back_navigation, music_player::{MusicPlayerStateStoreExt, MUSIC_PLAYER, PlayerViewMode}, notifications as notif_store,
    };
    let auth = auth_store::AUTH_STATE.read();
    let notif_count = use_memo(notif_store::get_unread_count);
    let mut sidebar_open = back_navigation::MOBILE_SIDEBAR_OPEN.signal();
    let mut sidebar_page = back_navigation::MOBILE_SIDEBAR_PAGE.signal();
    let sidebar_items_guard = crate::stores::sidebar_store::SIDEBAR_ITEMS.read();
    let sidebar_slot_count = *crate::stores::sidebar_store::SIDEBAR_SLOT_COUNT.read();
    let _sidebar_state = crate::stores::sidebar_store::SIDEBAR_STATE.read();
    let sidebar_visible = crate::stores::sidebar_store::compute_visible_items(
        &sidebar_items_guard,
        auth.is_authenticated,
    );
    let sidebar_total_pages = crate::stores::sidebar_store::compute_total_pages(
        sidebar_visible.len(),
        sidebar_slot_count,
    );
    use_effect(move || {
        let max_page = sidebar_total_pages.saturating_sub(1);
        if *sidebar_page.read() > max_page {
            *sidebar_page.write() = max_page;
        }
    });
    let mut radial_menu_open = back_navigation::RADIAL_MENU_OPEN.signal();
    let mut sidebar_customizer_open = back_navigation::SIDEBAR_CUSTOMIZER_OPEN.signal();
    let mut mobile_search_open = back_navigation::MOBILE_SEARCH_OPEN.signal();
    #[allow(unused_mut, unused_variables)]
    let mut android_back_nonce = use_signal(|| 0u64);
    #[allow(unused_mut, unused_variables)]
    let mut last_handled_android_back_nonce = use_signal(|| 0u64);
    let current_route = use_route::<Route>();
    let navigator = navigator();
    let mut previous_route_key: Signal<Option<String>> = use_signal(|| None);
    {
        let route = current_route.clone();
        use_effect(move || {
            let route = route.clone();
            let prev_key = previous_route_key.read().clone();
            spawn(async move {
                if let Some(pk) = prev_key {
                    let y = crate::stores::ui::scroll_restore::get_tracked_scroll_y().await;
                    if y > 0.0 {
                        crate::stores::ui::scroll_restore::save_scroll(&pk, y);
                    }
                }
                if crate::stores::ui::scroll_restore::was_popstate_nav().await {
                    let route_key = format!("{:?}", route);
                    if let Some(y) = crate::stores::ui::scroll_restore::get_scroll(&route_key) {
                        crate::stores::ui::scroll_restore::set_scroll_y(y).await;
                    }
                } else {
                    crate::stores::ui::scroll_restore::set_scroll_y(0.0).await;
                }
                crate::platform::timer::sleep_ms(100).await;
                crate::stores::ui::scroll_restore::clear_popstate_flag().await;
            });
        });
    }
    {
        let route = current_route.clone();
        use_effect(move || {
            let key = format!("{:?}", route);
            *previous_route_key.write() = Some(key);
        });
    }
    #[cfg(feature = "mobile_platform")]
    let _android_back_poller = use_future(move || async move {
        let mut last_seen = 0;
        loop {
            let latest = back_navigation::platform_android_back_request_count();
            if latest > last_seen {
                last_seen = latest;
                android_back_nonce.set(latest);
            }
            crate::platform::timer::sleep_ms(50).await;
        }
    });
    #[cfg(feature = "mobile_platform")]
    let route_for_android_back = current_route.clone();
    #[cfg(feature = "mobile_platform")]
    use_effect(use_reactive(&*android_back_nonce.read(), move |nonce| {
        if nonce == 0 {
            return;
        }

        if *last_handled_android_back_nonce.read() == nonce {
            return;
        }

        last_handled_android_back_nonce.set(nonce);
        handle_android_back(navigator, &route_for_android_back);
    }));
    let is_dms_page = matches!(current_route, Route::DMs {});
    let is_videos_page = matches!(
        current_route,
        Route::Videos {}
            | Route::VideosVerts {}
            | Route::VideoDetail { .. }
            | Route::VideosLive {}
            | Route::VideosLiveTag { .. }
            | Route::LiveStreamDetail { .. }
    );
    #[cfg(feature = "cashu")]
    let is_wallet_page = matches!(current_route, Route::CashuWallet {});
    #[cfg(not(feature = "cashu"))]
    let is_wallet_page = false;
    let is_music_page = matches!(
        current_route,
        Route::MusicHome {}
            | Route::MusicRadio {}
            | Route::MusicLeaderboard {}
            | Route::MusicSearch { .. }
            | Route::MusicArtist { .. }
            | Route::MusicAlbum { .. }
            | Route::MusicRssAlbum { .. }
            | Route::MusicRssArtist { .. }
            | Route::MusicTrackNew {}
            | Route::MusicTrackDetail { .. }
            | Route::MusicPlaylistNew {}
            | Route::MusicPlaylistDetail { .. }
    );
    let is_podcast_page = matches!(
        current_route,
        Route::PodcastHome {}
            | Route::PodcastTrending {}
            | Route::PodcastNostrDetail { .. }
            | Route::PodcastRssFeedDetail { .. }
            | Route::PodcastNostrEpisodeDetail { .. }
            | Route::PodcastRssEpisodeDetail { .. }
    );
    let is_radio_page = matches!(
        current_route,
        Route::RadioHome {} | Route::RadioStation { .. } | Route::RadioStationNew {}
    );
    let is_nips_page = matches!(
        current_route,
        Route::NipsHome {} | Route::NipDetail { .. } | Route::NipNew {}
    );
    let is_badges_page = matches!(
        current_route,
        Route::BadgesHome {} | Route::BadgeDetail { .. } | Route::BadgeNew {}
    );
    let is_packs_page = matches!(
        current_route,
        Route::PacksHome {} | Route::PackDetail { .. } | Route::PackNew {}
    );
    let is_code_page = matches!(
        current_route,
        Route::CodeHome {}
            | Route::CodeNew {}
            | Route::CodeStars {}
            | Route::CodeBounties {}
            | Route::CodeSettings {}
            | Route::CodeGlobalIssues {}
            | Route::CodeGlobalPulls {}
            | Route::CodeExplore {}
            | Route::CodeRepositories {}
            | Route::CodeSnippets {}
            | Route::CodeSnippetDetail { .. }
            | Route::CodeSnippetNew {}
            | Route::CodeImport {}
            | Route::CodeSearch { .. }
            | Route::CodeRepo { .. }
            | Route::CodeRepoCommit { .. }
            | Route::CodeRepoCommits { .. }
            | Route::CodeRepoIssues { .. }
            | Route::CodeRepoPulls { .. }
            | Route::CodeIssueNew { .. }
            | Route::CodePullNew { .. }
            | Route::CodeRepoSettings { .. }
            | Route::CodeRepoInsights { .. }
            | Route::CodeRepoProjects { .. }
            | Route::CodeRepoBlame { .. }
            | Route::CodeRepoCompare { .. }
            | Route::CodeRepoUpload { .. }
            | Route::CodeRepoNewFile { .. }
            | Route::CodeRepoEditFile { .. }
            | Route::CodeRepoArchitecture { .. }
            | Route::CodeRepoReleases { .. }
            | Route::CodeRepoDiscussions { .. }
            | Route::CodeDiscussionNew { .. }
            | Route::CodeDiscussionDetail { .. }
            | Route::CodeIssueDetail { .. }
            | Route::CodePullDetail { .. }
            | Route::CodeRepoTree { .. }
            | Route::CodeRepoBlob { .. }
            | Route::CodeUserProfile { .. }
            | Route::CodeNotifications {}
            | Route::CodePages {}
            | Route::CodeRepoPages { .. }
    );
    let is_p2p_page = matches!(
        current_route,
        Route::P2PHome {} | Route::P2POrderDetail { .. }
    );
    let is_chats_page = matches!(
        current_route,
        Route::Chats {} | Route::ChatNew {} | Route::ChatDetail { .. }
    );
    let is_community_page = matches!(
        current_route,
        Route::Communities {} | Route::CommunityPage { .. }
    );
    let is_groups_page = matches!(
        current_route,
        Route::Groups {} | Route::GroupDetail { .. }
    );
    let is_topics_page = matches!(
        current_route,
        Route::TopicsHome {}
            | Route::TopicsPopular {}
            | Route::TopicsBrowse {}
            | Route::TopicNewPost {}
            | Route::TopicFeed { .. }
            | Route::TopicPostDetail { .. }
    );
    let is_events_page = matches!(
        current_route,
        Route::Events {} | Route::CalendarEventDetail { .. } | Route::Calendar {}
    );
    let is_recipes_page = matches!(
        current_route,
        Route::RecipesHome {}
            | Route::RecipesAll {}
            | Route::RecipeDetail { .. }
            | Route::RecipeNew {}
            | Route::RecipeFork { .. }
            | Route::RecipesByTag { .. }
            | Route::RecipeChef { .. }
    );
    let is_pin_boards_page = matches!(
        current_route,
        Route::PinBoardsHome {}
            | Route::PinBoardDetail { .. }
            | Route::PinBoardNew {}
            | Route::PinBoardEdit { .. }
            | Route::PinNew {}
            | Route::UserPins {}
    );
    let is_wiki_page = matches!(
        current_route,
        Route::WikiHome {}
            | Route::WikiDetail { .. }
            | Route::WikiSlug { .. }
            | Route::WikiNew {}
            | Route::WikiAuthor { .. }
    );
    let is_publications_page = matches!(
        current_route,
        Route::PublicationsHome {}
            | Route::PublicationDetail { .. }
            | Route::PublicationNew {}
            | Route::PublicationSearch { .. }
    );
    let is_shop_page = matches!(
        current_route,
        Route::ShopHome {}
            | Route::ShopProductDetail { .. }
            | Route::ShopProductNew {}
            | Route::ShopProductEdit { .. }
            | Route::ShopCart {}
            | Route::ShopCheckout {}
            | Route::ShopOrders {}
            | Route::ShopMerchant {}
            | Route::ShopMerchantOrders {}
            | Route::ShopCollection { .. }
            | Route::ShopCollectionNew {}
            | Route::ShopSearch { .. }
    );
    let is_blossom_page = matches!(current_route, Route::BlossomPage {});
    let is_bible_page = matches!(
        current_route,
        Route::BibleHome {} | Route::BibleChapter { .. } | Route::BibleSearch {}
    );
    let is_quran_page = matches!(
        current_route,
        Route::QuranHome {} | Route::QuranSurah { .. } | Route::QuranSearch {}
    );
    let is_weather_page = matches!(
        current_route,
        Route::WeatherHome {}
            | Route::WeatherSearch {}
            | Route::WeatherDetail { .. }
    );
    let is_settings_page = matches!(
        current_route,
        Route::Settings {}
            | Route::SettingsAi {}
            | Route::SettingsBlocklist {}
            | Route::SettingsMuted {}
            | Route::SettingsRelays {}
            | Route::RelayExplorer {}
            | Route::RelayDetail { .. }
    );
    let is_creation_page = matches!(
        current_route,
        Route::NoteNew { .. }
            | Route::ArticleNew {}
            | Route::PhotoNew {}
            | Route::VideoNewLandscape {}
            | Route::VideoNewPortrait {}
            | Route::LiveStreamNew {}
    );
    let is_home_page = matches!(current_route, Route::Home { .. });
    let home_font_weight = if is_home_page { "font-bold" } else { "" };
    let is_address_wide_page = matches!(current_route, Route::AddressViewer { .. })
        && *crate::stores::ui::back_navigation::ADDRESS_WIDE_MODE.read();
    let is_wide_page = is_dms_page
        || is_videos_page
        || is_wallet_page
        || is_music_page
        || is_podcast_page
        || is_radio_page
        || is_nips_page
        || is_badges_page
        || is_packs_page
        || is_code_page
        || is_p2p_page
        || is_chats_page
        || is_community_page
        || is_groups_page
        || is_events_page
        || is_recipes_page
        || is_pin_boards_page
        || is_wiki_page
        || is_publications_page
        || is_shop_page
        || is_blossom_page
        || is_bible_page
        || is_quran_page
        || is_weather_page
        || is_creation_page
        || is_topics_page
        || is_address_wide_page
        || matches!(
            current_route,
            Route::AboutDonate {} | Route::ZapGoalsHome {} | Route::ZapGoalsNew {} | Route::BlobbiHome {}
        );
    let player_offset = {
        let store = MUSIC_PLAYER.resolve();
        let is_visible = *store.is_visible().read();
        let has_track = store.current_track().read().is_some();
        let view_mode = store.view_mode().cloned();
        if !is_visible || !has_track {
            "0px"
        } else {
            match view_mode {
                PlayerViewMode::Floating => "0px",
                _ => "6rem",
            }
        }
    };
    rsx! {
        div {
            class: "min-h-dynamic-screen bg-background transition-colors",
            style: format!(
                "--persistent-player-offset: {}; --mobile-shell-header-height: calc(var(--safe-area-top) + 57px);",
                player_offset
            ),
            onclick: move |_| {
                if *sidebar_page.read() != 0 {
                    *sidebar_page.write() = 0;
                }
            },
            div { class: "flex justify-center max-w-[1600px] mx-auto",
                aside {
                    class: "w-[275px] shrink-0 border-r border-border sticky top-0 h-screen hidden lg:block bg-background",
                    onmouseenter: move |_| {
                        crate::components::blobbi::companion::behavior_loop::set_gaze_target(120.0, 400.0);
                    },
                    onmouseleave: move |_| {
                        crate::components::blobbi::companion::behavior_loop::clear_gaze_target();
                    },
                    div { class: "h-full flex flex-col p-4 overflow-y-auto scrollbar-hide",
                        {
                            let current_page = (*sidebar_page.read()).min(sidebar_total_pages.saturating_sub(1));
                            let is_last_page = current_page >= sidebar_total_pages.saturating_sub(1);
                            let has_more = sidebar_total_pages > 1 && !is_last_page;
                            let page_items = crate::stores::sidebar_store::compute_page_items(&sidebar_visible, sidebar_slot_count, current_page);
                            rsx! {
                                if current_page == 0 {
                                    // Page 0: Logo
                                    div {
                                        class: "flex items-center gap-2 hover:opacity-80 transition mb-6 cursor-pointer",
                                        onclick: move |_| {
                                            if is_home_page {
                                                spawn(async move {
                                                    crate::stores::ui::scroll_restore::set_scroll_y(0.0).await;
                                                });
                                            } else {
                                                navigator.push(Route::Home { list: String::new() });
                                            }
                                        },
                                        div { class: "w-12 h-12 bg-blue-500 hover:bg-blue-600 rounded-full flex items-center justify-center text-white font-bold text-xl transition",
                                            "N"
                                        }
                                    }
                                } else {
                                    // Pages 1+: Back button
                                    button {
                                        class: "flex items-center gap-2 mb-4 p-2 rounded-lg hover:bg-accent transition cursor-pointer",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            let prev = sidebar_page.read().saturating_sub(1);
                                            *sidebar_page.write() = prev;
                                        },
                                        "← Back"
                                    }
                                }
                                nav { class: "flex flex-col gap-1",
                                    for item in page_items {
                                        {
                                            use crate::stores::sidebar_store::SidebarItem;
                                            match item {
                                                SidebarItem::Home => rsx! {
                                                    div {
                                                        key: "{item:?}",
                                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                                        onclick: move |_| {
                                                            *sidebar_page.write() = 0;
                                                            if is_home_page {
                                                                spawn(async move {
                                                                    crate::stores::ui::scroll_restore::set_scroll_y(0.0).await;
                                                                });
                                                            } else {
                                                                navigator.push(Route::Home { list: String::new() });
                                                            }
                                                        },
                                                        {render_sidebar_icon(&SidebarItem::Home, "w-7 h-7")}
                                                        span { "Home" }
                                                    }
                                                },
                                                SidebarItem::Profile => {
                                                    if let Some(pubkey) = &auth.pubkey {
                                                        rsx! {
                                                            div {
                                                                key: "{item:?}",
                                                                onclick: move |_| *sidebar_page.write() = 0,
                                                                NavLink {
                                                                    to: Route::AddressViewer {
                                                                        address: crate::utils::nip19_urls::profile_route_id(pubkey),
                                                                    },
                                                                    icon: render_sidebar_icon(&SidebarItem::Profile, "w-7 h-7"),
                                                                    label: "Profile",
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                }
                                                SidebarItem::Notifications => rsx! {
                                                    div {
                                                        key: "{item:?}",
                                                        onclick: move |_| *sidebar_page.write() = 0,
                                                        NavLink {
                                                            to: Route::Notifications {},
                                                            icon: render_sidebar_icon(&SidebarItem::Notifications, "w-7 h-7"),
                                                            label: "Notifications",
                                                            badge: Some(*notif_count.read()),
                                                        }
                                                    }
                                                },
                                                _ => {
                                                    if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                        rsx! {
                                                            div {
                                                                key: "{item:?}",
                                                                onclick: move |_| *sidebar_page.write() = 0,
                                                                NavLink {
                                                                    to: route,
                                                                    icon: render_sidebar_icon(&item, "w-7 h-7"),
                                                                    label: item.label(),
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
                                    if has_more {
                                        button {
                                            class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                let next = *sidebar_page.read() + 1;
                                                *sidebar_page.write() = next;
                                            },
                                            crate::components::icons::MoreHorizontalIcon { class: "w-7 h-7" }
                                            span { "More" }
                                        }
                                    }
                                    if is_last_page && current_page > 0 {
                                        div { class: "border-t border-border my-2" }
                                        button {
                                            class: "flex items-center gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full text-left",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                *sidebar_page.write() = 0;
                                                *sidebar_customizer_open.write() = true;
                                            },
                                            crate::components::icons::SettingsIcon { class: "w-7 h-7" }
                                            span { "Edit Sidebar" }
                                        }
                                    }
                                }
                            }
                        }
                        if auth.is_authenticated {
                            div { class: "relative w-full mt-4",
                                button {
                                    class: "w-full py-6 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-full transition text-lg flex items-center justify-center gap-2 relative z-50",
                                    onclick: move |_| {
                                        let is_open = *radial_menu_open.read();
                                        *radial_menu_open.write() = !is_open;
                                    },
                                    crate::components::icons::PenSquareIcon { class: "w-6 h-6" }
                                    span { "Post" }
                                }
                                crate::components::RadialMenu {
                                    is_open: *radial_menu_open.read(),
                                    on_close: move |_| *radial_menu_open.write() = false,
                                    on_note_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::NoteNew { quote: None });
                                    },
                                    on_article_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::ArticleNew {});
                                    },
                                    on_photo_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::PhotoNew {});
                                    },
                                    on_video_landscape_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::VideoNewLandscape {});
                                    },
                                    on_video_portrait_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::VideoNewPortrait {});
                                    },
                                    on_voice_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::VoiceMessageNew {});
                                    },
                                    on_poll_click: move |_| {
                                        *radial_menu_open.write() = false;
                                        navigator.push(Route::PollNew {});
                                    },
                                }
                            }
                        }
                    }
                }
                if *sidebar_open.read() {
                    div {
                        class: "fixed inset-0 bg-black/50 z-50 lg:hidden",
                        onclick: move |_| {
                            *sidebar_open.write() = false;
                            *sidebar_page.write() = 0;
                        },
                        aside {
                            class: "w-64 bg-background h-full overflow-y-auto pt-safe-top",
                            onclick: move |e| e.stop_propagation(),
                            div { class: "p-4 space-y-6",
                                {
                                    let current_page = (*sidebar_page.read()).min(sidebar_total_pages.saturating_sub(1));
                                    let is_last_page = current_page >= sidebar_total_pages.saturating_sub(1);
                                    let has_more = sidebar_total_pages > 1 && !is_last_page;
                                    let page_items = crate::stores::sidebar_store::compute_page_items(&sidebar_visible, sidebar_slot_count, current_page);
                                    rsx! {
                                        if current_page == 0 {
                                            button {
                                                class: "mb-4 p-2 rounded-lg hover:bg-accent",
                                                onclick: move |_| {
                                                    *sidebar_open.write() = false;
                                                    *sidebar_page.write() = 0;
                                                },
                                                "✕ Close"
                                            }
                                            div {
                                                class: "flex items-center gap-2 hover:opacity-80 transition mb-8 cursor-pointer",
                                                onclick: move |_| {
                                                    *sidebar_open.write() = false;
                                                    *sidebar_page.write() = 0;
                                                    if is_home_page {
                                                        spawn(async move {
                                                            crate::stores::ui::scroll_restore::set_scroll_y(0.0).await;
                                                        });
                                                    } else {
                                                        navigator.push(Route::Home { list: String::new() });
                                                    }
                                                },
                                                div { class: "w-10 h-10 bg-blue-500 rounded-full flex items-center justify-center text-white font-bold text-xl",
                                                    "N"
                                                }
                                                span { class: "text-2xl font-bold text-foreground",
                                                    "nostr.blue"
                                                }
                                            }
                                        } else {
                                            button {
                                                class: "mb-4 p-2 rounded-lg hover:bg-accent flex items-center gap-2",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    let prev = sidebar_page.read().saturating_sub(1);
                                                    *sidebar_page.write() = prev;
                                                },
                                                "← Back"
                                            }
                                        }
                                        nav { class: "flex flex-col gap-2",
                                            for item in page_items {
                                                {
                                                    use crate::stores::sidebar_store::SidebarItem;
                                                    match item {
                                                        SidebarItem::Home => rsx! {
                                                            div {
                                                                key: "{item:?}-mobile",
                                                                class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full cursor-pointer {home_font_weight}",
                                                                onclick: move |_| {
                                                                    *sidebar_open.write() = false;
                                                                    *sidebar_page.write() = 0;
                                                                    if is_home_page {
                                                                        spawn(async move {
                                                                            crate::stores::ui::scroll_restore::set_scroll_y(0.0).await;
                                                                        });
                                                                    } else {
                                                                        navigator.push(Route::Home { list: String::new() });
                                                                    }
                                                                },
                                                                {render_sidebar_icon(&SidebarItem::Home, "w-7 h-7")}
                                                                span { "Home" }
                                                            }
                                                        },
                                                        SidebarItem::Profile => {
                                                            if let Some(pubkey) = &auth.pubkey {
                                                                rsx! {
                                                                    div {
                                                                        key: "{item:?}-mobile",
                                                                        onclick: move |_| {
                                                                            *sidebar_open.write() = false;
                                                                            *sidebar_page.write() = 0;
                                                                        },
                                                                        NavLink {
                                                                            to: Route::AddressViewer {
                                                                                address: crate::utils::nip19_urls::profile_route_id(pubkey),
                                                                            },
                                                                            icon: render_sidebar_icon(&SidebarItem::Profile, "w-7 h-7"),
                                                                            label: "Profile",
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                rsx! {}
                                                            }
                                                        }
                                                        SidebarItem::Notifications => rsx! {
                                                            div {
                                                                key: "{item:?}-mobile",
                                                                onclick: move |_| {
                                                                    *sidebar_open.write() = false;
                                                                    *sidebar_page.write() = 0;
                                                                },
                                                                NavLink {
                                                                    to: Route::Notifications {},
                                                                    icon: render_sidebar_icon(&SidebarItem::Notifications, "w-7 h-7"),
                                                                    label: "Notifications",
                                                                    badge: Some(*notif_count.read()),
                                                                }
                                                            }
                                                        },
                                                        _ => {
                                                            if let Some(route) = item.as_route(auth.pubkey.as_deref()) {
                                                                rsx! {
                                                                    div {
                                                                        key: "{item:?}-mobile",
                                                                        onclick: move |_| {
                                                                            *sidebar_open.write() = false;
                                                                            *sidebar_page.write() = 0;
                                                                        },
                                                                        NavLink {
                                                                            to: route,
                                                                            icon: render_sidebar_icon(&item, "w-7 h-7"),
                                                                            label: item.label(),
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
                                            if has_more {
                                                button {
                                                    class: "flex items-center gap-4 px-4 py-3 rounded-full hover:bg-accent transition text-xl w-full",
                                                    onclick: move |e| {
                                                        e.stop_propagation();
                                                        let next = *sidebar_page.read() + 1;
                                                        *sidebar_page.write() = next;
                                                    },
                                                    crate::components::icons::MoreHorizontalIcon { class: "w-7 h-7".to_string() }
                                                    span { "More" }
                                                    span { class: "ml-auto text-muted-foreground",
                                                        "→"
                                                    }
                                                }
                                            }
                                            if is_last_page && current_page > 0 {
                                                div { class: "border-t border-border my-2" }
                                                button {
                                                    class: "flex items-center gap-4 px-4 py-3 rounded-full hover:bg-accent transition text-xl w-full text-left",
                                                    onclick: move |_| {
                                                        *sidebar_page.write() = 0;
                                                        *sidebar_open.write() = false;
                                                        *sidebar_customizer_open.write() = true;
                                                    },
                                                    crate::components::icons::SettingsIcon { class: "w-7 h-7" }
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
                // Mobile search slideout
                if *mobile_search_open.read() {
                    crate::components::MobileSearchSlideout {
                        show: *mobile_search_open.read(),
                        on_close: move |_| *mobile_search_open.write() = false,
                    }
                }
                main {
                    class: match is_wide_page {
                        true => "w-full flex-1 min-w-0 overflow-x-hidden border-r border-border pb-safe-player",
                        false => "w-full max-w-[600px] shrink grow border-r border-border pb-safe-player",
                    },
                    div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border lg:hidden pt-safe-top",
                        div { class: "flex items-center justify-between p-4",
                            button {
                                class: "p-2 hover:bg-accent rounded-lg",
                                onclick: move |_| *sidebar_open.write() = true,
                                "☰ Menu"
                            }
                            div { class: "text-lg font-bold", "nostr.blue" }
                            div { class: "flex items-center",
                                crate::components::PublishQueueIndicator {}
                                button {
                                    class: "p-2 hover:bg-accent rounded-lg",
                                    onclick: move |_| *mobile_search_open.write() = true,
                                    crate::components::icons::SearchIcon { class: "w-5 h-5".to_string() }
                                }
                            }
                        }
                    }
                    crate::components::OfflineBanner {}
                    ErrorBoundary {
                        handle_error: |ctx: ErrorContext| rsx! {
                            div { class: "container mx-auto px-4 py-8 max-w-5xl text-center",
                                h2 { class: "text-xl font-semibold mb-2", "Something went wrong" }
                                if let Some(err) = ctx.error() {
                                    p { class: "text-muted-foreground", "{err}" }
                                }
                                button {
                                    class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                    onclick: move |_| ctx.clear_errors(),
                                    "Try again"
                                }
                            }
                        },
                        Outlet::<Route> {}
                    }
                }
                if is_topics_page && !is_settings_page {
                    aside { class: "w-[350px] shrink-0 hidden xl:block",
                        div { class: "flex flex-col gap-4 sticky top-0 pt-4 pb-4 h-screen overflow-hidden px-4 z-0",
                            div { class: "flex-1 overflow-y-auto scrollbar-hide",
                                crate::components::TopicSidebar {
                                    current_topic: match &current_route {
                                        Route::TopicFeed { topic } | Route::TopicPostDetail { topic, .. } => Some(topic.clone()),
                                        _ => None,
                                    }
                                }
                            }
                        }
                    }
                } else if !is_wide_page && !is_settings_page {
                    aside { class: "w-[350px] shrink-0 hidden xl:block",
                        div { class: "sticky top-0 z-0 flex h-screen min-h-0 flex-col gap-4 overflow-hidden px-4 pt-4 pb-4",
                            div { class: "shrink-0", crate::components::SearchInput {} }
                            div { class: "min-h-0 flex-1 overflow-hidden", crate::components::RightDiscoverySidebar {} }
                            div { class: "text-xs text-muted-foreground flex flex-wrap gap-2 mt-auto shrink-0",
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
                                    to: Route::About {},
                                    class: "hover:underline",
                                    "About"
                                }
                                div { class: "w-full mt-1",
                                    "2025 nostr.blue - {env!(\"CARGO_PKG_VERSION\")}"
                                }
                            }
                        }
                    }
                }
            }
            crate::components::PersistentMusicPlayer {}
            crate::components::MusicZapDialog {}
            crate::components::PwaUpdateBanner {}
            if *sidebar_customizer_open.read() {
                crate::components::SidebarCustomizerModal { on_close: move |_| *sidebar_customizer_open.write() = false }
            }
            if auth.is_authenticated && crate::components::blobbi::companion::companion_visible() {
                crate::components::blobbi::companion::CompanionLayer {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fallback_route_for, Route};

    #[test]
    fn top_level_routes_do_not_fallback() {
        assert_eq!(
            fallback_route_for(&Route::Home {
                list: String::new()
            }),
            None
        );
        assert_eq!(fallback_route_for(&Route::CodeHome {}), None);
    }

    #[test]
    fn detail_routes_fallback_to_section_roots() {
        assert_eq!(
            fallback_route_for(&Route::ArticleDetail {
                naddr: "article".to_string(),
            }),
            Some(Route::Articles {})
        );
        assert_eq!(
            fallback_route_for(&Route::SettingsAi {}),
            Some(Route::Settings {})
        );
        assert_eq!(
            fallback_route_for(&Route::SettingsRelays {}),
            Some(Route::Settings {})
        );
        assert_eq!(
            fallback_route_for(&Route::RelayDetail {
                relay_id: "wss%3A%2F%2Frelay.example.com".to_string(),
            }),
            Some(Route::SettingsRelays {})
        );
        assert_eq!(
            fallback_route_for(&Route::RelayExplorer {}),
            Some(Route::SettingsRelays {})
        );
        assert_eq!(
            fallback_route_for(&Route::ShopProductDetail {
                naddr: "product".to_string(),
            }),
            Some(Route::ShopHome {})
        );
        assert_eq!(
            fallback_route_for(&Route::AboutDonate {}),
            Some(Route::About {})
        );
        assert_eq!(
            fallback_route_for(&Route::ZapGoalsHome {}),
            Some(Route::About {})
        );
        assert_eq!(
            fallback_route_for(&Route::ZapGoalsNew {}),
            Some(Route::ZapGoalsHome {})
        );
    }
}
