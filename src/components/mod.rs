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
pub mod live_stream_card;
pub mod mini_live_stream_card;
pub mod live_stream_share_modal;
pub mod stream_status;
pub mod live_stream_player;
pub mod live_chat;
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
pub mod cashu_setup_wizard;
pub mod cashu_send_modal;
pub mod cashu_receive_modal;
pub mod cashu_receive_lightning_modal;
pub mod cashu_send_lightning_modal;
pub mod cashu_optimize_modal;
pub mod cashu_add_mint_modal;
pub mod cashu_mint_discovery_modal;
pub mod cashu_transfer_modal;
pub mod cashu_create_request_modal;
pub mod cashu_pay_request_modal;
pub mod cashu_terms_modal;
pub mod cashu_token_card;
pub mod nwc_setup_modal;
pub mod report_modal;
pub mod add_to_list_modal;
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
pub mod external_content_card;
pub mod nip_card;

// Badge components (NIP-58)
pub mod profile_badges;
pub mod badge_detail_modal;

// Code/Git hosting components (NIP-34 + NIP-C0)
pub mod code_repo_card;
pub mod code_status_badge;
pub mod code_snippet_card;
pub mod code_issue_card;
pub mod code_pull_card;
pub mod code_file_tree;
pub mod code_file_viewer;

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
pub mod citation_card;
pub mod citation_editor_modal;
pub mod citation_picker_modal;

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
pub use mini_live_stream_card::MiniLiveStreamCard;
pub use live_stream_share_modal::LiveStreamShareModal;
pub use stream_status::StreamStatus;
pub use live_stream_player::LiveStreamPlayer;
pub use live_chat::LiveChat;
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
pub use radial_menu::RadialMenu;
pub use markdown_editor::MarkdownEditor;
pub use note_menu::NoteMenu;
// ContentMenu and ContentMenuType are used internally by components
pub use poll_timer::PollTimer;
pub use poll_card::PollCard;
pub use poll_option_list::{PollOptionList, PollOptionData};
pub use wallet_balance_card::WalletBalanceCard;
pub use token_list::TokenList;
pub use transaction_history::TransactionHistory;
pub use cashu_setup_wizard::CashuSetupWizard;
pub use cashu_send_modal::CashuSendModal;
pub use cashu_receive_modal::CashuReceiveModal;
pub use cashu_receive_lightning_modal::CashuReceiveLightningModal;
pub use cashu_send_lightning_modal::CashuSendLightningModal;
pub use cashu_optimize_modal::CashuOptimizeModal;
pub use cashu_add_mint_modal::CashuAddMintModal;
pub use cashu_mint_discovery_modal::CashuMintDiscoveryModal;
pub use cashu_transfer_modal::CashuTransferModal;
pub use cashu_create_request_modal::CashuCreateRequestModal;
pub use cashu_pay_request_modal::CashuPayRequestModal;
pub use cashu_terms_modal::CashuTermsModal;
pub use cashu_token_card::CashuTokenCard;
pub use nwc_setup_modal::NwcSetupModal;
pub use report_modal::ReportModal;
pub use add_to_list_modal::AddToListModal;
pub use poll_creator_modal::PollCreatorModal;
pub use dvm_selector_modal::DvmSelectorModal;
pub mod dialog;
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
pub use external_content_card::ExternalContentList;

// Code/Git hosting component exports
pub use code_repo_card::{CodeRepoCard, CodeRepoCardCompact};
pub use code_status_badge::CodeStatusBadge;
pub use code_snippet_card::CodeSnippetCard;
pub use code_issue_card::CodeIssueRow;
pub use code_pull_card::CodePullRow;
pub use code_file_tree::{CodeFileTree, FileTreeSkeleton, FilePathBreadcrumb, BranchSelector};
pub use code_file_viewer::{CodeFileViewer, CodeFileViewerSkeleton};

// NIP components
pub use nip_card::{OfficialNipCard, CustomNipCard, NipCardSkeleton};

// Citation components (NKBIP-03 Kinds 30-33)
// Access via crate::components::citation_card, citation_editor_modal, citation_picker_modal
pub use citation_picker_modal::{CitationPickerModal, CitationSelection};

// Book reference picker (NKBIP-08)
pub use book_picker_modal::{BookPickerModal, BookSelection};

// Badge component exports (NIP-58)
pub use profile_badges::ProfileBadgesSection;

// P2P trading components (NIP-69)
pub mod p2p_order_card;
pub mod p2p_status_badge;
pub mod p2p_order_filters;
pub mod p2p_depth_chart;

pub use p2p_order_card::{P2POrderCard, P2POrderCardSkeleton};
pub use p2p_status_badge::{P2PStatusBadge, P2PTypeBadge, P2PLayerBadge, P2PNetworkBadge};
pub use p2p_order_filters::P2POrderFilters;
pub use p2p_depth_chart::{P2PDepthChart, P2PDepthChartSkeleton};

// Calendar/Events components (NIP-52 + NIP-53)
pub mod event_card;
pub mod event_map;
pub mod calendar_view;
pub mod calendar_mini;

#[allow(unused_imports)]
pub use event_card::{EventCard, EventCardCompact, EventCardSkeleton, EventCardCompactSkeleton};
pub use event_map::EventMap;
pub use calendar_view::{CalendarView, CalendarViewMode, CalendarViewSkeleton};
pub use calendar_mini::MiniCalendar;

// Community components (NIP-72)
pub mod community_card;
pub mod community_post_card;
pub mod community_post_composer;

pub use community_card::{CommunityCard, CommunityCardSkeleton, CommunityCardWithMembership, JoinButton};
pub use community_post_card::{CommunityPostCard, CommunityPostCardSkeleton, UserRoleBadge};
pub use community_post_composer::{CommunityPostComposer, CommunityPostComposerInline};

// Pin Board components (Kind 33889 Pinstr-compatible)
pub mod pin_board_card;
pub mod pin_board_item_card;
pub mod pin_board_item_selector;
pub mod board_slideover;

// Recipe components
pub mod recipe_card;
pub mod recipe_tag_chip;
pub mod recipe_detail_view;
pub mod recipe_ingredients_editor;
pub mod recipe_directions_editor;
pub mod recipe_tag_selector;
pub mod recipe_form;
pub mod collection_card;
pub mod tag_section_card;
pub mod popular_chef_avatar;
pub mod discover_recipe_card;

pub use recipe_card::{RecipeCard, RecipeCardSkeleton};
pub use recipe_tag_chip::RecipeTagChipExplore;
pub use recipe_detail_view::{RecipeDetailView, RecipeDetailViewSkeleton};
pub use recipe_form::{RecipeForm, RecipeFormData};
pub use collection_card::{CollectionCard, CollectionCardSkeleton};
pub use tag_section_card::TagSectionCard;
pub use popular_chef_avatar::{PopularChefAvatar, PopularChefAvatarSkeleton};
pub use discover_recipe_card::{DiscoverRecipeCard, DiscoverRecipeCardSkeleton};
// Note: RecipeIngredientsEditor, RecipeDirectionsEditor, RecipeTagSelector are internal to RecipeForm

// Pinboard component exports
pub use pin_board_card::{PinBoardMosaicGrid, HashtagBadge};
pub use pin_board_item_card::{PinCardSkeleton, PinGrid};
pub use board_slideover::BoardSlideover;

// Publication component exports (NKBIP-01)
pub use asciidoc_content::{AsciiDocContent, WikilinksList, CitationMetadata};
pub use publication_card::{PublicationCardCompact, PublicationCardSkeleton, PublicationGrid};
pub use publication_toc::{PublicationToc, PublicationTocHorizontal, PublicationProgress, PublicationTocSkeleton};
pub use publication_section::{PublicationSectionContent, SectionNavigation, PublicationSectionSkeleton, SectionMetadata, SectionOutline};

// Wiki component exports (NIP-54)
pub use wiki_card::{WikiCardSearchResult, WikiCardSkeleton, WikiGrid, WikiMetadataCard};
pub use wiki_content::{WikiPageContent, WikiOutline, WikiForwardLinks, WikiPageNotFound, WikiPageSkeleton};
pub use wiki_backlinks::WikiBacklinks;
