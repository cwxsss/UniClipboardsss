use tracing::warn;

use crate::setup::{SetupAction, SetupError, SetupEvent, SetupState};

pub struct SetupStateMachine;

impl SetupStateMachine {
    pub fn transition(state: SetupState, event: SetupEvent) -> (SetupState, Vec<SetupAction>) {
        match (state, event) {
            // 1. Welcome
            (SetupState::Welcome, SetupEvent::StartNewSpace) => (
                SetupState::CreateSpaceInputPassphrase { error: None },
                vec![],
            ),
            (SetupState::Welcome, SetupEvent::StartJoinSpace) => (
                SetupState::JoinSpaceSelectDevice { error: None },
                vec![SetupAction::EnsureDiscovery],
            ),

            // 2. CreateSpaceInputPassphrase
            (
                SetupState::CreateSpaceInputPassphrase { .. },
                SetupEvent::SubmitPassphrase { .. },
            ) => (
                SetupState::ProcessingCreateSpace {
                    message: Some("Creating encrypted space…".into()),
                },
                vec![SetupAction::CreateEncryptedSpace],
            ),
            (SetupState::CreateSpaceInputPassphrase { .. }, SetupEvent::CancelSetup) => {
                (SetupState::Welcome, vec![])
            }
            (SetupState::ProcessingCreateSpace { .. }, SetupEvent::CreateSpaceFailed { error }) => {
                (
                    SetupState::CreateSpaceInputPassphrase { error: Some(error) },
                    vec![],
                )
            }

            // 3. JoinSpaceSelectDevice
            (SetupState::JoinSpaceSelectDevice { .. }, SetupEvent::ChooseJoinPeer { .. }) => (
                // NOTE: peer_id is stored into SetupContext by orchestrator
                // before or after dispatching this event.
                SetupState::ProcessingJoinSpace {
                    message: Some("Connecting to selected device…".into()),
                },
                vec![SetupAction::EnsurePairing {}],
            ),
            (state @ SetupState::JoinSpaceSelectDevice { .. }, SetupEvent::RefreshPeerList) => {
                (state, vec![SetupAction::EnsureDiscovery])
            }
            (SetupState::JoinSpaceSelectDevice { .. }, SetupEvent::CancelSetup) => {
                (SetupState::Welcome, vec![])
            }

            // 4. JoinSpaceConfirmPeer
            (SetupState::JoinSpaceConfirmPeer { .. }, SetupEvent::ConfirmPeerTrust) => (
                SetupState::JoinSpaceInputPassphrase { error: None },
                vec![SetupAction::ConfirmPeerTrust {}],
            ),
            (SetupState::JoinSpaceConfirmPeer { .. }, SetupEvent::CancelSetup) => (
                SetupState::JoinSpaceSelectDevice { error: None },
                vec![SetupAction::AbortPairing {}],
            ),

            // 5. JoinSpaceInputPassphrase
            (SetupState::JoinSpaceInputPassphrase { .. }, SetupEvent::SubmitPassphrase { .. }) => (
                SetupState::ProcessingJoinSpace {
                    message: Some("Verifying passphrase…".into()),
                },
                vec![SetupAction::StartJoinSpaceAccess {}],
            ),
            (SetupState::JoinSpaceInputPassphrase { .. }, SetupEvent::VerifyPassphrase { .. }) => (
                SetupState::ProcessingJoinSpace {
                    message: Some("Verifying passphrase…".into()),
                },
                vec![SetupAction::StartJoinSpaceAccess {}],
            ),
            (SetupState::JoinSpaceInputPassphrase { .. }, SetupEvent::CancelSetup) => {
                (SetupState::JoinSpaceSelectDevice { error: None }, vec![])
            }

            // 6. Processing
            (SetupState::ProcessingJoinSpace { .. }, SetupEvent::JoinSpaceSucceeded) => {
                (SetupState::Completed, vec![SetupAction::MarkSetupComplete])
            }
            (SetupState::ProcessingCreateSpace { .. }, SetupEvent::CreateSpaceSucceeded) => {
                (SetupState::Completed, vec![SetupAction::MarkSetupComplete])
            }
            (SetupState::ProcessingJoinSpace { .. }, SetupEvent::JoinSpaceFailed { error }) => {
                let target = match &error {
                    // Passphrase-related failures → return to passphrase input
                    SetupError::PassphraseInvalidOrMismatch
                    | SetupError::PassphraseMismatch
                    | SetupError::PassphraseEmpty => {
                        SetupState::JoinSpaceInputPassphrase { error: Some(error) }
                    }
                    // Pairing/network failures → return to device selection
                    SetupError::PairingFailed
                    | SetupError::PairingRejected
                    | SetupError::PeerUnavailable
                    | SetupError::NetworkTimeout => {
                        SetupState::JoinSpaceSelectDevice { error: Some(error) }
                    }
                };
                (target, vec![])
            }
            (SetupState::ProcessingJoinSpace { .. }, SetupEvent::CancelSetup) => (
                SetupState::JoinSpaceSelectDevice { error: None },
                vec![SetupAction::AbortPairing {}],
            ),
            (SetupState::ProcessingCreateSpace { .. }, SetupEvent::CancelSetup) => {
                (SetupState::Welcome, vec![SetupAction::AbortPairing {}])
            }

            // 7. Completed
            (state @ SetupState::Completed, _) => (state, vec![]),

            // 8. Invalid
            (state, event) => {
                warn!(?state, ?event, "invalid setup transition");
                (state, vec![SetupAction::AbortPairing {}])
            }
        }
    }
}
