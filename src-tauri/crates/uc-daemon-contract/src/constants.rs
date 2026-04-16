//! Daemon wire-protocol string constants shared between uc-daemon (server) and uc-daemon-client (consumer).

/// WebSocket topic names used to subscribe to event streams.
pub mod ws_topic {
    pub const STATUS: &str = "status";
    pub const PEERS: &str = "peers";
    pub const PAIRED_DEVICES: &str = "paired-devices";
    pub const PAIRING: &str = "pairing";
    pub const PAIRING_SESSION: &str = "pairing/session";
    pub const PAIRING_VERIFICATION: &str = "pairing/verification";
    pub const SETUP: &str = "setup";
    pub const SPACE_ACCESS: &str = "space-access";
    pub const CLIPBOARD: &str = "clipboard";
    pub const FILE_TRANSFER: &str = "file-transfer";
    pub const ENCRYPTION: &str = "encryption";
    /// Search index events topic (Phase 92).
    pub const SEARCH: &str = "search";
}

/// WebSocket event type names emitted within topics.
pub mod ws_event {
    pub const STATUS_SNAPSHOT: &str = "status.snapshot";
    pub const STATUS_UPDATED: &str = "status.updated";
    pub const PEERS_SNAPSHOT: &str = "peers.snapshot";
    pub const PEERS_CHANGED: &str = "peers.changed";
    pub const PEERS_NAME_UPDATED: &str = "peers.nameUpdated";
    pub const PEERS_CONNECTION_CHANGED: &str = "peers.connectionChanged";
    pub const PAIRED_DEVICES_SNAPSHOT: &str = "paired-devices.snapshot";
    pub const PAIRED_DEVICES_CHANGED: &str = "paired-devices.changed";
    pub const PAIRING_SNAPSHOT: &str = "pairing.snapshot";
    pub const PAIRING_UPDATED: &str = "pairing.updated";
    pub const PAIRING_VERIFICATION_REQUIRED: &str = "pairing.verification_required";
    pub const PAIRING_COMPLETE: &str = "pairing.complete";
    pub const PAIRING_FAILED: &str = "pairing.failed";
    pub const SETUP_STATE_CHANGED: &str = "setup.stateChanged";
    pub const SETUP_SPACE_ACCESS_COMPLETED: &str = "setup.spaceAccessCompleted";
    pub const SPACE_ACCESS_SNAPSHOT: &str = "space_access.snapshot";
    pub const SPACE_ACCESS_STATE_CHANGED: &str = "space_access.state_changed";
    pub const CLIPBOARD_NEW_CONTENT: &str = "clipboard.new_content";
    pub const FILE_TRANSFER_STATUS_CHANGED: &str = "file-transfer.status_changed";
    pub const FILE_TRANSFER_PROGRESS: &str = "file-transfer.progress";
    pub const ENCRYPTION_SESSION_READY: &str = "encryption.session_ready";
    /// Search availability snapshot event (Phase 92).
    pub const SEARCH_STATUS_SNAPSHOT: &str = "search.status_snapshot";
    /// Search rebuild progress event (Phase 92).
    pub const SEARCH_REBUILD_PROGRESS: &str = "search.rebuild_progress";
}

/// Pairing stage labels used in pairing session state payloads.
pub mod pairing_stage {
    pub const REQUEST: &str = "request";
    pub const VERIFICATION: &str = "verification";
    pub const VERIFYING: &str = "verifying";
    pub const COMPLETE: &str = "complete";
    pub const FAILED: &str = "failed";
}

/// Reasons emitted when a pairing request is rejected because the host is busy.
pub mod pairing_busy_reason {
    pub const HOST_NOT_DISCOVERABLE: &str = "host_not_discoverable";
    pub const NO_LOCAL_PAIRING_PARTICIPANT_READY: &str = "no_local_pairing_participant_ready";
    pub const BUSY: &str = "busy";
}

/// HTTP/JSON error codes returned by the daemon pairing API endpoints.
pub mod pairing_error_code {
    pub const ACTIVE_SESSION_EXISTS: &str = "active_session_exists";
    pub const HOST_NOT_DISCOVERABLE: &str = "host_not_discoverable";
    pub const NO_LOCAL_PARTICIPANT: &str = "no_local_participant";
    pub const SESSION_NOT_FOUND: &str = "session_not_found";
    pub const INTERNAL: &str = "internal";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const RUNTIME_UNAVAILABLE: &str = "runtime_unavailable";
}

/// HTTP route path prefixes for daemon REST endpoints.
pub mod http_route {
    /// POST /clipboard/restore/:entry_id — restore clipboard entry to OS clipboard
    pub const CLIPBOARD_RESTORE: &str = "/clipboard/restore";
    /// GET /clipboard/entries — list clipboard entries with pagination
    pub const CLIPBOARD_ENTRIES: &str = "/clipboard/entries";
    /// GET /clipboard/stats — clipboard statistics
    pub const CLIPBOARD_STATS: &str = "/clipboard/stats";
    /// GET /settings — daemon settings
    pub const SETTINGS: &str = "/settings";
    /// GET /encryption/state — encryption state
    pub const ENCRYPTION_STATE: &str = "/encryption/state";
    /// POST /encryption/unlock — unlock encryption with passphrase
    pub const ENCRYPTION_UNLOCK: &str = "/encryption/unlock";
    /// POST /encryption/lock — lock encryption
    pub const ENCRYPTION_LOCK: &str = "/encryption/lock";
    /// GET /storage/stats — storage statistics
    pub const STORAGE_STATS: &str = "/storage/stats";
    /// POST /storage/clear-cache — clear storage cache
    pub const STORAGE_CLEAR_CACHE: &str = "/storage/clear-cache";
    /// GET /clipboard/blobs/:blob_id — serve raw blob binary content
    pub const CLIPBOARD_BLOBS: &str = "/clipboard/blobs";
    /// GET /clipboard/thumbnails/:rep_id — serve raw thumbnail binary content
    pub const CLIPBOARD_THUMBNAILS: &str = "/clipboard/thumbnails";
    /// GET /search/query — execute a structured search query (Phase 92)
    pub const SEARCH_QUERY: &str = "/search/query";
    /// GET /search/status — get search index availability status (Phase 92)
    pub const SEARCH_STATUS: &str = "/search/status";
    /// POST /search/rebuild — trigger manual search index rebuild (Phase 92)
    pub const SEARCH_REBUILD: &str = "/search/rebuild";
}

/// HTTP route paths for daemon auth endpoints.
pub mod auth_route {
    /// POST /auth/connect — exchange bearer token for JWT session token
    pub const AUTH_CONNECT: &str = "/auth/connect";
}
