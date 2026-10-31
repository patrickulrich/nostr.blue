//! Dual-read union of app-level pending tracking and cdk's native ledger.
//!
//! nostr.blue tracks pending wallet operations in app-level signals
//! (`PENDING_MINT_QUOTES`, `PENDING_MELT_QUOTES`, `IN_FLIGHT_*`) and an
//! `ActiveTransaction` ledger, while cdk 0.18 persists its own durable
//! `Transaction` ledger (written by every saga-driven operation). A crash
//! between the wallet call and the signal write leaves the app-level view
//! blind while the native ledger still knows — and vice versa on freshly
//! restored wallets where sagas replay without our signals ever being set.
//!
//! Stage S2 of native-saga adoption: enumerate the UNION of both views,
//! deduped by (mint, quote id), and act on the existing recovery primitives.
//! Actions are unchanged — this only widens discovery. The merge itself is a
//! pure function so the dedup semantics are unit-tested; the async collector
//! feeds it real state.

use cdk_common::wallet::TransactionStatus;

/// What kind of pending operation a source row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOpKind {
    /// Mint quote awaiting payment + issuance (lightning receive / topup).
    Mint,
    /// Melt quote awaiting payment / change recovery (lightning send).
    Melt,
    /// Ecash send tracked at app level (no quote id).
    Send,
    /// Native transaction without a quote id (saga-driven swap/receive/…);
    /// `recover_cdk_sagas` owns those, the union only surfaces them.
    Other,
}

/// Which tracking surfaces know about a pending operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PendingSources(u8);

impl PendingSources {
    pub const SIGNALS: Self = Self(1 << 0);
    pub const ACTIVE_TX: Self = Self(1 << 1);
    pub const NATIVE: Self = Self(1 << 2);

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    #[cfg(test)]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A pending operation merged from every tracking surface that knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperation {
    pub mint_url: String,
    pub quote_id: Option<String>,
    pub kind: PendingOpKind,
    pub sources: PendingSources,
}

/// Quote-bearing ref from the app-level signal lists
/// (`PENDING_MINT_QUOTES` / `PENDING_MELT_QUOTES` / in-flight requests).
#[derive(Debug, Clone)]
pub struct SignalQuoteRef {
    pub mint_url: String,
    pub quote_id: String,
}

/// Ref from the app-level `ActiveTransaction` ledger.
#[derive(Debug, Clone)]
pub struct ActiveTxRef {
    pub mint_url: String,
    pub quote_id: Option<String>,
    /// App-level type; quote-bearing Topup/Transfer map to Mint/Melt.
    pub kind: PendingOpKind,
}

/// Ref from cdk's native `Transaction` ledger (pending rows only).
#[derive(Debug, Clone)]
pub struct NativeTxRef {
    pub mint_url: String,
    pub quote_id: Option<String>,
    /// Native rows have no kind field: direction Incoming + quote = mint,
    /// Outgoing + quote = melt, anything else is saga-owned (Other).
    pub direction_incoming: bool,
}

/// Merge every tracking surface into one deduplicated pending list.
///
/// Dedup key: `(mint_url, quote_id)` for quote-bearing rows; quote-less rows
/// (app-level `Send`, native saga transactions) never merge with anything —
/// they have no stable shared identifier, and collapsing them by
/// `(mint, None)` would silently drop distinct operations.
pub fn merge_pending_operations(
    mint_quote_signals: &[SignalQuoteRef],
    melt_quote_signals: &[SignalQuoteRef],
    active_txs: &[ActiveTxRef],
    native_txs: &[NativeTxRef],
) -> Vec<PendingOperation> {
    let mut merged: Vec<PendingOperation> = Vec::new();

    // Quote-bearing entries merge by (mint, quote). Quote-less entries are
    // appended as-is (keyed only by their own identity — see doc comment).
    fn upsert_quote(
        merged: &mut Vec<PendingOperation>,
        mint_url: &str,
        quote_id: &str,
        kind: PendingOpKind,
        source: PendingSources,
    ) {
        if let Some(op) = merged
            .iter_mut()
            .find(|op| op.mint_url == mint_url && op.quote_id.as_deref() == Some(quote_id))
        {
            op.sources = op.sources.union(source);
        } else {
            merged.push(PendingOperation {
                mint_url: mint_url.to_string(),
                quote_id: Some(quote_id.to_string()),
                kind,
                sources: source,
            });
        }
    }

    for r in mint_quote_signals {
        upsert_quote(&mut merged, &r.mint_url, &r.quote_id, PendingOpKind::Mint, PendingSources::SIGNALS);
    }
    for r in melt_quote_signals {
        upsert_quote(&mut merged, &r.mint_url, &r.quote_id, PendingOpKind::Melt, PendingSources::SIGNALS);
    }
    for tx in active_txs {
        match &tx.quote_id {
            Some(quote_id) => upsert_quote(
                &mut merged,
                &tx.mint_url,
                quote_id,
                tx.kind,
                PendingSources::ACTIVE_TX,
            ),
            None => merged.push(PendingOperation {
                mint_url: tx.mint_url.clone(),
                quote_id: None,
                kind: tx.kind,
                sources: PendingSources::ACTIVE_TX,
            }),
        }
    }
    for tx in native_txs {
        let kind = match (&tx.quote_id, tx.direction_incoming) {
            (Some(_), true) => PendingOpKind::Mint,
            (Some(_), false) => PendingOpKind::Melt,
            (None, _) => PendingOpKind::Other,
        };
        match &tx.quote_id {
            Some(quote_id) => upsert_quote(
                &mut merged,
                &tx.mint_url,
                quote_id,
                kind,
                PendingSources::NATIVE,
            ),
            None => merged.push(PendingOperation {
                mint_url: tx.mint_url.clone(),
                quote_id: None,
                kind,
                sources: PendingSources::NATIVE,
            }),
        }
    }

    merged
}

/// Filter a merged list down to quote-bearing mint/melt operations that ONLY
/// the native ledger knows about — the app-level recovery loops (driven by
/// `ActiveTransaction`) would skip these without the dual-read.
pub fn native_only_quote_ops(merged: &[PendingOperation]) -> Vec<&PendingOperation> {
    merged
        .iter()
        .filter(|op| {
            op.quote_id.is_some()
                && matches!(op.kind, PendingOpKind::Mint | PendingOpKind::Melt)
                && op.sources == PendingSources::NATIVE
        })
        .collect()
}

/// Collect the real union across signals, the active-transaction ledger and
/// every wallet's native transaction ledger.
pub async fn collect_pending_operations() -> Vec<PendingOperation> {
    use super::signals::{IN_FLIGHT_MELT_REQUESTS, PENDING_MELT_QUOTES, PENDING_MINT_QUOTES};
    use super::types::{MeltQuoteInfo, MintQuoteInfo};
    use crate::stores::cashu::proofs::get_pending_transactions;
    use dioxus::prelude::ReadableExt;
    use super::types::{PendingMeltQuotesStoreStoreExt, PendingMintQuotesStoreStoreExt};

    let mint_signals: Vec<SignalQuoteRef> = {
        let store = PENDING_MINT_QUOTES.read();
        let data = store.data();
        let quotes: Vec<MintQuoteInfo> = data.read().clone();
        quotes
            .into_iter()
            .map(|q| SignalQuoteRef {
                mint_url: q.mint_url,
                quote_id: q.quote_id,
            })
            .collect()
    };
    let melt_signals: Vec<SignalQuoteRef> = {
        let store = PENDING_MELT_QUOTES.read();
        let data = store.data();
        let quotes: Vec<MeltQuoteInfo> = data.read().clone();
        quotes
            .into_iter()
            .map(|q| SignalQuoteRef {
                mint_url: q.mint_url,
                quote_id: q.quote_id,
            })
            .collect()
    };

    let active_txs: Vec<ActiveTxRef> = get_pending_transactions()
        .into_iter()
        .map(|tx| ActiveTxRef {
            mint_url: tx.mint_url,
            quote_id: tx.quote_id,
            kind: match tx.tx_type {
                super::types::TransactionType::Topup => PendingOpKind::Mint,
                super::types::TransactionType::Transfer => PendingOpKind::Melt,
                super::types::TransactionType::Send => PendingOpKind::Send,
                _ => PendingOpKind::Other,
            },
        })
        .collect();

    let mut native_txs: Vec<NativeTxRef> = Vec::new();
    if let Some(repo) = crate::stores::cashu_cdk_bridge::MULTI_WALLET.read().as_ref() {
        for wallet in repo.get_wallets().await {
            // Pending native rows only — Completed/Failed rows are history.
            match wallet.list_transactions(None).await {
                Ok(txs) => {
                    for tx in txs {
                        if tx.status != TransactionStatus::Pending {
                            continue;
                        }
                        native_txs.push(NativeTxRef {
                            mint_url: wallet.mint_url.to_string(),
                            quote_id: tx.quote_id,
                            direction_incoming: tx.direction
                                == cdk_common::wallet::TransactionDirection::Incoming,
                        });
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to list native transactions for {}: {}",
                        wallet.mint_url,
                        e
                    );
                }
            }
        }
    }

    // In-flight melts carry quote ids too — fold them into the signal side
    // so native rows still being actively driven by our orchestration aren't
    // treated as orphaned. (In-flight sends/swap requests are quote-less and
    // surface through the ActiveTransaction arm above.)
    let mut all_melt_signals = melt_signals;
    for r in IN_FLIGHT_MELT_REQUESTS.read().iter() {
        all_melt_signals.push(SignalQuoteRef {
            mint_url: r.mint_url.clone(),
            quote_id: r.quote_id.clone(),
        });
    }

    merge_pending_operations(&mint_signals, &all_melt_signals, &active_txs, &native_txs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(mint: &str, quote: &str) -> SignalQuoteRef {
        SignalQuoteRef {
            mint_url: mint.to_string(),
            quote_id: quote.to_string(),
        }
    }

    fn active(mint: &str, quote: Option<&str>, kind: PendingOpKind) -> ActiveTxRef {
        ActiveTxRef {
            mint_url: mint.to_string(),
            quote_id: quote.map(|q| q.to_string()),
            kind,
        }
    }

    fn native(mint: &str, quote: Option<&str>, incoming: bool) -> NativeTxRef {
        NativeTxRef {
            mint_url: mint.to_string(),
            quote_id: quote.map(|q| q.to_string()),
            direction_incoming: incoming,
        }
    }

    #[test]
    fn same_quote_across_all_three_surfaces_merges_into_one() {
        let merged = merge_pending_operations(
            &[sig("https://mint.a", "q1")],
            &[],
            &[active("https://mint.a", Some("q1"), PendingOpKind::Mint)],
            &[native("https://mint.a", Some("q1"), true)],
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].kind, PendingOpKind::Mint);
        assert!(merged[0].sources.contains(PendingSources::SIGNALS));
        assert!(merged[0].sources.contains(PendingSources::ACTIVE_TX));
        assert!(merged[0].sources.contains(PendingSources::NATIVE));
        assert!(native_only_quote_ops(&merged).is_empty());
    }

    #[test]
    fn same_quote_id_on_different_mints_stays_separate() {
        let merged = merge_pending_operations(
            &[sig("https://mint.a", "shared")],
            &[],
            &[active("https://mint.b", Some("shared"), PendingOpKind::Mint)],
            &[],
        );
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn native_only_rows_survive_for_dual_read() {
        let merged = merge_pending_operations(
            &[],
            &[],
            &[],
            &[
                native("https://mint.a", Some("m1"), true),
                native("https://mint.a", Some("m2"), false),
            ],
        );
        assert_eq!(merged.len(), 2);
        let native_only = native_only_quote_ops(&merged);
        assert_eq!(native_only.len(), 2);
        assert!(matches!(native_only[0].kind, PendingOpKind::Mint));
        assert!(matches!(native_only[1].kind, PendingOpKind::Melt));
    }

    #[test]
    fn native_row_covered_by_signal_is_not_native_only() {
        let merged = merge_pending_operations(
            &[sig("https://mint.a", "q1")],
            &[],
            &[],
            &[native("https://mint.a", Some("q1"), true)],
        );
        assert!(native_only_quote_ops(&merged).is_empty());
    }

    #[test]
    fn kind_conflict_resolved_by_first_writer() {
        // A mint signal and a native melt row share a quote id — the union
        // keeps one entry with the first kind seen (signals win; native rows
        // are the widest net, the app's typed view the authority).
        let merged = merge_pending_operations(
            &[sig("https://mint.a", "q9")],
            &[],
            &[],
            &[native("https://mint.a", Some("q9"), false)],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0].kind, PendingOpKind::Mint));
        // But the union is still marked as native-covered.
        assert!(merged[0].sources.contains(PendingSources::NATIVE));
    }

    #[test]
    fn quote_less_rows_never_collapse() {
        let merged = merge_pending_operations(
            &[],
            &[],
            &[
                active("https://mint.a", None, PendingOpKind::Send),
                active("https://mint.a", None, PendingOpKind::Send),
            ],
            &[native("https://mint.a", None, false)],
        );
        // Two distinct app sends + one saga-owned native row: 3 entries.
        assert_eq!(merged.len(), 3);
        assert!(merged.iter().all(|op| op.quote_id.is_none()));
    }

    #[test]
    fn signals_only_entries_keep_signal_source() {
        let merged = merge_pending_operations(
            &[],
            &[sig("https://mint.a", "melt-q")],
            &[],
            &[],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0].kind, PendingOpKind::Melt));
        assert_eq!(merged[0].sources, PendingSources::SIGNALS);
        assert!(native_only_quote_ops(&merged).is_empty());
    }
}
