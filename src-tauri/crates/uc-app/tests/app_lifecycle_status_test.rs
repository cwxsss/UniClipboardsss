use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use uc_app::usecases::{
    AppLifecycleCoordinator, AppLifecycleCoordinatorDeps, LifecycleEvent, LifecycleEventEmitter,
    LifecycleState, LifecycleStatusPort, SessionReadyEmitter, StartNetworkAfterUnlock,
};
use uc_core::ports::network_control::NetworkControlPort;

// ---------------------------------------------------------------------------
// Mock: NetworkControlPort (configurable failure)
// ---------------------------------------------------------------------------

struct MockNetworkControl {
    calls: Arc<AtomicUsize>,
    error_message: Option<String>,
}

#[async_trait]
impl NetworkControlPort for MockNetworkControl {
    async fn start_network(&self) -> anyhow::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(message) = &self.error_message {
            return Err(anyhow::anyhow!(message.clone()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock: SessionReadyEmitter
// ---------------------------------------------------------------------------

struct MockSessionReadyEmitter {
    events: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl SessionReadyEmitter for MockSessionReadyEmitter {
    async fn emit_ready(&self) -> anyhow::Result<()> {
        let mut guard = self.events.lock().await;
        guard.push("ready".to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock: LifecycleStatusPort
// ---------------------------------------------------------------------------

struct MockLifecycleStatus {
    states: Arc<Mutex<Vec<LifecycleState>>>,
}

#[async_trait]
impl LifecycleStatusPort for MockLifecycleStatus {
    async fn set_state(&self, state: LifecycleState) -> anyhow::Result<()> {
        let mut guard = self.states.lock().await;
        guard.push(state);
        Ok(())
    }

    async fn get_state(&self) -> LifecycleState {
        let guard = self.states.lock().await;
        guard.last().cloned().unwrap_or(LifecycleState::Idle)
    }
}

// ---------------------------------------------------------------------------
// Mock: LifecycleEventEmitter
// ---------------------------------------------------------------------------

struct MockLifecycleEventEmitter {
    events: Arc<Mutex<Vec<LifecycleEvent>>>,
}

#[async_trait]
impl LifecycleEventEmitter for MockLifecycleEventEmitter {
    async fn emit_lifecycle_event(&self, event: LifecycleEvent) -> anyhow::Result<()> {
        let mut guard = self.events.lock().await;
        guard.push(event);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestMocks {
    network_calls: Arc<AtomicUsize>,
    session_events: Arc<Mutex<Vec<String>>>,
    status_states: Arc<Mutex<Vec<LifecycleState>>>,
    lifecycle_events: Arc<Mutex<Vec<LifecycleEvent>>>,
}

fn build_coordinator_with_network_error(
    network_error: Option<&str>,
) -> (TestMocks, AppLifecycleCoordinator) {
    let network_calls = Arc::new(AtomicUsize::new(0));
    let session_events = Arc::new(Mutex::new(Vec::new()));
    let status_states = Arc::new(Mutex::new(Vec::new()));
    let lifecycle_events = Arc::new(Mutex::new(Vec::new()));

    let network_control = Arc::new(MockNetworkControl {
        calls: network_calls.clone(),
        error_message: network_error.map(ToString::to_string),
    });
    let network = Arc::new(StartNetworkAfterUnlock::new(network_control));

    let emitter = Arc::new(MockSessionReadyEmitter {
        events: session_events.clone(),
    }) as Arc<dyn SessionReadyEmitter>;

    let status = Arc::new(MockLifecycleStatus {
        states: status_states.clone(),
    }) as Arc<dyn LifecycleStatusPort>;

    let lifecycle_emitter = Arc::new(MockLifecycleEventEmitter {
        events: lifecycle_events.clone(),
    }) as Arc<dyn LifecycleEventEmitter>;

    let coordinator = AppLifecycleCoordinator::from_deps(AppLifecycleCoordinatorDeps {
        network,
        announcer: None,
        emitter,
        status,
        lifecycle_emitter,
    });

    (
        TestMocks {
            network_calls,
            session_events,
            status_states,
            lifecycle_events,
        },
        coordinator,
    )
}

#[tokio::test]
async fn ensure_ready_succeeds_when_network_already_started() {
    let (mocks, coordinator) =
        build_coordinator_with_network_error(Some("network already started"));

    let result = coordinator.ensure_ready().await;

    assert!(
        result.is_ok(),
        "already started network should be treated as non-fatal"
    );
    assert_eq!(mocks.network_calls.load(Ordering::SeqCst), 1);

    let states = mocks.status_states.lock().await;
    assert_eq!(states.len(), 2);
    assert_eq!(states[0], LifecycleState::Pending);
    assert_eq!(states[1], LifecycleState::Ready);

    let lifecycle_events = mocks.lifecycle_events.lock().await;
    assert_eq!(lifecycle_events.as_slice(), [LifecycleEvent::Ready]);

    let session_events = mocks.session_events.lock().await;
    assert_eq!(session_events.as_slice(), ["ready"]);
}

#[tokio::test]
async fn coordinator_records_status_and_failure_event_on_network_fail() {
    let (mocks, coordinator) = build_coordinator_with_network_error(Some("network mock failure"));

    let result = coordinator.ensure_ready().await;

    // Must return an error
    assert!(result.is_err(), "should fail when network fails");

    // Network was called
    assert_eq!(mocks.network_calls.load(Ordering::SeqCst), 1);

    // Session ready should NOT have been emitted
    assert!(
        mocks.session_events.lock().await.is_empty(),
        "session ready should not be emitted on failure"
    );

    // State transitions: Pending → NetworkFailed
    let states = mocks.status_states.lock().await;
    assert_eq!(states.len(), 2);
    assert_eq!(states[0], LifecycleState::Pending);
    assert_eq!(states[1], LifecycleState::NetworkFailed);

    // Lifecycle event: NetworkFailed
    let events = mocks.lifecycle_events.lock().await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        LifecycleEvent::NetworkFailed(msg) => {
            assert!(
                msg.contains("network mock failure"),
                "expected error message to contain 'network mock failure', got: {}",
                msg
            );
        }
        other => panic!("expected NetworkFailed event, got: {:?}", other),
    }
}
