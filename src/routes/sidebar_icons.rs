use dioxus::prelude::*;

use crate::stores::sidebar_store::SidebarItem;

/// Helper function to render sidebar icons for dynamic sidebar
pub(super) fn render_sidebar_icon(item: &SidebarItem, class: &str) -> Element {
    match item {
        SidebarItem::Home => {
            rsx! {
                crate::components::icons::HomeIcon { class: class.to_string() }
            }
        }
        SidebarItem::Explore => {
            rsx! {
                crate::components::icons::CompassIcon { class: class.to_string() }
            }
        }
        SidebarItem::Articles => {
            rsx! {
                crate::components::icons::BookOpenIcon { class: class.to_string() }
            }
        }
        SidebarItem::Music => {
            rsx! {
                crate::components::icons::MusicIcon { class: class.to_string() }
            }
        }
        SidebarItem::Photos => {
            rsx! {
                crate::components::icons::CameraIcon { class: class.to_string() }
            }
        }
        SidebarItem::Videos => {
            rsx! {
                crate::components::icons::VideoIcon { class: class.to_string() }
            }
        }
        SidebarItem::Live => {
            rsx! {
                crate::components::icons::LiveVideoIcon { class: class.to_string() }
            }
        }
        SidebarItem::Verts => {
            rsx! {
                crate::components::icons::VertsIcon { class: class.to_string() }
            }
        }
        SidebarItem::Notifications => {
            rsx! {
                crate::components::icons::BellIcon { class: class.to_string() }
            }
        }
        SidebarItem::Messages => {
            rsx! {
                crate::components::icons::MailIcon { class: class.to_string() }
            }
        }
        SidebarItem::Bookmarks => {
            rsx! {
                crate::components::icons::BookmarkIcon { class: class.to_string() }
            }
        }
        SidebarItem::Profile => {
            rsx! {
                crate::components::icons::UserIcon { class: class.to_string() }
            }
        }
        SidebarItem::Settings => {
            rsx! {
                crate::components::icons::SettingsIcon { class: class.to_string() }
            }
        }
        SidebarItem::VoiceMessages => {
            rsx! {
                crate::components::icons::MicrophoneIcon { class: class.to_string() }
            }
        }
        SidebarItem::Polls => {
            rsx! {
                crate::components::icons::PollIcon { class: class.to_string() }
            }
        }
        SidebarItem::Workouts => {
            rsx! {
                crate::components::icons::RunIcon { class: class.to_string() }
            }
        }
        SidebarItem::WebBookmarks => {
            rsx! {
                crate::components::icons::BookmarkIcon { class: class.to_string() }
            }
        }
        SidebarItem::Podcasts => {
            rsx! {
                crate::components::icons::MicrophoneIcon { class: class.to_string() }
            }
        }
        SidebarItem::Radio => {
            rsx! {
                crate::components::icons::RadioIcon { class: class.to_string() }
            }
        }
        #[cfg(feature = "cashu")]
        SidebarItem::Wallet => {
            rsx! {
                crate::components::icons::WalletIcon { class: class.to_string() }
            }
        }
        #[cfg(not(feature = "cashu"))]
        SidebarItem::Wallet => {
            rsx! { div {} }
        }
        SidebarItem::Mostro => {
            rsx! {
                crate::components::icons::ArrowsExchangeIcon { class: class.to_string() }
            }
        }
        SidebarItem::Communities => {
            rsx! {
                crate::components::icons::UsersIcon { class: class.to_string() }
            }
        }
        SidebarItem::Topics => {
            rsx! {
                crate::components::icons::HashIcon { class: class.to_string() }
            }
        }
        SidebarItem::Events => {
            rsx! {
                crate::components::icons::MapPinIcon { class: class.to_string() }
            }
        }
        SidebarItem::Calendar => {
            rsx! {
                crate::components::icons::CalendarIcon { class: class.to_string() }
            }
        }
        SidebarItem::Recipes => {
            rsx! {
                crate::components::icons::ChefHatIcon { class: class.to_string() }
            }
        }
        SidebarItem::PinBoards => {
            rsx! {
                crate::components::icons::PinIcon { class: class.to_string(), filled: false }
            }
        }
        SidebarItem::Trending => {
            rsx! {
                crate::components::icons::TrendingUpIcon { class: class.to_string() }
            }
        }
        SidebarItem::Nips => {
            rsx! {
                crate::components::icons::FileTextIcon { class: class.to_string() }
            }
        }
        SidebarItem::Badges => {
            rsx! {
                crate::components::icons::AwardIcon { class: class.to_string() }
            }
        }
        SidebarItem::Citations => {
            rsx! {
                crate::components::icons::QuoteIcon { class: class.to_string() }
            }
        }
        SidebarItem::Code => {
            rsx! {
                crate::components::icons::CodeBracketIcon { class: class.to_string() }
            }
        }
        SidebarItem::Lists => {
            rsx! {
                crate::components::icons::ListIcon { class: class.to_string() }
            }
        }
        SidebarItem::Packs => {
            rsx! {
                crate::components::icons::UsersIcon { class: class.to_string() }
            }
        }
        SidebarItem::Chats => {
            rsx! {
                crate::components::icons::MessageCircleIcon { class: class.to_string() }
            }
        }
        SidebarItem::Dvm => {
            rsx! {
                crate::components::icons::CpuIcon { class: class.to_string() }
            }
        }
        SidebarItem::Wiki => {
            rsx! {
                crate::components::icons::BookIcon { class: class.to_string() }
            }
        }
        SidebarItem::Publications => {
            rsx! {
                crate::components::icons::BookOpenIcon { class: class.to_string() }
            }
        }
        SidebarItem::Shop => {
            rsx! {
                crate::components::icons::ShoppingBagIcon { class: class.to_string() }
            }
        }
        SidebarItem::Blossom => {
            rsx! {
                crate::components::icons::CloudIcon { class: class.to_string() }
            }
        }
        SidebarItem::Bible => {
            rsx! {
                crate::components::icons::BibleIcon { class: class.to_string() }
            }
        }
        SidebarItem::Quran => {
            rsx! {
                crate::components::icons::QuranIcon { class: class.to_string() }
            }
        }
        SidebarItem::Highlights => {
            rsx! {
                crate::components::icons::HighlighterIcon { class: class.to_string() }
            }
        }
        SidebarItem::AIChat => {
            rsx! {
                crate::components::icons::SparklesIcon { class: class.to_string() }
            }
        }
        SidebarItem::Nests => {
            rsx! {
                crate::components::icons::NestIcon { class: class.to_string() }
            }
        }
        SidebarItem::Groups => {
            rsx! {
                crate::components::icons::UsersIcon { class: class.to_string() }
            }
        }
        SidebarItem::Weather => {
            rsx! {
                crate::components::icons::CloudSunIcon { class: class.to_string() }
            }
        }
        SidebarItem::Games => {
            rsx! {
                crate::components::icons::GamepadIcon { class: class.to_string() }
            }
        }
        SidebarItem::Places => {
            rsx! {
                crate::components::icons::MapPinIcon { class: class.to_string() }
            }
        }
        SidebarItem::Deflock => {
            rsx! {
                crate::components::icons::EyeIcon { class: class.to_string() }
            }
        }
    }
}
