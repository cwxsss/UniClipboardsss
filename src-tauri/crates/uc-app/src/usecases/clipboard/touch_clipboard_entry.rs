use anyhow::Result;
use std::sync::Arc;

use uc_core::ids::EntryId;
use uc_core::ports::{ClipboardEntryRepositoryPort, ClockPort};

/// Update clipboard entry active time.
///
/// 更新剪贴板条目的活跃时间。
pub struct TouchClipboardEntryUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    clock: Arc<dyn ClockPort>,
}

impl TouchClipboardEntryUseCase {
    pub fn new(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self { entry_repo, clock }
    }

    pub async fn execute(&self, entry_id: &EntryId) -> Result<bool> {
        let now_ms = self.clock.now_ms();

        self.entry_repo.touch_entry(entry_id, now_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::TouchClipboardEntryUseCase;
    use crate::test_mocks::{MockClipboardEntryRepository, MockClock};
    use std::sync::{Arc, Mutex};
    use uc_core::ids::EntryId;

    #[tokio::test]
    async fn execute_uses_clock_now_ms_for_touch() {
        let touched_at = Arc::new(Mutex::new(None::<i64>));
        let touched_at_clone = touched_at.clone();

        let mut entry_repo = MockClipboardEntryRepository::new();
        entry_repo
            .expect_touch_entry()
            .returning(move |_entry_id, active_time_ms| {
                *touched_at_clone.lock().unwrap() = Some(active_time_ms);
                Ok(true)
            });

        let mut clock = MockClock::new();
        clock.expect_now_ms().returning(|| 1234);

        let uc = TouchClipboardEntryUseCase::new(Arc::new(entry_repo), Arc::new(clock));
        let entry_id = EntryId::from("entry-1");

        let result = uc.execute(&entry_id).await.unwrap();

        assert!(result);
        assert_eq!(*touched_at.lock().unwrap(), Some(1234));
    }
}
