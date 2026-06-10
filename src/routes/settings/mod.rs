pub mod home;
pub mod p2p;
pub mod settings_ai;
pub mod settings_blocklist;
pub mod settings_muted;
pub mod settings_relays;

pub use home::Settings;
pub use p2p::SettingsP2P;
pub use settings_ai::SettingsAi;
pub use settings_blocklist::SettingsBlocklist;
pub use settings_muted::SettingsMuted;
pub use settings_relays::SettingsRelays;
