//! Outbound transfer_id → entry_id hint cache.
//!
//! Sender-side transfers have no receiver projection row, so
//! `FileTransferRepositoryPort::get_entry_id_for_transfer` returns `None`.
//! The clipboard_watcher populates this cache when it initiates an outbound
//! transfer; `FileTransferHostEventPublisher` falls back to it whenever the
//! projection lookup misses, so sender-side host events still carry a
//! usable `entry_id`.
//!
//! This is intentionally a lightweight in-memory hint, not a projection —
//! entries naturally disappear when `remove` is called at transfer end, and
//! there is no durability requirement.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct OutboundEntryIdCache {
    inner: Mutex<HashMap<String, String>>,
}

impl OutboundEntryIdCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, transfer_id: impl Into<String>, entry_id: impl Into<String>) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(transfer_id.into(), entry_id.into());
    }

    pub fn get(&self, transfer_id: &str) -> Option<String> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.get(transfer_id).cloned()
    }

    pub fn remove(&self, transfer_id: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.remove(transfer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get_returns_entry_id() {
        let cache = OutboundEntryIdCache::new();
        cache.insert("tx-1", "entry-1");
        assert_eq!(cache.get("tx-1"), Some("entry-1".to_string()));
    }

    #[test]
    fn get_missing_returns_none() {
        let cache = OutboundEntryIdCache::new();
        assert!(cache.get("tx-missing").is_none());
    }

    #[test]
    fn remove_drops_entry() {
        let cache = OutboundEntryIdCache::new();
        cache.insert("tx-1", "entry-1");
        cache.remove("tx-1");
        assert!(cache.get("tx-1").is_none());
    }
}
