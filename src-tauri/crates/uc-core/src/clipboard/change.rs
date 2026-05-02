use super::SystemClipboardSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardChangeOrigin {
    LocalCapture,
    LocalRestore,
    RemotePush,
}

#[derive(Debug, Clone)]
pub struct ClipboardChange {
    pub snapshot: SystemClipboardSnapshot,
    pub origin: ClipboardChangeOrigin,
}
