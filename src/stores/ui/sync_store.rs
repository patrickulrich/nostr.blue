use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncPhase {
    #[default]
    Idle,
    Waiting,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SyncTarget {
    FollowingFeed,
    RelayList,
    OwnIdentity,
    OwnContent,
    FollowedProfiles,
    Notifications,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncProgressState {
    pub current: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncTargetStats {
    pub last_started_at: Option<u64>,
    pub last_finished_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub local_count: usize,
    pub remote_count: usize,
    pub sent_count: usize,
    pub received_count: usize,
    pub send_failure_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncServiceState {
    pub phase: SyncPhase,
    pub active_target: Option<SyncTarget>,
    pub progress: Option<SyncProgressState>,
    pub waiting_reason: Option<String>,
    pub last_started_at: Option<u64>,
    pub last_finished_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub next_scheduled_at: Option<u64>,
    pub last_error: Option<String>,
    pub following_feed: SyncTargetStats,
    pub relay_list: SyncTargetStats,
    pub own_identity: SyncTargetStats,
    pub own_content: SyncTargetStats,
    pub followed_profiles: SyncTargetStats,
    pub notifications: SyncTargetStats,
}

impl SyncServiceState {
    pub fn target_stats_mut(&mut self, target: SyncTarget) -> &mut SyncTargetStats {
        match target {
            SyncTarget::FollowingFeed => &mut self.following_feed,
            SyncTarget::RelayList => &mut self.relay_list,
            SyncTarget::OwnIdentity => &mut self.own_identity,
            SyncTarget::OwnContent => &mut self.own_content,
            SyncTarget::FollowedProfiles => &mut self.followed_profiles,
            SyncTarget::Notifications => &mut self.notifications,
        }
    }
}

pub static SYNC_SERVICE_STATE: GlobalSignal<SyncServiceState> =
    Signal::global(SyncServiceState::default);
