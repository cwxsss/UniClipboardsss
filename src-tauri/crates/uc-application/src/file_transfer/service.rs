use std::sync::Arc;

use thiserror::Error;
use uc_core::file_transfer::{FileTransferEventPublisherPort, FileTransferEventStorePort};
use uc_core::{
    FileTransferCancellationReason, FileTransferEvent, FileTransferFailureReason,
    FileTransferProgress,
};

/// Application service for file-transfer lifecycle orchestration.
///
/// 文件传输应用服务。
///
/// # Responsibility / 职责
/// - Rebuild transfer state from stored domain events.
/// - Validate whether the requested application action is still legal.
/// - Persist the newly produced domain event.
/// - Publish the same event to outer observers.
///
/// - 从已存储的领域事件中恢复传输状态。
/// - 校验当前应用动作是否仍然合法。
/// - 持久化这次新产生的领域事件。
/// - 将同一个事件发布给外层观察者。
///
/// # Boundary / 边界
/// This type does not decide transport details, UI behavior, or persistence
/// implementation. It only coordinates the application flow around
/// `FileTransferEvent`.
///
/// 这个类型不决定传输协议细节、界面行为，也不关心存储实现。
/// 它只负责围绕 `FileTransferEvent` 编排应用层流程。
pub struct FileTransferApplicationService<S, P> {
    store: Arc<S>,
    publisher: Arc<P>,
}

impl<S, P> FileTransferApplicationService<S, P>
where
    S: FileTransferEventStorePort,
    P: FileTransferEventPublisherPort,
{
    /// Create a new file-transfer application service.
    ///
    /// 创建文件传输应用服务。
    ///
    /// # Parameters / 参数
    /// - `store`: durable event storage used to rebuild transfer history
    /// - `publisher`: outbound publisher used to notify outer layers
    ///
    /// - `store`：用于恢复传输历史的事件存储
    /// - `publisher`：用于通知外层的事件发布器
    pub fn new(store: Arc<S>, publisher: Arc<P>) -> Self {
        Self { store, publisher }
    }

    /// Start a new transfer and emit a `Started` event.
    ///
    /// 启动一个新的传输，并产出 `Started` 事件。
    ///
    /// # Behavior / 行为
    /// - Loads existing history for `transfer_id`
    /// - Rejects the request if the transfer was already started
    /// - Persists and publishes the new start event on success
    ///
    /// - 先读取 `transfer_id` 对应的历史
    /// - 如果传输已经开始过，则拒绝这次请求
    /// - 成功时持久化并发布新的开始事件
    pub async fn start_transfer(
        &self,
        input: StartTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let history = self
            .store
            .load(&input.transfer_id)
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        let timeline = TransferTimeline::from_history(&input.transfer_id, &history)?;

        if timeline.started {
            return Err(FileTransferApplicationError::TransferAlreadyStarted {
                transfer_id: input.transfer_id,
            });
        }

        let event = FileTransferEvent::started(
            input.transfer_id,
            input.peer_id,
            input.filename,
            input.file_size,
        );

        self.persist_and_publish(event).await
    }

    /// Record transfer progress and emit a `Progress` event.
    ///
    /// 记录传输进度，并产出 `Progress` 事件。
    ///
    /// # Validation / 校验
    /// - the transfer must have been started
    /// - the transfer must not be finished already
    /// - the peer must match the original transfer owner
    /// - reported bytes must not move backwards
    ///
    /// - 传输必须已经开始
    /// - 传输不能已经结束
    /// - 上报的对端必须和原始传输一致
    /// - 已传输字节数不能倒退
    pub async fn report_progress(
        &self,
        input: ReportTransferProgress,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let history = self
            .store
            .load(&input.transfer_id)
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        let timeline = TransferTimeline::from_history(&input.transfer_id, &history)?;
        timeline.ensure_active(&input.transfer_id, &input.peer_id)?;

        if let Some(previous_bytes) = timeline.last_progress_bytes {
            if input.progress.bytes_transferred < previous_bytes {
                return Err(FileTransferApplicationError::ProgressWentBackwards {
                    transfer_id: input.transfer_id,
                    previous_bytes,
                    new_bytes: input.progress.bytes_transferred,
                });
            }
        }

        let event = FileTransferEvent::Progress {
            transfer_id: input.transfer_id,
            peer_id: input.peer_id,
            progress: input.progress,
        };

        self.persist_and_publish(event).await
    }

    /// Mark a transfer as completed and emit a `Completed` event.
    ///
    /// 将传输标记为完成，并产出 `Completed` 事件。
    pub async fn complete_transfer(
        &self,
        input: CompleteTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let history = self
            .store
            .load(&input.transfer_id)
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        let timeline = TransferTimeline::from_history(&input.transfer_id, &history)?;
        timeline.ensure_active(&input.transfer_id, &input.peer_id)?;

        let event = FileTransferEvent::completed(input.transfer_id, input.peer_id);
        self.persist_and_publish(event).await
    }

    /// Mark a transfer as failed and emit a `Failed` event.
    ///
    /// 将传输标记为失败，并产出 `Failed` 事件。
    pub async fn fail_transfer(
        &self,
        input: FailTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let history = self
            .store
            .load(&input.transfer_id)
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        let timeline = TransferTimeline::from_history(&input.transfer_id, &history)?;
        timeline.ensure_active(&input.transfer_id, &input.peer_id)?;

        let event = FileTransferEvent::failed(input.transfer_id, input.peer_id, input.reason);
        self.persist_and_publish(event).await
    }

    /// Cancel an active transfer and emit a `Cancelled` event.
    ///
    /// 取消一个仍在进行中的传输，并产出 `Cancelled` 事件。
    pub async fn cancel_transfer(
        &self,
        input: CancelTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let history = self
            .store
            .load(&input.transfer_id)
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        let timeline = TransferTimeline::from_history(&input.transfer_id, &history)?;
        timeline.ensure_active(&input.transfer_id, &input.peer_id)?;

        let event = FileTransferEvent::cancelled(input.transfer_id, input.peer_id, input.reason);
        self.persist_and_publish(event).await
    }

    async fn persist_and_publish(
        &self,
        event: FileTransferEvent,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        // 应用层保证“先落存储，再向外发布”，避免外层看到一个还没被持久化的事实。
        self.store
            .append(event.clone())
            .await
            .map_err(|error| FileTransferApplicationError::Store(error.to_string()))?;
        self.publisher
            .publish(event.clone())
            .await
            .map_err(|error| FileTransferApplicationError::Publish(error.to_string()))?;
        Ok(event)
    }
}

/// Input for starting a transfer.
///
/// 启动传输时的应用层输入。
///
/// `transfer_id` 和 `peer_id` 用来确定这条传输事实属于谁；
/// `filename` 和 `file_size` 则描述开始事件里需要暴露的业务信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTransfer {
    pub transfer_id: String,
    pub peer_id: String,
    pub filename: String,
    pub file_size: u64,
}

/// Input for reporting transfer progress.
///
/// 上报传输进度时的应用层输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportTransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub progress: FileTransferProgress,
}

/// Input for completing a transfer.
///
/// 标记传输完成时的应用层输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteTransfer {
    pub transfer_id: String,
    pub peer_id: String,
}

/// Input for failing a transfer.
///
/// 标记传输失败时的应用层输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailTransfer {
    pub transfer_id: String,
    pub peer_id: String,
    pub reason: FileTransferFailureReason,
}

/// Input for cancelling a transfer.
///
/// 取消传输时的应用层输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelTransfer {
    pub transfer_id: String,
    pub peer_id: String,
    pub reason: FileTransferCancellationReason,
}

/// Application-layer errors for file-transfer orchestration.
///
/// 文件传输应用层错误。
///
/// 这些错误只表达“这次应用动作为什么不能继续”，
/// 不承担底层存储实现或传输实现的细节语义。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FileTransferApplicationError {
    /// 持久化历史事件失败。
    #[error("file transfer event store failed: {0}")]
    Store(String),
    /// 向外发布事件失败。
    #[error("file transfer event publishing failed: {0}")]
    Publish(String),
    /// 传输尚未开始，不能继续推进。
    #[error("transfer `{transfer_id}` has not been started")]
    TransferNotStarted { transfer_id: String },
    /// 传输已经开始过，不能重复开始。
    #[error("transfer `{transfer_id}` has already been started")]
    TransferAlreadyStarted { transfer_id: String },
    /// 传输已经进入结束态，不能继续推进。
    #[error("transfer `{transfer_id}` has already finished")]
    TransferAlreadyFinished { transfer_id: String },
    /// 当前操作的对端与历史记录里的对端不一致。
    #[error(
        "transfer `{transfer_id}` belongs to peer `{expected_peer_id}`, not `{actual_peer_id}`"
    )]
    PeerMismatch {
        transfer_id: String,
        expected_peer_id: String,
        actual_peer_id: String,
    },
    /// 已存储的事件历史本身不合法，无法恢复出可信状态。
    #[error("transfer `{transfer_id}` history is invalid: {message}")]
    InvalidHistory {
        transfer_id: String,
        message: String,
    },
    /// 进度出现倒退，说明输入和当前状态不一致。
    #[error(
        "transfer `{transfer_id}` progress moved backwards from {previous_bytes} to {new_bytes}"
    )]
    ProgressWentBackwards {
        transfer_id: String,
        previous_bytes: u64,
        new_bytes: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalState {
    Completed,
    Failed,
    Cancelled,
}

/// Reconstructed transfer state derived from event history.
///
/// 根据事件历史恢复出的传输状态快照。
///
/// 这里不是新的领域真相，只是应用层为了校验“下一步能不能做”
/// 临时整理出来的判断结果。
#[derive(Debug, Default)]
struct TransferTimeline {
    started: bool,
    peer_id: Option<String>,
    terminal_state: Option<TerminalState>,
    last_progress_bytes: Option<u64>,
}

impl TransferTimeline {
    /// Rebuild the minimal state needed by the application layer.
    ///
    /// 从事件历史中恢复应用层所需的最小状态。
    ///
    /// 它不试图还原全部传输细节，只关心：
    /// - 是否已开始
    /// - 属于哪个对端
    /// - 是否已经结束
    /// - 最近一次进度值
    fn from_history(
        transfer_id: &str,
        history: &[FileTransferEvent],
    ) -> Result<Self, FileTransferApplicationError> {
        let mut timeline = Self::default();

        for event in history {
            match event {
                FileTransferEvent::Started {
                    transfer_id: event_transfer_id,
                    peer_id,
                    ..
                } => {
                    timeline.ensure_transfer_id_matches(transfer_id, event_transfer_id)?;
                    // 同一条传输只能有一个开始事实；出现第二个说明历史被污染了。
                    if timeline.started {
                        return Err(FileTransferApplicationError::InvalidHistory {
                            transfer_id: transfer_id.to_owned(),
                            message: "duplicate Started event".to_owned(),
                        });
                    }
                    // 结束之后又重新出现 Started，同样说明历史顺序不可信。
                    if timeline.terminal_state.is_some() {
                        return Err(FileTransferApplicationError::InvalidHistory {
                            transfer_id: transfer_id.to_owned(),
                            message: "Started event appears after terminal state".to_owned(),
                        });
                    }

                    timeline.started = true;
                    timeline.peer_id = Some(peer_id.clone());
                }
                FileTransferEvent::Progress {
                    transfer_id: event_transfer_id,
                    peer_id,
                    progress,
                } => {
                    timeline.ensure_transfer_id_matches(transfer_id, event_transfer_id)?;
                    timeline.ensure_active(transfer_id, peer_id)?;
                    timeline.last_progress_bytes = Some(progress.bytes_transferred);
                }
                FileTransferEvent::Completed {
                    transfer_id: event_transfer_id,
                    peer_id,
                } => {
                    timeline.ensure_transfer_id_matches(transfer_id, event_transfer_id)?;
                    timeline.ensure_active(transfer_id, peer_id)?;
                    timeline.terminal_state = Some(TerminalState::Completed);
                }
                FileTransferEvent::Failed {
                    transfer_id: event_transfer_id,
                    peer_id,
                    ..
                } => {
                    timeline.ensure_transfer_id_matches(transfer_id, event_transfer_id)?;
                    timeline.ensure_active(transfer_id, peer_id)?;
                    timeline.terminal_state = Some(TerminalState::Failed);
                }
                FileTransferEvent::Cancelled {
                    transfer_id: event_transfer_id,
                    peer_id,
                    ..
                } => {
                    timeline.ensure_transfer_id_matches(transfer_id, event_transfer_id)?;
                    timeline.ensure_active(transfer_id, peer_id)?;
                    timeline.terminal_state = Some(TerminalState::Cancelled);
                }
            }
        }

        Ok(timeline)
    }

    /// Ensure an event in history really belongs to the target transfer.
    ///
    /// 确认历史里的事件确实属于当前正在恢复的这条传输。
    fn ensure_transfer_id_matches(
        &self,
        expected_transfer_id: &str,
        actual_transfer_id: &str,
    ) -> Result<(), FileTransferApplicationError> {
        if expected_transfer_id == actual_transfer_id {
            Ok(())
        } else {
            Err(FileTransferApplicationError::InvalidHistory {
                transfer_id: expected_transfer_id.to_owned(),
                message: format!("history contains event for `{actual_transfer_id}`"),
            })
        }
    }

    /// Ensure the transfer is still active for the given peer.
    ///
    /// 确认这条传输对指定对端来说仍然处于可推进状态。
    ///
    /// 这里会同时校验：
    /// - 已经开始
    /// - 尚未结束
    /// - 对端一致
    fn ensure_active(
        &self,
        transfer_id: &str,
        peer_id: &str,
    ) -> Result<(), FileTransferApplicationError> {
        if !self.started {
            return Err(FileTransferApplicationError::TransferNotStarted {
                transfer_id: transfer_id.to_owned(),
            });
        }

        if self.terminal_state.is_some() {
            return Err(FileTransferApplicationError::TransferAlreadyFinished {
                transfer_id: transfer_id.to_owned(),
            });
        }

        match &self.peer_id {
            Some(expected_peer_id) if expected_peer_id == peer_id => Ok(()),
            Some(expected_peer_id) => Err(FileTransferApplicationError::PeerMismatch {
                transfer_id: transfer_id.to_owned(),
                expected_peer_id: expected_peer_id.clone(),
                actual_peer_id: peer_id.to_owned(),
            }),
            None => Err(FileTransferApplicationError::InvalidHistory {
                transfer_id: transfer_id.to_owned(),
                message: "started transfer is missing peer_id".to_owned(),
            }),
        }
    }
}
