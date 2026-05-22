use dioxus::prelude::*;

use super::Route;

#[component]
pub(super) fn NavLink(
    to: Route,
    icon: Element,
    label: &'static str,
    #[props(default = None)] badge: Option<usize>,
) -> Element {
    let current_route = use_route::<Route>();
    let is_active = match (&to, &current_route) {
        (Route::Home { .. }, Route::Home { .. }) => true,
        (Route::Explore {}, Route::Explore {}) => true,
        (Route::Articles {}, Route::Articles {}) => true,
        (Route::Articles {}, Route::ArticleDetail { .. }) => true,
        (Route::Notifications {}, Route::Notifications {}) => true,
        (Route::DMs {}, Route::DMs {}) => true,
        (Route::Photos {}, Route::Photos {}) => true,
        (Route::Photos {}, Route::PhotoDetail { .. }) => true,
        (Route::PhotoDetail { photo_id: p1 }, Route::PhotoDetail { photo_id: p2 }) => p1 == p2,
        (Route::MusicHome {}, Route::MusicHome {})
        | (Route::MusicHome {}, Route::MusicRadio {})
        | (Route::MusicHome {}, Route::MusicLeaderboard {})
        | (Route::MusicHome {}, Route::MusicSearch { .. })
        | (Route::MusicHome {}, Route::MusicArtist { .. })
        | (Route::MusicHome {}, Route::MusicAlbum { .. })
        | (Route::MusicHome {}, Route::MusicRssAlbum { .. })
        | (Route::MusicHome {}, Route::MusicRssArtist { .. })
        | (Route::MusicHome {}, Route::MusicTrackNew {})
        | (Route::MusicHome {}, Route::MusicTrackDetail { .. })
        | (Route::MusicHome {}, Route::MusicPlaylistNew {})
        | (Route::MusicHome {}, Route::MusicPlaylistDetail { .. }) => true,
        (Route::Bookmarks {}, Route::Bookmarks {}) => true,
        (Route::Videos {}, Route::Videos {}) => true,
        (Route::Videos {}, Route::VideosVerts {}) => true,
        (Route::Videos {}, Route::VideoDetail { .. }) => true,
        (Route::VideosVerts {}, Route::VideosVerts {}) => true,
        (Route::VideoDetail { video_id: v1 }, Route::VideoDetail { video_id: v2 }) => v1 == v2,
        (Route::VideosLive {}, Route::VideosLive {})
        | (Route::VideosLive {}, Route::VideosLiveTag { .. })
        | (Route::VideosLive {}, Route::LiveStreamDetail { .. }) => true,
        (Route::VideosLiveTag { tag: t1 }, Route::VideosLiveTag { tag: t2 }) => t1 == t2,
        (Route::LiveStreamDetail { note_id: n1 }, Route::LiveStreamDetail { note_id: n2 }) => {
            n1 == n2
        }
        (Route::LiveStreamNew {}, Route::LiveStreamNew {}) => true,
        #[cfg(feature = "cashu")]
        (Route::CashuWallet {}, Route::CashuWallet {}) => true,
        (Route::Settings {}, Route::Settings {}) => true,
        (Route::BlossomPage {}, Route::BlossomPage {}) => true,
        (Route::Profile { pubkey: p1 }, Route::Profile { pubkey: p2 }) => {
            crate::utils::nip19_urls::parse_profile_id(p1)
                == crate::utils::nip19_urls::parse_profile_id(p2)
        }
        (Route::BibleHome {}, Route::BibleHome {})
        | (Route::BibleHome {}, Route::BibleChapter { .. })
        | (Route::BibleHome {}, Route::BibleSearch {}) => true,
        (Route::Highlights {}, Route::Highlights {}) => true,
        (Route::BlobbiHome {}, Route::BlobbiHome {}) => true,
        (Route::NestsHome {}, Route::NestsHome {})
        | (Route::NestsHome {}, Route::NestDetail { .. })
        | (Route::NestsHome {}, Route::NestCreate { .. }) => true,
        // Packs group
        (Route::PacksHome {}, Route::PacksHome {})
        | (Route::PacksHome {}, Route::PackNew {})
        | (Route::PacksHome {}, Route::PackDetail { .. }) => true,
        // Chats group
        (Route::Chats {}, Route::Chats {})
        | (Route::Chats {}, Route::ChatNew {})
        | (Route::Chats {}, Route::ChatDetail { .. }) => true,
        // Groups
        (Route::Groups {}, Route::Groups {})
        | (Route::Groups {}, Route::GroupDetail { .. }) => true,
        _ => false,
    };
    let font_class = if is_active { "font-bold" } else { "" };
    rsx! {
        Link {
            to,
            class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-accent transition text-xl w-full {font_class}",
            {icon}
            span { "{label}" }
            if let Some(count) = badge {
                if count > 0 {
                    span { class: "ml-auto min-w-[24px] h-6 px-2 bg-primary text-primary-foreground rounded-full text-sm font-bold flex items-center justify-center",
                        "{count}"
                    }
                }
            }
        }
    }
}
