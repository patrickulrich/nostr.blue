use crate::hooks::UserList;
use crate::stores::auth_store;

#[derive(Clone, PartialEq, Debug)]
pub enum FeedType {
    Following,
    FollowingWithReplies,
    Global,
    PeopleList(Box<UserList>),
    RelayFeed {
        url: String,
        name: String,
    },
    RelaySetFeed {
        name: String,
        urls: Vec<String>,
    },
}

impl FeedType {
    pub fn label(&self) -> String {
        match self {
            FeedType::Following => "Following".to_string(),
            FeedType::FollowingWithReplies => "Following + Replies".to_string(),
            FeedType::Global => "Global".to_string(),
            FeedType::PeopleList(list) => list.name.clone(),
            FeedType::RelayFeed { name, .. } => name.clone(),
            FeedType::RelaySetFeed { name, .. } => name.clone(),
        }
    }

    pub fn is_relay_feed(&self) -> bool {
        matches!(self, FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. })
    }

    pub fn relay_urls(&self) -> Vec<String> {
        match self {
            FeedType::RelayFeed { url, .. } => vec![url.clone()],
            FeedType::RelaySetFeed { urls, .. } => urls.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Nip55State {
    Idle,
    Checking,
    Error(String),
}

pub fn login_method_requires_signer(login_method: Option<&auth_store::LoginMethod>) -> bool {
    matches!(
        login_method,
        Some(auth_store::LoginMethod::BrowserExtension)
            | Some(auth_store::LoginMethod::PrivateKey)
            | Some(auth_store::LoginMethod::RemoteSigner)
    ) || {
        #[cfg(feature = "mobile_platform")]
        {
            matches!(login_method, Some(auth_store::LoginMethod::AndroidSigner))
        }
        #[cfg(not(feature = "mobile_platform"))]
        {
            false
        }
    }
}
