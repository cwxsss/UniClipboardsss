use chrono::{DateTime, Duration, Utc};
use tracing::warn;

use crate::space_access::action::SpaceAccessAction;
use crate::space_access::event::SpaceAccessEvent;
use crate::space_access::state::{CancelReason, SpaceAccessState};

pub struct SpaceAccessStateMachine;

impl SpaceAccessStateMachine {
    pub fn transition(
        state: SpaceAccessState,
        event: SpaceAccessEvent,
    ) -> (SpaceAccessState, Vec<SpaceAccessAction>) {
        Self::transition_at(state, event, Utc::now())
    }

    pub(crate) fn transition_at(
        state: SpaceAccessState,
        event: SpaceAccessEvent,
        now: DateTime<Utc>,
    ) -> (SpaceAccessState, Vec<SpaceAccessAction>) {
        match (state, event) {
            // ===== Start =====
            (
                SpaceAccessState::Idle,
                SpaceAccessEvent::JoinRequested {
                    pairing_session_id,
                    ttl_secs,
                },
            ) => {
                let expires_at = now + Duration::seconds(ttl_secs as i64);
                (
                    SpaceAccessState::WaitingOffer {
                        pairing_session_id,
                        expires_at,
                    },
                    vec![SpaceAccessAction::StartTimer { ttl_secs }],
                )
            }
            (
                SpaceAccessState::Idle,
                SpaceAccessEvent::SponsorAuthorizationRequested {
                    pairing_session_id,
                    space_id,
                    ttl_secs,
                },
            ) => {
                let expires_at = now + Duration::seconds(ttl_secs as i64);
                let actions = vec![
                    SpaceAccessAction::RequestOfferPreparation {
                        pairing_session_id: pairing_session_id.clone().into(),
                        space_id: space_id.clone(),
                        expires_at,
                    },
                    SpaceAccessAction::SendOffer,
                    SpaceAccessAction::StartTimer { ttl_secs },
                ];
                (
                    SpaceAccessState::WaitingJoinerProof {
                        pairing_session_id,
                        space_id,
                        expires_at,
                    },
                    actions,
                )
            }

            // ===== Offer =====
            (
                SpaceAccessState::WaitingOffer { .. },
                SpaceAccessEvent::OfferAccepted {
                    pairing_session_id,
                    space_id,
                    expires_at,
                },
            ) => {
                let ttl_secs = ttl_from_expires_at(expires_at, now);
                (
                    SpaceAccessState::WaitingUserPassphrase {
                        pairing_session_id,
                        space_id,
                        expires_at,
                    },
                    vec![
                        SpaceAccessAction::StopTimer,
                        SpaceAccessAction::StartTimer { ttl_secs },
                    ],
                )
            }

            // ===== User input =====
            (
                SpaceAccessState::WaitingUserPassphrase {
                    space_id,
                    pairing_session_id,
                    ..
                },
                SpaceAccessEvent::PassphraseSubmitted,
            ) => (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id,
                    space_id: space_id.clone(),
                    sent_at: now,
                },
                vec![
                    SpaceAccessAction::RequestSpaceKeyDerivation { space_id },
                    SpaceAccessAction::SendProof,
                ],
            ),

            // ===== Proof =====
            (
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id,
                    space_id,
                    ..
                },
                SpaceAccessEvent::ProofVerified { .. },
            ) => (
                SpaceAccessState::Granted {
                    pairing_session_id,
                    space_id: space_id.clone(),
                },
                vec![
                    SpaceAccessAction::SendResult,
                    SpaceAccessAction::PersistSponsorAccess { space_id },
                    SpaceAccessAction::StopTimer,
                ],
            ),
            (
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id,
                    space_id,
                    ..
                },
                SpaceAccessEvent::ProofRejected { reason, .. },
            ) => (
                SpaceAccessState::Denied {
                    pairing_session_id,
                    space_id,
                    reason,
                },
                vec![SpaceAccessAction::SendResult, SpaceAccessAction::StopTimer],
            ),

            // ===== Result =====
            (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id,
                    space_id,
                    ..
                },
                SpaceAccessEvent::AccessGranted { .. },
            ) => (
                SpaceAccessState::Granted {
                    pairing_session_id,
                    space_id: space_id.clone(),
                },
                vec![
                    SpaceAccessAction::PersistJoinerAccess { space_id },
                    SpaceAccessAction::StopTimer,
                ],
            ),
            (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id,
                    space_id,
                    ..
                },
                SpaceAccessEvent::AccessDenied { reason, .. },
            ) => (
                SpaceAccessState::Denied {
                    pairing_session_id,
                    space_id,
                    reason,
                },
                vec![SpaceAccessAction::StopTimer],
            ),

            // ===== Cancel / Timeout =====
            (
                SpaceAccessState::WaitingOffer {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::CancelledByUser,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::UserCancelled,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingOffer {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::Timeout,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::Timeout,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingOffer {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::SessionClosed,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::SessionClosed,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingUserPassphrase {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::CancelledByUser,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::UserCancelled,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingUserPassphrase {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::Timeout,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::Timeout,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingUserPassphrase {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::SessionClosed,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::SessionClosed,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::CancelledByUser,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::UserCancelled,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::Timeout,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::Timeout,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingDecision {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::SessionClosed,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::SessionClosed,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::CancelledByUser,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::UserCancelled,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::Timeout,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::Timeout,
                },
                vec![SpaceAccessAction::StopTimer],
            ),
            (
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id, ..
                },
                SpaceAccessEvent::SessionClosed,
            ) => (
                SpaceAccessState::Cancelled {
                    pairing_session_id,
                    reason: CancelReason::SessionClosed,
                },
                vec![SpaceAccessAction::StopTimer],
            ),

            // ===== Sponsor re-authorization from any non-Idle state =====
            // After completing authorization for one joiner, or when a previous
            // session left stale state (e.g. WaitingJoinerProof from a failed
            // pairing), the sponsor must be able to start a fresh authorization.
            (
                SpaceAccessState::Granted { .. }
                | SpaceAccessState::Denied { .. }
                | SpaceAccessState::Cancelled { .. }
                | SpaceAccessState::WaitingJoinerProof { .. }
                | SpaceAccessState::WaitingOffer { .. }
                | SpaceAccessState::WaitingUserPassphrase { .. }
                | SpaceAccessState::WaitingDecision { .. },
                SpaceAccessEvent::SponsorAuthorizationRequested {
                    pairing_session_id,
                    space_id,
                    ttl_secs,
                },
            ) => {
                let expires_at = now + Duration::seconds(ttl_secs as i64);
                let actions = vec![
                    SpaceAccessAction::RequestOfferPreparation {
                        pairing_session_id: pairing_session_id.clone().into(),
                        space_id: space_id.clone(),
                        expires_at,
                    },
                    SpaceAccessAction::SendOffer,
                    SpaceAccessAction::StartTimer { ttl_secs },
                ];
                (
                    SpaceAccessState::WaitingJoinerProof {
                        pairing_session_id,
                        space_id,
                        expires_at,
                    },
                    actions,
                )
            }

            // ===== Terminal =====
            (state @ SpaceAccessState::Granted { .. }, _) => (state, vec![]),
            (state @ SpaceAccessState::Denied { .. }, _) => (state, vec![]),
            (state @ SpaceAccessState::Cancelled { .. }, _) => (state, vec![]),

            // ===== Invalid =====
            (state, event) => {
                warn!(?state, ?event, "invalid space access transition");
                (state, vec![])
            }
        }
    }
}

fn ttl_from_expires_at(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    let delta = expires_at.signed_duration_since(now).num_seconds();
    if delta <= 0 {
        0
    } else {
        delta as u64
    }
}
