//! Shared profile typeahead engine.
//!
//! One cascade used by every user-picker surface (@-mention autocomplete,
//! search inputs, mention dialogs, participant pickers):
//!
//! 1. **Instant local scan** — `search_cached_profiles` in a `use_memo` keyed
//!    on the query + `PROFILE_CACHE_VERSION` + contacts + participants. Bare
//!    queries (bare `@`) fall back to participants → MRU → follows preview.
//! 2. **Debounced special tracks** — cancellation-based 300ms debounce
//!    (`Task::cancel` per keystroke), then either:
//!    - NIP-05 `user@domain` short-circuit → single resolved user, or
//!    - full npub/nprofile/hex identifier → outbox metadata fetch, or
//!    - streaming NIP-50 relay search — hits are written into
//!      `PROFILE_CACHE` by `stream_profile_search`, which bumps
//!      `PROFILE_CACHE_VERSION` and re-runs the memo → the dropdown
//!      live-fills as results stream in.
//!
//! The hook never mutates the results for the relay track directly — the
//! cache write-back is the update path (network → cache → UI re-query).

use crate::services::profile_search::{
    get_contact_pubkeys, resolve_nip05_profile, search_cached_profiles, search_profiles,
    stream_profile_search, ProfileSearchResult,
};
use crate::stores::profiles::PROFILE_CACHE_VERSION;
use crate::stores::ui::mention_mru;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::*;

const DEBOUNCE_MS: u32 = 300;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeaheadOptions {
    /// Maximum number of results.
    pub limit: usize,
    /// Minimum query length (in chars) before the relay track may run.
    pub min_chars_relay: usize,
    /// Relay search only while the local scan returns fewer than this.
    pub relay_below: usize,
}

impl Default for TypeaheadOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            min_chars_relay: 2,
            relay_below: 5,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct UseProfileTypeahead {
    /// Instant local results (memoized; re-runs as the cache fills).
    pub local_results: Memo<Vec<ProfileSearchResult>>,
    /// Override results from the special tracks (NIP-05 / identifier).
    override_results: Signal<Option<Vec<ProfileSearchResult>>>,
    is_searching: Signal<bool>,
    search_task: Signal<Option<Task>>,
    options: TypeaheadOptions,
}

/// Strict `user@domain` detection — NIP-05 resolution must only run when the
/// user actually typed an `@` separator with a non-empty local part, since
/// `Nip05Address::parse` coerces bare domains to `_@domain`.
pub fn is_nip05_address(query: &str) -> bool {
    match query.split_once('@') {
        Some((name, domain)) => {
            !name.is_empty() && !domain.is_empty() && !domain.contains('@')
        }
        None => false,
    }
}

fn bare_mention_suggestions(
    participants: &[PublicKey],
    contacts: &[PublicKey],
    limit: usize,
) -> Vec<ProfileSearchResult> {
    fn push_result(
        pk: &PublicKey,
        boost: u32,
        seen: &mut std::collections::HashSet<PublicKey>,
        results: &mut Vec<ProfileSearchResult>,
    ) {
        if !seen.insert(*pk) {
            return;
        }
        let profile = crate::stores::profiles::get_cached_profile(&pk.to_hex());
        results.push(ProfileSearchResult {
            pubkey: *pk,
            name: profile.as_ref().and_then(|p| p.name.clone()),
            display_name: profile.as_ref().and_then(|p| p.display_name.clone()),
            picture: profile.as_ref().and_then(|p| p.picture.clone()),
            nip05: profile.as_ref().and_then(|p| p.nip05.clone()),
            is_contact: true,
            is_thread_participant: boost >= 2000,
            relevance: boost,
        });
    }
    let mut results: Vec<ProfileSearchResult> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Thread participants first, then recently-mentioned users, then a
    // follows preview.
    for pk in participants {
        push_result(pk, 2000, &mut seen, &mut results);
    }
    for mru in mention_mru::get_mru_results(limit) {
        if seen.insert(mru.pubkey) {
            let mut mru = mru;
            mru.relevance = 500;
            if participants.contains(&mru.pubkey) {
                mru.is_thread_participant = true;
                mru.relevance = 2000;
            }
            results.push(mru);
        }
    }
    for pk in contacts.iter().take(limit) {
        push_result(pk, 1000, &mut seen, &mut results);
    }
    results.sort_by_key(|r| std::cmp::Reverse(r.relevance));
    results.truncate(limit);
    results
}

pub fn use_profile_typeahead(
    query: Signal<String>,
    enabled: Signal<bool>,
    participants: Signal<Vec<PublicKey>>,
    options: TypeaheadOptions,
) -> UseProfileTypeahead {
    let contacts: Signal<Vec<PublicKey>> = use_signal(Vec::new);
    let override_results: Signal<Option<Vec<ProfileSearchResult>>> = use_signal(|| None);
    let is_searching = use_signal(|| false);
    let search_task: Signal<Option<Task>> = use_signal(|| None);

    // Fetch the contact list once per mount (shared input for local scan,
    // relay-result ranking and bare-@ follows preview).
    {
        let mut contacts = contacts;
        use_effect(move || {
            spawn(async move {
                let fetched = get_contact_pubkeys().await;
                contacts.set(fetched);
            });
        });
    }

    // Local instant scan. Re-runs on query/contacts/participants changes AND
    // whenever PROFILE_CACHE_VERSION bumps (i.e. when streamed relay hits are
    // written back) — that's the live-fill mechanism.
    let local_results = use_memo(move || {
        let _version = *PROFILE_CACHE_VERSION.read();
        let q = query.read().clone();
        let parts = participants.read().clone();
        if q.is_empty() {
            return bare_mention_suggestions(&parts, &contacts.read().clone(), options.limit);
        }
        search_cached_profiles(&q, options.limit, &contacts.read().clone(), &parts)
    });

    // Debounced network tracks.
    {
        let mut search_task = search_task;
        let mut is_searching = is_searching;
        let mut override_results = override_results;
        use_effect(move || {
            let q = query.read().clone();
            let enabled = *enabled.read();
            let _participants = participants.read().clone(); // re-run if participants change

            // Cancel any in-flight debounce/stream from a previous query.
            if let Some(task) = search_task.write().take() {
                task.cancel();
            }
            override_results.set(None);

            if !enabled || q.is_empty() {
                is_searching.set(false);
                return;
            }

            // NIP-05 short-circuit: strict user@domain only.
            if is_nip05_address(&q) {
                is_searching.set(true);
                let mut override_sig = override_results;
                let mut searching_sig = is_searching;
                spawn(async move {
                    let snapshot = q.clone();
                    match resolve_nip05_profile(&snapshot).await {
                        Ok(Some(resolution)) => {
                            // Learn the nostr.json relay hints so mention
                            // insertion can embed them in the nprofile.
                            crate::stores::ui::mention_mru::record_hints(
                                &resolution.result.pubkey.to_hex(),
                                &resolution.relays,
                            );
                            if query.read().as_str() == snapshot.as_str() {
                                override_sig.set(Some(vec![resolution.result]));
                            }
                            searching_sig.set(false);
                        }
                        _ => {
                            searching_sig.set(false);
                        }
                    }
                });
                return;
            }

            // Full identifiers (npub/nprofile/hex) — outbox metadata fetch,
            // never NIP-50 full-text search.
            if crate::utils::nip19_urls::parse_profile_id(&q).is_some() {
                is_searching.set(true);
                let mut override_sig = override_results;
                let mut searching_sig = is_searching;
                spawn(async move {
                    let snapshot = q.clone();
                    let result = search_profiles(&snapshot, 1, false).await;
                    if query.read().as_str() == snapshot.as_str() {
                        if let Ok(results) = result {
                            if !results.is_empty() {
                                override_sig.set(Some(results));
                            }
                        }
                        searching_sig.set(false);
                    }
                });
                return;
            }

            // Streaming NIP-50 relay search, only when local tiers can't fill.
            let local_count = local_results.peek().len();
            if q.chars().count() >= options.min_chars_relay && local_count < options.relay_below {
                is_searching.set(true);
                let contacts_snapshot = contacts.read().clone();
                let task = spawn(async move {
                    crate::platform::timer::sleep_ms(DEBOUNCE_MS).await;
                    let snapshot = q.clone();
                    // Stale guard: another keystroke superseded this query.
                    if query.read().as_str() != snapshot.as_str() {
                        return;
                    }
                    // Hits are written into PROFILE_CACHE by the stream (which
                    // bumps PROFILE_CACHE_VERSION and re-runs `local_results`)
                    // — no direct signal writes needed here.
                    let _ = stream_profile_search(&snapshot, options.limit, &contacts_snapshot, |_| {})
                        .await;
                    if query.read().as_str() == snapshot.as_str() {
                        is_searching.set(false);
                    }
                });
                search_task.set(Some(task));
            } else {
                is_searching.set(false);
            }
        });
    }

    UseProfileTypeahead {
        local_results,
        override_results,
        is_searching,
        search_task,
        options,
    }
}

impl UseProfileTypeahead {
    /// Effective results: the special-track override when present, otherwise
    /// the live local scan.
    pub fn results(&self) -> Vec<ProfileSearchResult> {
        if let Some(overridden) = self.override_results.read().clone() {
            return overridden;
        }
        self.local_results.read().clone()
    }

    pub fn is_searching(&self) -> bool {
        *self.is_searching.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nip05_address_detection_strict() {
        assert!(is_nip05_address("user@domain.com"));
        assert!(is_nip05_address("a@b"));
        assert!(!is_nip05_address("user@")); // empty domain
        assert!(!is_nip05_address("@domain.com")); // empty local part — the guard
        assert!(!is_nip05_address("domain.com")); // bare domain must NOT resolve
        assert!(!is_nip05_address("user@a@b")); // multiple @
        assert!(!is_nip05_address("plain"));
    }

    #[test]
    fn typeahead_options_defaults() {
        let opts = TypeaheadOptions::default();
        assert_eq!(opts.limit, 10);
        assert_eq!(opts.min_chars_relay, 2);
        assert_eq!(opts.relay_below, 5);
    }
}
