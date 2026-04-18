//! Setup user-intent UseCases (phase B.3).
//!
//! Each UseCase is a thin wrapper over a single `SetupOrchestrator` method,
//! naming one user intent or system action so it can be routed from
//! `SetupFacade` and independently tested. All UseCases are `pub(crate)` per
//! `uc-application/AGENTS.md` §11.4 — the orchestrator never leaks out.

pub(crate) mod apply_joiner_space_access_result;
pub(crate) mod cancel_setup;
pub(crate) mod clear_setup_transient_state;
pub(crate) mod complete_join_space;
pub(crate) mod confirm_peer_trust;
pub(crate) mod get_setup_state;
pub(crate) mod reset_setup;
pub(crate) mod resolve_host_space_access_proof;
pub(crate) mod select_join_peer;
pub(crate) mod start_join_space;
pub(crate) mod start_new_space;
pub(crate) mod start_sponsor_authorization_for_joiner;
pub(crate) mod submit_new_space_passphrase;
pub(crate) mod verify_join_passphrase;

pub(crate) use apply_joiner_space_access_result::ApplyJoinerSpaceAccessResultUseCase;
pub(crate) use cancel_setup::CancelSetupUseCase;
pub(crate) use clear_setup_transient_state::ClearSetupTransientStateUseCase;
pub(crate) use complete_join_space::CompleteJoinSpaceUseCase;
pub(crate) use confirm_peer_trust::ConfirmPeerTrustUseCase;
pub(crate) use get_setup_state::GetSetupStateQuery;
pub(crate) use reset_setup::ResetSetupUseCase;
pub(crate) use resolve_host_space_access_proof::ResolveHostSpaceAccessProofUseCase;
pub(crate) use select_join_peer::SelectJoinPeerUseCase;
pub(crate) use start_join_space::StartJoinSpaceUseCase;
pub(crate) use start_new_space::StartNewSpaceUseCase;
pub(crate) use start_sponsor_authorization_for_joiner::StartSponsorAuthorizationForJoinerUseCase;
pub(crate) use submit_new_space_passphrase::SubmitNewSpacePassphraseUseCase;
pub(crate) use verify_join_passphrase::VerifyJoinPassphraseUseCase;
