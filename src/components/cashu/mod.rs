//! Cashu wallet components
//!
//! This module contains all eCash/Cashu-related UI components
//! for the wallet functionality.

pub mod setup_wizard;
pub mod send_modal;
pub mod receive_modal;
pub mod receive_lightning_modal;
pub mod send_lightning_modal;
pub mod optimize_modal;
pub mod add_mint_modal;
pub mod mint_discovery_modal;
pub mod transfer_modal;
pub mod create_request_modal;
pub mod pay_request_modal;
pub mod terms_modal;
pub mod token_card;
pub mod wallet_health;
pub mod wallet_health_modal;
pub mod nutzap_send_modal;
pub mod nutzap_settings_modal;
pub mod nutzap_inbox;

// Re-export main component types for convenience
pub use setup_wizard::CashuSetupWizard;
pub use send_modal::CashuSendModal;
pub use receive_modal::CashuReceiveModal;
pub use receive_lightning_modal::CashuReceiveLightningModal;
pub use send_lightning_modal::CashuSendLightningModal;
pub use optimize_modal::CashuOptimizeModal;
pub use add_mint_modal::CashuAddMintModal;
pub use mint_discovery_modal::CashuMintDiscoveryModal;
pub use transfer_modal::CashuTransferModal;
pub use create_request_modal::CashuCreateRequestModal;
pub use pay_request_modal::CashuPayRequestModal;
pub use terms_modal::CashuTermsModal;
pub use token_card::CashuTokenCard;
pub use wallet_health::WalletHealthIndicator;
pub use wallet_health_modal::WalletHealthModal;
// NIP-61 Nutzap components
#[allow(unused_imports)]
pub use nutzap_send_modal::NutzapSendModal;  // For future use when sending nutzaps from profile/post
pub use nutzap_settings_modal::NutzapSettingsModal;
pub use nutzap_inbox::{NutzapInbox, NutzapBadge};
