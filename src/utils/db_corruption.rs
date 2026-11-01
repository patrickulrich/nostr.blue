//! Classification of SQLite corruption-class errors from the WASM event and
//! gossip stores, plus the cross-boot recovery flag they coordinate through.
//!
//! On web, the event database (`nostr-sqlite`) and the gossip store
//! (`nostr-gossip-sqlite`) are WASM SQLite files over an OPFS SAHPool (or
//! relaxed IndexedDB fallback) VFS. Torn writes can corrupt those files and
//! SQLite then reports `SQLITE_CORRUPT`/`SQLITE_NOTADB` on ordinary queries
//! — including the SDK's pre-broadcast `save_event`, which aborts every
//! publish (reactions enqueue fine but never reach a relay).
//!
//! Both stores are pure relay-rehydratable caches, so the sanctioned
//! recovery is delete-and-rebuild, never repair. Errors surface through
//! several layers (rusqlite Display strings wrapped into `nostr-database` /
//! `nostr-gossip` / `nostr-sdk` errors), so classification is a pragmatic
//! substring match on the rendered message.

/// LocalStorage key set by the publish-queue corruption escalation path
/// ([`crate::stores::publish_queue::processor`]) and consumed at boot by
/// [`crate::stores::nostr_client`], which deletes both SQLite cache files
/// before anything opens them (VFS deletion requires a closed database).
///
/// WASM-only: the SQLite VFS stores exist solely on web builds (native uses
/// nostrdb/LMDB, which has its own recovery story).
#[cfg(target_arch = "wasm32")]
pub const DB_RECOVERY_FLAG_KEY: &str = "nostr.blue/db-recovery-pending";

/// Returns `true` when a rendered error message looks like SQLite file
/// corruption, as opposed to a relay rejection, a timeout, a lock, or a
/// network failure.
pub fn is_corruption_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    // SQLITE_CORRUPT — the torn-write signature ("database disk image is malformed")
    lower.contains("database disk image is malformed")
        // SQLITE_NOTADB — truncated/zeroed file or garbage header
        || lower.contains("file is not a database")
        // Generic wording used by some wrappers ("database corruption detected")
        || lower.contains("corrupt")
}

#[cfg(test)]
mod tests {
    use super::is_corruption_error;

    #[test]
    fn classifies_sqlite_corruption() {
        assert!(is_corruption_error(
            "Backend: Error code 11: database disk image is malformed"
        ));
        assert!(is_corruption_error("file is not a database"));
        assert!(is_corruption_error(
            "Database corruption detected in index events_idx"
        ));
        // Wrapped variants as they surface through nostr-database / nostr-gossip.
        assert!(is_corruption_error(
            "Backend(Error code 11: database disk image is malformed)"
        ));
        assert!(is_corruption_error(
            "Store(Rusqlite(Database is corrupt (code 11))"
        ));
    }

    #[test]
    fn rejects_non_corruption_errors() {
        // Relay-side rejections must keep their normal retry semantics.
        assert!(!is_corruption_error("relay: unable to send event: blocked"));
        assert!(!is_corruption_error("Send timed out after 20s"));
        // Locks/IO hiccups are transient, not corruption — never reset for them.
        assert!(!is_corruption_error("temporary failure: database is locked"));
        assert!(!is_corruption_error("disk I/O error"));
        assert!(!is_corruption_error("NIP-07 request dismissed"));
        assert!(!is_corruption_error(""));
    }
}
