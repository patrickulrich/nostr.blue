// UI Components
// This module contains all reusable UI components

pub mod note;
pub mod note_card;
pub mod note_composer;
pub mod pinned_notes_carousel;
pub mod rich_content;
pub mod reply_composer;
pub mod comment_composer;
pub mod confirm_modal;
pub mod trending_notes;
pub mod search_input;
pub mod threaded_comment;
pub mod icons;
pub mod article_card;
pub mod article_content;
pub mod photo_card;
pub mod video_card;
// Live streaming components (NIP-53)
pub mod live;
pub mod voice_message_card;
pub mod voice_recorder;
pub mod voice_reply_composer;
pub mod webbookmark_card;
pub mod webbookmark_modal;
pub mod zap_modal;
pub mod music_player;
pub mod track_card;
pub mod artist_card;
pub mod album_card;
pub mod music_zap_dialog;
pub mod music_source_tabs;
pub mod unified_track_card;
pub mod client_initializing;
pub mod media_uploader;
pub mod profile_editor_modal;
pub mod emoji_picker;
pub mod reaction_picker;
pub mod reaction_button;
pub mod reaction_defaults_modal;
pub mod gif_picker;
pub mod mention_autocomplete;
pub mod share_modal;
pub mod radial_menu;
pub mod markdown_editor;
pub mod note_menu;
pub mod content_menu;
pub mod virtual_list;
pub mod poll_timer;
pub mod poll_card;
pub mod poll_option_list;
pub mod poll_creator_modal;
pub mod wallet_balance_card;
pub mod token_list;
pub mod transaction_history;
// Cashu wallet components (eCash)
pub mod cashu;
pub mod nwc_setup_modal;
pub mod password_modal;  // NIP-49 password entry for encrypted keys
pub mod report_modal;
pub mod add_to_list_modal;
pub mod create_list_modal;
pub mod dvm_selector_modal;
pub mod gif_upload_modal;
pub mod podcast_show_card;
pub mod podcast_episode_card;
pub mod podcast_episode_list;
pub mod podcast_chapters;
pub mod podcast_transcript;
pub mod podcast_soundbites;
pub mod podcast_persons;
pub mod podcast_v4v;
pub mod podcast_add_feed_modal;
pub mod radio_card;
pub mod external_content_card;
pub mod nip_card;

// Badge components (NIP-58)
pub mod profile_badges;
pub mod badge_detail_modal;

// Code/Git hosting components (NIP-34 + NIP-C0)
pub mod code;

// Publication components (NKBIP-01 Kind 30040/30041)
pub mod asciidoc_content;
pub mod publication_card;
pub mod publication_toc;
pub mod publication_section;

// Wiki components (NIP-54 Kind 30818)
pub mod wiki_card;
pub mod wiki_content;
pub mod wiki_backlinks;

// Citation components (NKBIP-03 Kinds 30-33)
pub mod citation;

// Shop/Marketplace components (NIP-99)
pub mod shop;

// Book reference picker (NKBIP-08)
pub mod book_picker_modal;

// pub use note::NoteDisplay;
pub use note_card::{NoteCard, NoteCardSkeleton};
pub use note_composer::NoteComposer;
pub use pinned_notes_carousel::PinnedNotesCarousel;
pub use rich_content::RichContent;
pub use reply_composer::ReplyComposer;
pub use comment_composer::CommentComposer;
pub use confirm_modal::ConfirmModal;
pub use trending_notes::TrendingNotes;
pub use search_input::SearchInput;
pub use threaded_comment::ThreadedComment;
pub use article_card::{ArticleCard, ArticleCardSkeleton};
pub use article_content::ArticleContent;
pub use photo_card::PhotoCard;
pub use video_card::VideoCard;
// Live streaming component re-exports
pub use live::{MiniLiveStreamCard, LiveStreamShareModal, LiveStreamPlayer, LiveChat, StreamStatus};
pub use voice_message_card::VoiceMessageCard;
pub use voice_recorder::VoiceRecorder;
pub use voice_reply_composer::VoiceReplyComposer;
pub use webbookmark_card::{WebBookmarkCard, WebBookmarkCardSkeleton};
pub use webbookmark_modal::{WebBookmarkModal, BookmarkModalMode};
pub use zap_modal::ZapModal;
pub use music_player::PersistentMusicPlayer;
pub use track_card::TrackCard;
pub use artist_card::{ArtistCard, ArtistCardSkeleton};
pub use album_card::{AlbumCard, AlbumCardSkeleton};
pub use music_zap_dialog::MusicZapDialog;
pub use music_source_tabs::{DiscoveryTabs, DiscoveryTab};
pub use unified_track_card::{UnifiedTrackCard, UnifiedTrackCardSkeleton};
pub use client_initializing::ClientInitializing;
pub use media_uploader::MediaUploader;
pub mod article_cover_uploader;
pub use article_cover_uploader::ArticleCoverUploader;
pub mod publish_confirm_dialog;
pub use publish_confirm_dialog::{PublishConfirmDialog, PublishConfig};
pub mod markdown_toolbar;
// Re-export toolbar and formatting utilities (some kept for future programmatic formatting features)
#[allow(unused_imports)]
pub use markdown_toolbar::{MarkdownToolbar, MarkdownFormat, apply_markdown_format, set_textarea_cursor, get_textarea_cursor};
pub mod nostr_mention_dialog;
// MentionType kept for future mention type differentiation in UI
#[allow(unused_imports)]
pub use nostr_mention_dialog::{NostrMentionDialog, MentionSelection, MentionType};
pub mod image_upload_dialog;
pub use image_upload_dialog::{ImageUploadDialog, ImageInsertData};
pub use profile_editor_modal::ProfileEditorModal;
pub use emoji_picker::EmojiPicker;
pub use reaction_picker::InlineReactionPicker;
pub use reaction_button::ReactionButton;
pub use reaction_defaults_modal::ReactionDefaultsModal;
pub mod sidebar_customizer_modal;
pub use sidebar_customizer_modal::SidebarCustomizerModal;
pub use gif_picker::GifPicker;
pub use mention_autocomplete::MentionAutocomplete;
pub use share_modal::ShareModal;
pub mod content_share_modal;
pub use content_share_modal::{ContentShareModal, ContentType};
pub use radial_menu::RadialMenu;
pub use markdown_editor::MarkdownEditor;
pub use note_menu::NoteMenu;
pub use poll_timer::PollTimer;
pub use poll_card::PollCard;
pub use poll_option_list::{PollOptionList, PollOptionData};
pub use wallet_balance_card::WalletBalanceCard;
pub use token_list::TokenList;
pub use transaction_history::TransactionHistory;
// Cashu wallet component re-exports
pub use cashu::{
    CashuSetupWizard, CashuSendModal, CashuReceiveModal, CashuReceiveLightningModal,
    CashuSendLightningModal, CashuOptimizeModal, CashuAddMintModal, CashuMintDiscoveryModal,
    CashuTransferModal, CashuCreateRequestModal, CashuPayRequestModal, CashuTermsModal, CashuTokenCard,
};
pub use nwc_setup_modal::NwcSetupModal;
pub use report_modal::ReportModal;
pub use add_to_list_modal::AddToListModal;
pub use create_list_modal::CreateListModal;
pub mod add_to_people_list_modal;
pub use add_to_people_list_modal::AddToPeopleListModal;
pub mod people_list_members_modal;
pub use people_list_members_modal::PeopleListMembersModal;
pub use poll_creator_modal::PollCreatorModal;
pub use dvm_selector_modal::DvmSelectorModal;
pub mod dialog;
pub mod modal;
// Modal components kept for future dialog refactoring
#[allow(unused_imports)]
pub use modal::{Modal, ModalHeader, ModalBody, ModalFooter};
pub mod toast;

// Podcast components
pub use podcast_show_card::{PodcastShowCard, PodcastShowCardSkeleton, PodcastShow};
pub use podcast_episode_card::{PodcastEpisodeCard, PodcastEpisodeCardSkeleton, DisplayEpisode};
pub use podcast_episode_list::PodcastEpisodeList;
pub use podcast_chapters::PodcastChapters;
pub use podcast_transcript::PodcastTranscript;
pub use podcast_soundbites::{PodcastSoundbites, FeaturedSoundbite};
pub use podcast_persons::{PodcastPersons, InlineCredits};
pub use podcast_v4v::{V4VInfo, V4VBoostButton};
pub use podcast_add_feed_modal::PodcastAddFeedModal;
pub use radio_card::{RadioCard, RadioCardSkeleton};
pub use external_content_card::ExternalContentList;

// Code/Git hosting component exports
pub use code::{
    CodeRepoCard, CodeRepoCardCompact, CodeStatusBadge, CodeSnippetCard,
    CodeIssueRow, CodePullRow, CodeFileTree, FileTreeSkeleton, FilePathBreadcrumb,
    BranchSelector, CodeFileViewer, CodeFileViewerSkeleton,
};

// NIP components
pub use nip_card::{OfficialNipCard, CustomNipCard, NipCardSkeleton};

// Citation component re-exports (NKBIP-03 Kinds 30-33)
pub use citation::{CitationPickerModal, CitationSelection};

// Book reference picker (NKBIP-08)
pub use book_picker_modal::{BookPickerModal, BookSelection};

// Badge component exports (NIP-58)
pub use profile_badges::ProfileBadgesSection;

// P2P trading components (NIP-69)
pub mod p2p;
pub use p2p::{
    P2POrderCard, P2POrderCardSkeleton, P2PStatusBadge, P2PTypeBadge,
    P2PLayerBadge, P2PNetworkBadge, P2POrderFilters, P2PDepthChart, P2PDepthChartSkeleton,
};

// Calendar/Events components (NIP-52 + NIP-53)
pub mod event_card;
pub mod event_map;
pub mod calendar_view;
pub mod calendar_mini;

#[allow(unused_imports)] // Skeleton re-exported for public API consistency
pub use event_card::{EventCard, EventCardCompact, EventCardSkeleton, EventCardCompactSkeleton};
pub use event_map::EventMap;
pub use calendar_view::{CalendarView, CalendarViewMode, CalendarViewSkeleton};
pub use calendar_mini::MiniCalendar;

// Community components (NIP-72)
pub mod community;
pub use community::{
    CommunityCard, CommunityCardSkeleton, CommunityCardWithMembership, JoinButton,
    CommunityPostCard, CommunityPostCardSkeleton, UserRoleBadge,
    CommunityPostComposer, CommunityPostComposerInline,
};

// Pin Board components (Kind 33889 Pinstr-compatible)
pub mod pin_board_card;
pub mod pin_board_item_card;
pub mod pin_board_item_selector;
pub mod pin_menu;
pub mod board_slideover;

// Recipe components
pub mod recipe_card;
pub mod recipe_tag_chip;
pub mod recipe_detail_view;
pub(crate) mod recipe_ingredients_editor;
pub(crate) mod recipe_directions_editor;
pub(crate) mod recipe_tag_selector;
pub mod recipe_form;
pub mod cookbook_card;
pub mod create_cookbook_modal;
pub mod add_to_cookbook_modal;
pub mod discover_recipe_card;

pub use recipe_card::{RecipeCard, RecipeCardSkeleton};
pub use recipe_tag_chip::RecipeTagChipExplore;
pub use recipe_detail_view::{RecipeDetailView, RecipeDetailViewSkeleton};
pub use recipe_form::{RecipeForm, RecipeFormData};
pub use cookbook_card::{CookbookCard, CookbookCardSkeleton};
pub use create_cookbook_modal::CreateCookbookModal;
pub use add_to_cookbook_modal::AddToCookbookModal;
pub use discover_recipe_card::{DiscoverRecipeCard, DiscoverRecipeCardSkeleton};
// Note: RecipeIngredientsEditor, RecipeDirectionsEditor, RecipeTagSelector are internal to RecipeForm

// Pinboard component exports
pub use pin_board_card::{PinBoardMosaicGrid, HashtagBadge};
pub use pin_board_item_card::{PinMosaicGrid, PinCardMosaicSkeleton};
pub use pin_menu::{PinMenu, PinToBoardRequest};
pub use board_slideover::BoardSlideover;

// Publication component exports (NKBIP-01)
pub use asciidoc_content::{AsciiDocContent, WikilinksList, CitationMetadata};
pub use publication_card::{PublicationCardCompact, PublicationCardSkeleton, PublicationGrid};
pub use publication_toc::{PublicationTocHorizontal, PublicationProgress, PublicationTocSkeleton, PublicationTocDynamic};
pub use publication_section::{PublicationSectionContent, SectionNavigation, PublicationSectionSkeleton, SectionMetadata, SectionOutline};

// Wiki component exports (NIP-54)
pub use wiki_card::{WikiCardSearchResult, WikiCardSkeleton, WikiGrid, WikiMetadataCard};
pub use wiki_content::{WikiPageContent, WikiOutline, WikiForwardLinks, WikiPageNotFound, WikiPageSkeleton};
pub use wiki_backlinks::WikiBacklinks;

// PWA components
pub mod pwa_update_banner;
pub use pwa_update_banner::PwaUpdateBanner;
