use super::worker::send_request;
use super::commands::NdbRequest;

pub enum SubKey {
    FollowingFeed,
    Notifications,
    Profile { pubkey: [u8; 32] },
    Thread { root_id: [u8; 32] },
    DmInbox,
}

impl SubKey {
    pub fn as_str(&self) -> String {
        match self {
            Self::FollowingFeed => "following-feed".into(),
            Self::Notifications => "notifications".into(),
            Self::Profile { pubkey } => format!("profile-{}", hex::encode(pubkey)),
            Self::Thread { root_id } => format!("thread-{}", hex::encode(root_id)),
            Self::DmInbox => "dm-inbox".into(),
        }
    }
}

pub async fn subscribe(key: SubKey, filter_jsons: Vec<String>) -> Result<(), String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    send_request(NdbRequest::Subscribe {
        key: key.as_str(),
        filter_jsons,
        reply: reply_tx,
    })?;
    reply_rx.await.map_err(|e| e.to_string())?
}

pub async fn unsubscribe(key: SubKey) -> Result<(), String> {
    send_request(NdbRequest::Unsubscribe {
        key: key.as_str(),
    })?;
    Ok(())
}
