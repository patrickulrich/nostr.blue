use crate::hooks::UserList;
use crate::stores::auth_store;

#[derive(Clone, PartialEq, Debug)]
pub enum FeedType {
    Following,
    FollowingWithReplies,
    Global,
    PeopleList(Box<UserList>),
}

impl FeedType {
    pub fn label(&self) -> String {
        match self {
            FeedType::Following => "Following".to_string(),
            FeedType::FollowingWithReplies => "Following + Replies".to_string(),
            FeedType::Global => "Global".to_string(),
            FeedType::PeopleList(list) => list.name.clone(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Nip55State {
    Idle,
    Checking,
    WaitingForApproval,
    Error(String),
}

pub fn login_method_requires_signer(login_method: Option<&auth_store::LoginMethod>) -> bool {
    matches!(
        login_method,
        Some(auth_store::LoginMethod::BrowserExtension)
            | Some(auth_store::LoginMethod::PrivateKey)
            | Some(auth_store::LoginMethod::RemoteSigner)
    ) || {
        #[cfg(feature = "mobile")]
        {
            matches!(login_method, Some(auth_store::LoginMethod::AndroidSigner))
        }
        #[cfg(not(feature = "mobile"))]
        {
            false
        }
    }
}
