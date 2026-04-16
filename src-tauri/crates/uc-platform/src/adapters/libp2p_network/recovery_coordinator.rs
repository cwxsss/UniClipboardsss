//! Recovery coordinator for the Connection Stability Recovery wave-1 feature.
//!
//! See `docs/p2p/2026-04-11-connection-stability-recovery-prd.md` for the
//! full specification this module implements.
//!
//! # Design
//!
//! [`RecoveryCoordinator`] is a **synchronous** state machine.  It holds no
//! async tasks and performs no I/O.  Every method either accepts an external
//! signal or performs a time-driven tick and returns a `Vec<CoordinatorCmd>`
//! that the caller (the swarm event loop) must execute.
//!
//! The coordinator is driven from the swarm loop's `select!` block via:
//!
//! 1. A `tokio::time::interval` of [`RECOVERY_PROBE_CADENCE`] that calls
//!    `tick()` on every fire.
//! 2. One-shot signal calls for external events: `on_mdns_expired`,
//!    `on_connection_established`, `on_probe_result`, `on_sleep_wake`,
//!    `on_network_change`.
//!
//! # Per-peer state
//!
//! Each peer in recovery is tracked in a [`RecoveryCycle`].  The cycle starts
//! when a trigger fires and ends when the peer transitions to `Online` or
//! `Offline`.
//!
//! # Silent / Visible phases (PRD §User-Facing State Model)
//!
//! - **0–15 s (silent phase):** internal recovery in progress; user-facing
//!   state remains `Online`.
//! - **15–120 s (visible phase):** user-facing state is `Recovering`.
//! - **>120 s:** allowed to transition to `Offline` only after the escalation
//!   ladder has been exhausted.
//!
//! # Escalation ladder
//!
//! 1. Step 1 — retry the usable path (`last_dial_observations`).
//! 2. Step 2 — refresh discovery; dial all known candidate addresses.
//! 3. Step 3 — rebuild the local network session.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use super::recovery_events::{PeerRuntimeState, RecoveryEvent, RecoveryProof, RecoveryTrigger};
use super::{
    RECOVERY_MULTI_PEER_REBUILD_OFFSET, RECOVERY_PROBE_CADENCE, RECOVERY_SILENT_PHASE_DURATION,
    RECOVERY_SILENT_PHASE_MAX_PROBES, RECOVERY_TIMED_REBUILD_PROBE_THRESHOLD, RECOVERY_WINDOW,
};

// ── Command type ─────────────────────────────────────────────────────────────

/// Commands returned by the coordinator for the swarm loop to execute.
#[derive(Debug)]
pub(crate) enum CoordinatorCmd {
    /// Open a business stream to `peer_id`, write nothing, close immediately.
    /// Uses the address in `last_dial_observations` (Step 1).
    SendProbe {
        peer_id: String,
        cycle_id: String,
        attempt: u32,
    },
    /// Dial all known candidate addresses for `peer_id` (Step 2 escalation).
    DialBroad {
        peer_id: String,
        cycle_id: String,
        escalation_level: u8,
    },
    /// Rebuild the local network session (Step 3 escalation).
    RebuildSession {
        rebuild_id: String,
        reason: RebuildReason,
        /// Peer IDs whose cycles are participating in this rebuild.
        participating_peer_ids: Vec<String>,
    },
    /// Emit a recovery-layer event for in-process observability (tracing/
    /// logging). These events never leave the libp2p adapter.
    EmitEvent(RecoveryEvent),
}

/// Reason a session rebuild was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RebuildReason {
    ImmediateSleepWake,
    ImmediateNetworkChange,
    TimedProbeFailures,
    MultiPeer,
}

impl RebuildReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RebuildReason::ImmediateSleepWake => "immediate_sleep_wake",
            RebuildReason::ImmediateNetworkChange => "immediate_network_change",
            RebuildReason::TimedProbeFailures => "timed_probe_failures",
            RebuildReason::MultiPeer => "multi_peer",
        }
    }
}

// ── Per-peer recovery cycle ───────────────────────────────────────────────────

/// Flags that signal an immediate rebuild context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImmediateContext {
    SleepWake,
    NetworkChange,
}

/// Runtime state for one recovery cycle for one peer.
#[derive(Debug)]
struct RecoveryCycle {
    cycle_id: String,
    peer_id: String,
    started_at: Instant,
    trigger: RecoveryTrigger,

    /// Current escalation level (0 = not yet escalated beyond initial probe).
    escalation_level: u8,

    /// Number of probes dispatched this cycle.
    total_probes: u32,

    /// Consecutive probe failures without an intervening success.
    consecutive_probe_failures: u32,

    /// Whether a session rebuild has been used in this cycle.
    rebuild_used: bool,

    /// When the last probe was dispatched.
    last_probe_at: Option<Instant>,

    /// Whether a probe is currently in flight (we avoid stacking probes).
    probe_in_flight: bool,

    /// Whether the silent phase has ended (15 s elapsed or early-ended).
    silent_phase_ended: bool,

    /// Pending immediate rebuild context (sleep/wake or network change).
    immediate_context: Option<ImmediateContext>,
}

impl RecoveryCycle {
    fn new(peer_id: String, trigger: RecoveryTrigger, now: Instant) -> Self {
        Self {
            cycle_id: Uuid::new_v4().to_string(),
            peer_id,
            started_at: now,
            trigger,
            escalation_level: 0,
            total_probes: 0,
            consecutive_probe_failures: 0,
            rebuild_used: false,
            last_probe_at: None,
            probe_in_flight: false,
            silent_phase_ended: false,
            immediate_context: None,
        }
    }

    fn elapsed(&self, now: Instant) -> Duration {
        now.duration_since(self.started_at)
    }

    fn in_silent_phase(&self, now: Instant) -> bool {
        !self.silent_phase_ended && self.elapsed(now) < RECOVERY_SILENT_PHASE_DURATION
    }

    /// Whether the next probe is due based on cadence and in-flight state.
    fn probe_due(&self, now: Instant) -> bool {
        if self.probe_in_flight {
            return false;
        }
        match self.last_probe_at {
            None => true,
            Some(last) => now.duration_since(last) >= RECOVERY_PROBE_CADENCE,
        }
    }

    /// Maximum probes allowed during the silent phase.
    fn silent_phase_probe_limit_reached(&self, now: Instant) -> bool {
        self.in_silent_phase(now) && self.total_probes >= RECOVERY_SILENT_PHASE_MAX_PROBES
    }
}

// ── Coordinator ───────────────────────────────────────────────────────────────

/// Central recovery state machine, driven synchronously by the swarm loop.
pub(crate) struct RecoveryCoordinator {
    /// Active recovery cycles keyed by peer_id.
    cycles: HashMap<String, RecoveryCycle>,

    /// When the *first* peer entered recovery in the current wave; used for
    /// the multi-peer rebuild evaluation.
    first_recovery_at: Option<Instant>,

    /// Whether the multi-peer rebuild has already been evaluated (once per
    /// first-peer silent-phase boundary).
    multi_peer_rebuild_evaluated: bool,
}

impl RecoveryCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            cycles: HashMap::new(),
            first_recovery_at: None,
            multi_peer_rebuild_evaluated: false,
        }
    }

    /// Returns `true` if `peer_id` is currently in a recovery cycle.
    pub(crate) fn is_recovering(&self, peer_id: &str) -> bool {
        self.cycles.contains_key(peer_id)
    }

    // ── External signal handlers ──────────────────────────────────────────

    /// mDNS expiry for `peer_id`.  The caller must only call this when the
    /// peer is *paired* (`Trusted` pairing state); unpaired peers are not
    /// subject to recovery.
    ///
    /// If the peer is already in a recovery cycle this starts a **new** cycle
    /// (a fresh mDNS flicker resets the cycle id per the PRD).
    #[instrument(
        name = "recovery.on_mdns_expired",
        level = "debug",
        skip(self, now),
        fields(peer_id = %peer_id)
    )]
    pub(crate) fn on_mdns_expired(&mut self, peer_id: String, now: Instant) -> Vec<CoordinatorCmd> {
        self.start_recovery_cycle(peer_id, RecoveryTrigger::MdnsExpired, now)
    }

    /// A fresh `ConnectionEstablished` swarm event arrived for `peer_id`.
    ///
    /// This is transport-level proof of recovery per PRD §Recovery Success
    /// Criteria.
    #[instrument(
        name = "recovery.on_connection_established",
        level = "debug",
        skip(self, now),
        fields(peer_id = %peer_id)
    )]
    pub(crate) fn on_connection_established(
        &mut self,
        peer_id: &str,
        now: Instant,
    ) -> Vec<CoordinatorCmd> {
        let Some(cycle) = self.cycles.remove(peer_id) else {
            return Vec::new();
        };
        let elapsed_ms = cycle.elapsed(now).as_millis() as u64;
        info!(
            event = "peer.recovery_cycle_succeeded",
            peer_id = %peer_id,
            recovery_cycle_id = %cycle.cycle_id,
            trigger = ?cycle.trigger,
            elapsed_ms,
            proof = "connection_established",
            "recovery cycle ended by fresh ConnectionEstablished"
        );
        self.check_reset_multi_peer_tracking();

        vec![
            CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                peer_id: peer_id.to_string(),
                state: PeerRuntimeState::Online,
                cycle_id: None,
            }),
            CoordinatorCmd::EmitEvent(RecoveryEvent::PeerRecovered {
                peer_id: peer_id.to_string(),
                cycle_id: cycle.cycle_id,
                elapsed_ms,
                proof: RecoveryProof::ConnectionEstablished,
            }),
        ]
    }

    /// Result of a recovery probe dispatched earlier.
    #[instrument(
        name = "recovery.on_probe_result",
        level = "debug",
        skip(self, error, now),
        fields(peer_id = %peer_id, recovery_cycle_id = %cycle_id, success)
    )]
    pub(crate) fn on_probe_result(
        &mut self,
        peer_id: &str,
        cycle_id: &str,
        success: bool,
        error: Option<&str>,
        now: Instant,
    ) -> Vec<CoordinatorCmd> {
        let Some(cycle) = self.cycles.get_mut(peer_id) else {
            debug!(
                event = "peer.recovery_probe_result_ignored",
                reason = "no_active_cycle",
                "ignoring probe result for peer without active recovery cycle"
            );
            return Vec::new();
        };
        if cycle.cycle_id != cycle_id {
            // Stale result from a superseded cycle; discard.
            debug!(
                event = "peer.recovery_probe_result_ignored",
                reason = "stale_cycle",
                active_cycle_id = %cycle.cycle_id,
                "ignoring probe result from superseded cycle"
            );
            return Vec::new();
        }
        cycle.probe_in_flight = false;

        if success {
            let elapsed_ms = cycle.elapsed(now).as_millis() as u64;
            let cid = cycle.cycle_id.clone();
            let trigger = cycle.trigger;
            self.cycles.remove(peer_id);
            info!(
                event = "peer.recovery_cycle_succeeded",
                peer_id = %peer_id,
                recovery_cycle_id = %cid,
                trigger = ?trigger,
                elapsed_ms,
                proof = "business_stream_open",
                "recovery cycle ended by successful probe"
            );
            self.check_reset_multi_peer_tracking();
            return vec![
                CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                    peer_id: peer_id.to_string(),
                    state: PeerRuntimeState::Online,
                    cycle_id: None,
                }),
                CoordinatorCmd::EmitEvent(RecoveryEvent::PeerRecovered {
                    peer_id: peer_id.to_string(),
                    cycle_id: cid,
                    elapsed_ms,
                    proof: RecoveryProof::BusinessStreamOpen,
                }),
            ];
        }

        // Probe failed.
        let cycle = self.cycles.get_mut(peer_id).unwrap();
        cycle.consecutive_probe_failures += 1;
        debug!(
            event = "peer.recovery_probe_failure_recorded",
            consecutive_failures = cycle.consecutive_probe_failures,
            total_probes = cycle.total_probes,
            silent_phase_active = !cycle.silent_phase_ended,
            "probe failure accounted"
        );
        // The structured failure cause is logged at warn level by
        // `recovery_probe.rs` with stable fields — don't repeat-bomb here.
        let _ = error;

        // Immediate rebuild: if context set and first probe just failed.
        if !cycle.rebuild_used {
            if let Some(ctx) = cycle.immediate_context.take() {
                let reason = match ctx {
                    ImmediateContext::SleepWake => RebuildReason::ImmediateSleepWake,
                    ImmediateContext::NetworkChange => RebuildReason::ImmediateNetworkChange,
                };
                return self.trigger_rebuild(peer_id, reason, now);
            }
        }

        Vec::new()
    }

    /// The local device just resumed from sleep.  Marks all active cycles so
    /// the next probe failure for each triggers an immediate rebuild.
    #[instrument(
        name = "recovery.on_sleep_wake",
        level = "info",
        skip(self, now),
        fields(active_cycles = self.cycles.len())
    )]
    pub(crate) fn on_sleep_wake(&mut self, now: Instant) -> Vec<CoordinatorCmd> {
        let mut marked = 0u32;
        for cycle in self.cycles.values_mut() {
            // Only override if not already set (first context wins).
            if cycle.immediate_context.is_none() {
                cycle.immediate_context = Some(ImmediateContext::SleepWake);
                marked += 1;
            }
        }
        info!(
            event = "peer.recovery_immediate_context_set",
            context = "sleep_wake",
            marked_cycles = marked,
            "sleep/wake signal armed immediate rebuild on next probe failure"
        );

        // For peers not yet in recovery, the trigger will be handled when
        // their next outbound sync attempt fails (FirstAttemptAfterIdle) or
        // when mDNS expires.  Peers that were already online and connected
        // may still be fine; don't start recovery proactively here.
        let _ = now;
        Vec::new()
    }

    /// The local network interface or IP address changed.  Same semantics as
    /// `on_sleep_wake` for active cycles.
    #[instrument(
        name = "recovery.on_network_change",
        level = "info",
        skip(self, now),
        fields(active_cycles = self.cycles.len())
    )]
    pub(crate) fn on_network_change(&mut self, now: Instant) -> Vec<CoordinatorCmd> {
        let mut marked = 0u32;
        for cycle in self.cycles.values_mut() {
            if cycle.immediate_context.is_none() {
                cycle.immediate_context = Some(ImmediateContext::NetworkChange);
                marked += 1;
            }
        }
        info!(
            event = "peer.recovery_immediate_context_set",
            context = "network_change",
            marked_cycles = marked,
            "network-change signal armed immediate rebuild on next probe failure"
        );
        let _ = now;
        Vec::new()
    }

    /// Dial failure streak for `peer_id` (used when outbound delivery to a
    /// paired peer fails repeatedly).  Starts a recovery cycle if not already
    /// in one.
    #[instrument(
        name = "recovery.on_dial_failure_streak",
        level = "debug",
        skip(self, now),
        fields(peer_id = %peer_id)
    )]
    pub(crate) fn on_dial_failure_streak(
        &mut self,
        peer_id: String,
        now: Instant,
    ) -> Vec<CoordinatorCmd> {
        if self.cycles.contains_key(&peer_id) {
            debug!(
                event = "peer.recovery_dial_streak_ignored",
                reason = "already_recovering",
                "dial failure streak ignored — peer already in recovery"
            );
            return Vec::new();
        }
        self.start_recovery_cycle(peer_id, RecoveryTrigger::DialFailureStreak, now)
    }

    // ── Time-driven tick ──────────────────────────────────────────────────

    /// Drive the coordinator forward.  Must be called on every
    /// `RECOVERY_PROBE_CADENCE` interval tick from the swarm loop.
    ///
    /// Returns commands to execute for this tick.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<CoordinatorCmd> {
        let mut cmds = Vec::new();

        // Collect peer_ids to process (avoids borrow issues).
        let peer_ids: Vec<String> = self.cycles.keys().cloned().collect();

        for peer_id in peer_ids {
            let cmds_for_peer = self.tick_peer(&peer_id, now);
            cmds.extend(cmds_for_peer);
        }

        // Multi-peer rebuild evaluation — runs once when the first peer's
        // silent phase ends (15 s mark).
        if !self.multi_peer_rebuild_evaluated {
            if let Some(first_at) = self.first_recovery_at {
                if now.duration_since(first_at) >= RECOVERY_MULTI_PEER_REBUILD_OFFSET {
                    let multi_cmds = self.evaluate_multi_peer_rebuild(now);
                    cmds.extend(multi_cmds);
                    self.multi_peer_rebuild_evaluated = true;
                }
            }
        }

        cmds
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn start_recovery_cycle(
        &mut self,
        peer_id: String,
        trigger: RecoveryTrigger,
        now: Instant,
    ) -> Vec<CoordinatorCmd> {
        // Remove any existing cycle (mDNS flicker = new cycle id).
        let superseded = self.cycles.remove(&peer_id).is_some();

        let cycle = RecoveryCycle::new(peer_id.clone(), trigger, now);
        let cycle_id = cycle.cycle_id.clone();

        if self.first_recovery_at.is_none() {
            self.first_recovery_at = Some(now);
            self.multi_peer_rebuild_evaluated = false;
        }

        self.cycles.insert(peer_id.clone(), cycle);

        info!(
            event = "peer.recovery_cycle_started",
            peer_id = %peer_id,
            recovery_cycle_id = %cycle_id,
            trigger = ?trigger,
            active_cycle_count = self.cycles.len(),
            superseded_previous_cycle = superseded,
            "recovery cycle started"
        );

        // During silent phase the user-facing state stays Online.
        // We still emit PeerRecoveryStarted so tracing can follow the cycle.
        vec![CoordinatorCmd::EmitEvent(
            RecoveryEvent::PeerRecoveryStarted {
                peer_id: peer_id.clone(),
                cycle_id: cycle_id.clone(),
                trigger,
            },
        )]
    }

    fn tick_peer(&mut self, peer_id: &str, now: Instant) -> Vec<CoordinatorCmd> {
        let mut cmds = Vec::new();

        // --- Phase transition: silent → visible ---
        {
            let Some(cycle) = self.cycles.get_mut(peer_id) else {
                return cmds;
            };
            if !cycle.silent_phase_ended && cycle.elapsed(now) >= RECOVERY_SILENT_PHASE_DURATION {
                cycle.silent_phase_ended = true;
                let cid = cycle.cycle_id.clone();
                let elapsed_ms = cycle.elapsed(now).as_millis() as u64;
                let total_probes = cycle.total_probes;

                info!(
                    event = "peer.recovery_silent_phase_ended",
                    peer_id = %peer_id,
                    recovery_cycle_id = %cid,
                    elapsed_ms,
                    total_probes,
                    from_state = "Online",
                    to_state = "Recovering",
                    escalation_level = 2u8,
                    "silent phase ended; escalating to Step 2 broad dial"
                );

                cmds.push(CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                    peer_id: peer_id.to_string(),
                    state: PeerRuntimeState::Recovering,
                    cycle_id: Some(cid.clone()),
                }));

                // Step 2 escalation: broaden dialing.
                cycle.escalation_level = cycle.escalation_level.max(2);
                cmds.push(CoordinatorCmd::DialBroad {
                    peer_id: peer_id.to_string(),
                    cycle_id: cid,
                    escalation_level: 2,
                });
            }
        }

        // --- Recovery window exhausted → Offline ---
        {
            let Some(cycle) = self.cycles.get(peer_id) else {
                return cmds;
            };
            if cycle.elapsed(now) >= RECOVERY_WINDOW {
                let cid = cycle.cycle_id.clone();
                let trigger = cycle.trigger;
                let last_escalation = cycle.escalation_level;
                let elapsed_ms = cycle.elapsed(now).as_millis() as u64;
                let total_probes = cycle.total_probes;
                let rebuild_used = cycle.rebuild_used;
                warn!(
                    event = "peer.recovery_window_exhausted",
                    peer_id = %peer_id,
                    recovery_cycle_id = %cid,
                    trigger = ?trigger,
                    elapsed_ms,
                    last_escalation,
                    total_probes,
                    rebuild_used,
                    error_kind = "recovery_window_exhausted",
                    retryable = false,
                    "recovery window exhausted; transitioning peer to Offline"
                );
                self.cycles.remove(peer_id);
                self.check_reset_multi_peer_tracking();

                cmds.push(CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                    peer_id: peer_id.to_string(),
                    state: PeerRuntimeState::Offline,
                    cycle_id: None,
                }));
                cmds.push(CoordinatorCmd::EmitEvent(
                    RecoveryEvent::PeerRecoveryFailed {
                        peer_id: peer_id.to_string(),
                        cycle_id: cid,
                        elapsed_ms,
                        last_escalation,
                    },
                ));
                return cmds;
            }
        }

        // --- Timed rebuild trigger ---
        {
            let Some(cycle) = self.cycles.get(peer_id) else {
                return cmds;
            };
            if cycle.silent_phase_ended
                && !cycle.rebuild_used
                && cycle.consecutive_probe_failures >= RECOVERY_TIMED_REBUILD_PROBE_THRESHOLD
            {
                let rebuild_cmds =
                    self.trigger_rebuild(peer_id, RebuildReason::TimedProbeFailures, now);
                cmds.extend(rebuild_cmds);
                // trigger_rebuild may have removed the cycle; return early.
                if !self.cycles.contains_key(peer_id) {
                    return cmds;
                }
            }
        }

        // --- Probe cadence ---
        {
            let Some(cycle) = self.cycles.get_mut(peer_id) else {
                return cmds;
            };

            // During silent phase respect max-probe limit.
            if cycle.silent_phase_probe_limit_reached(now) {
                return cmds;
            }

            if cycle.probe_due(now) {
                cycle.total_probes += 1;
                cycle.probe_in_flight = true;
                cycle.last_probe_at = Some(now);
                let attempt = cycle.total_probes;
                let cid = cycle.cycle_id.clone();

                cmds.push(CoordinatorCmd::SendProbe {
                    peer_id: peer_id.to_string(),
                    cycle_id: cid,
                    attempt,
                });
            }
        }

        cmds
    }

    /// Issue a session rebuild command for `peer_id`'s cycle.
    fn trigger_rebuild(
        &mut self,
        peer_id: &str,
        reason: RebuildReason,
        now: Instant,
    ) -> Vec<CoordinatorCmd> {
        let Some(cycle) = self.cycles.get_mut(peer_id) else {
            return Vec::new();
        };
        if cycle.rebuild_used {
            debug!(
                event = "peer.recovery_rebuild_skipped",
                peer_id = %peer_id,
                recovery_cycle_id = %cycle.cycle_id,
                reason = "rebuild_already_used",
                "rebuild suppressed: already used in this cycle"
            );
            return Vec::new();
        }
        cycle.rebuild_used = true;
        cycle.escalation_level = cycle.escalation_level.max(3);
        let was_silent_phase = !cycle.silent_phase_ended;
        let elapsed_ms = cycle.elapsed(now).as_millis() as u64;

        // Immediate rebuild ends the silent phase early.
        if !cycle.silent_phase_ended {
            cycle.silent_phase_ended = true;
            let cid = cycle.cycle_id.clone();
            let rebuild_id = Uuid::new_v4().to_string();

            info!(
                event = "peer.recovery_rebuild_triggered",
                peer_id = %peer_id,
                recovery_cycle_id = %cid,
                rebuild_id = %rebuild_id,
                rebuild_reason = reason.as_str(),
                escalation_level = 3u8,
                elapsed_ms,
                ended_silent_phase_early = was_silent_phase,
                "session rebuild triggered (silent-phase early exit)"
            );

            return vec![
                CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                    peer_id: peer_id.to_string(),
                    state: PeerRuntimeState::Recovering,
                    cycle_id: Some(cid),
                }),
                CoordinatorCmd::RebuildSession {
                    rebuild_id,
                    reason,
                    participating_peer_ids: vec![peer_id.to_string()],
                },
            ];
        }

        let rebuild_id = Uuid::new_v4().to_string();

        info!(
            event = "peer.recovery_rebuild_triggered",
            peer_id = %peer_id,
            rebuild_id = %rebuild_id,
            rebuild_reason = reason.as_str(),
            escalation_level = 3u8,
            elapsed_ms,
            ended_silent_phase_early = false,
            "session rebuild triggered"
        );

        vec![CoordinatorCmd::RebuildSession {
            rebuild_id,
            reason,
            participating_peer_ids: vec![peer_id.to_string()],
        }]
    }

    /// Multi-peer rebuild evaluation.  Called once at the 15 s mark after the
    /// first peer entered recovery.
    fn evaluate_multi_peer_rebuild(&mut self, now: Instant) -> Vec<CoordinatorCmd> {
        // Collect peers still in recovery that haven't recovered during their
        // own silent phase.
        let candidates: Vec<String> = self
            .cycles
            .values()
            .filter(|c| c.silent_phase_ended || c.elapsed(now) >= RECOVERY_SILENT_PHASE_DURATION)
            .map(|c| c.peer_id.clone())
            .collect();

        if candidates.len() < 2 {
            debug!(
                event = "peer.recovery_multi_peer_rebuild_skipped",
                candidate_count = candidates.len(),
                reason = "below_threshold",
                "multi-peer rebuild not triggered"
            );
            return Vec::new();
        }

        // Check: none of them have already used a rebuild.
        let any_rebuilt = candidates
            .iter()
            .any(|pid| self.cycles.get(pid).is_some_and(|c| c.rebuild_used));
        if any_rebuilt {
            debug!(
                event = "peer.recovery_multi_peer_rebuild_skipped",
                candidate_count = candidates.len(),
                reason = "already_rebuilt",
                "multi-peer rebuild not triggered: one or more peers already used rebuild"
            );
            return Vec::new();
        }

        let rebuild_id = Uuid::new_v4().to_string();

        info!(
            event = "peer.recovery_rebuild_triggered",
            rebuild_id = %rebuild_id,
            rebuild_reason = "multi_peer",
            participating_peer_count = candidates.len(),
            "multi-peer session rebuild triggered"
        );

        // Mark all participating cycles as having used the rebuild.
        for pid in &candidates {
            if let Some(cycle) = self.cycles.get_mut(pid) {
                cycle.rebuild_used = true;
                cycle.escalation_level = cycle.escalation_level.max(3);
                if !cycle.silent_phase_ended {
                    cycle.silent_phase_ended = true;
                }
            }
        }

        // Emit Recovering for any that are still in silent phase.
        let mut cmds: Vec<CoordinatorCmd> = candidates
            .iter()
            .filter_map(|pid| {
                self.cycles.get(pid).map(|c| {
                    CoordinatorCmd::EmitEvent(RecoveryEvent::PeerStateChanged {
                        peer_id: pid.clone(),
                        state: PeerRuntimeState::Recovering,
                        cycle_id: Some(c.cycle_id.clone()),
                    })
                })
            })
            .collect();

        cmds.push(CoordinatorCmd::RebuildSession {
            rebuild_id,
            reason: RebuildReason::MultiPeer,
            participating_peer_ids: candidates,
        });

        cmds
    }

    /// Reset multi-peer tracking when no cycles remain.
    fn check_reset_multi_peer_tracking(&mut self) {
        if self.cycles.is_empty() {
            self.first_recovery_at = None;
            self.multi_peer_rebuild_evaluated = false;
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────
