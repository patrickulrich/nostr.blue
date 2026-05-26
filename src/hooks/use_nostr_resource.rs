use crate::stores::nostr_client;
use dioxus::prelude::*;
use std::future::Future;

#[derive(Clone, Debug, PartialEq)]
pub enum NostrResourceState<T> {
    Initializing,
    AuthRequired,
    Loading,
    Loaded(T),
    Error(String),
}

pub struct UseNostrResource<T: 'static> {
    state: Memo<NostrResourceState<T>>,
    resource: Resource<NostrResourceState<T>>,
}

impl<T: 'static + Clone + PartialEq> UseNostrResource<T> {
    pub fn state(&self) -> Memo<NostrResourceState<T>> {
        self.state
    }

    pub fn restart(&mut self) {
        self.resource.restart();
    }

    pub fn is_loading(&self) -> bool {
        matches!(&*self.state.read(), NostrResourceState::Loading)
    }
}

#[allow(dead_code)]
pub fn use_nostr_resource<T, F>(
    mut fetcher: impl FnMut() -> F + 'static,
) -> UseNostrResource<T>
where
    T: 'static + Clone + PartialEq,
    F: Future<Output = Result<T, String>> + 'static,
{
    let resource = use_resource(move || {
        let ci = *nostr_client::CLIENT_INITIALIZED.read();
        let hs = *nostr_client::HAS_SIGNER.read();
        let fut = fetcher();
        async move {
            if !ci {
                return NostrResourceState::<T>::Initializing;
            }
            if !hs {
                return NostrResourceState::<T>::AuthRequired;
            }
            match fut.await {
                Ok(d) => NostrResourceState::Loaded(d),
                Err(e) => NostrResourceState::Error(e),
            }
        }
    });

    let state = use_memo(move || match &*resource.read() {
        None => NostrResourceState::Loading,
        Some(s) => s.clone(),
    });

    UseNostrResource { state, resource }
}

pub fn use_nostr_resource_public<T, F>(
    mut fetcher: impl FnMut() -> F + 'static,
) -> UseNostrResource<T>
where
    T: 'static + Clone + PartialEq,
    F: Future<Output = Result<T, String>> + 'static,
{
    let resource = use_resource(move || {
        let ci = *nostr_client::CLIENT_INITIALIZED.read();
        let fut = fetcher();
        async move {
            if !ci {
                return NostrResourceState::<T>::Initializing;
            }
            match fut.await {
                Ok(d) => NostrResourceState::Loaded(d),
                Err(e) => NostrResourceState::Error(e),
            }
        }
    });

    let state = use_memo(move || match &*resource.read() {
        None => NostrResourceState::Loading,
        Some(s) => s.clone(),
    });

    UseNostrResource { state, resource }
}
