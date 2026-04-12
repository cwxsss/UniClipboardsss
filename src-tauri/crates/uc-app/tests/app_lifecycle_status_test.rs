use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mockall::mock;

use uc_app::usecases::{
    AppLifecycleCoordinator, AppLifecycleCoordinatorDeps, LifecycleEvent, LifecycleEventEmitter,
    LifecycleState, LifecycleStatusPort, SessionReadyEmitter, StartNetworkAfterUnlock,
};
use uc_core::ports::network_control::NetworkControlPort;

mock! {
    NetworkControl {}

    #[async_trait]
    impl NetworkControlPort for NetworkControl {
        async fn start_network(&self) -> anyhow::Result<()>;
    }
}

mock! {
    SessionReady {}

    #[async_trait]
    impl SessionReadyEmitter for SessionReady {
        async fn emit_ready(&self) -> anyhow::Result<()>;
    }
}

mock! {
    LifecycleStatus {}

    #[async_trait]
    impl LifecycleStatusPort for LifecycleStatus {
        async fn set_state(&self, state: LifecycleState) -> anyhow::Result<()>;
        async fn get_state(&self) -> LifecycleState;
    }
}

mock! {
    LifecycleEventEmitterMock {}

    #[async_trait]
    impl LifecycleEventEmitter for LifecycleEventEmitterMock {
        async fn emit_lifecycle_event(&self, event: LifecycleEvent) -> anyhow::Result<()>;
    }
}

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

    let mut network_control = MockNetworkControl::new();
    let network_calls_clone = network_calls.clone();
    let network_error = network_error.map(ToString::to_string);
    network_control.expect_start_network().returning(move || {
        network_calls_clone.fetch_add(1, Ordering::SeqCst);
        if let Some(message) = &network_error {
            return Err(anyhow::anyhow!(message.clone()));
        }
        Ok(())
    });
    let network = Arc::new(StartNetworkAfterUnlock::new(Arc::new(network_control)));

    let mut emitter = MockSessionReady::new();
    let session_events_clone = session_events.clone();
    emitter.expect_emit_ready().returning(move || {
        session_events_clone
            .lock()
            .unwrap()
            .push("ready".to_string());
        Ok(())
    });
    let emitter = Arc::new(emitter) as Arc<dyn SessionReadyEmitter>;

    let mut status = MockLifecycleStatus::new();
    let status_states_for_set = status_states.clone();
    status.expect_set_state().returning(move |state| {
        status_states_for_set.lock().unwrap().push(state);
        Ok(())
    });
    let status_states_for_get = status_states.clone();
    status.expect_get_state().returning(move || {
        status_states_for_get
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or(LifecycleState::Idle)
    });
    let status = Arc::new(status) as Arc<dyn LifecycleStatusPort>;

    let mut lifecycle_emitter = MockLifecycleEventEmitterMock::new();
    let lifecycle_events_clone = lifecycle_events.clone();
    lifecycle_emitter
        .expect_emit_lifecycle_event()
        .returning(move |event| {
            lifecycle_events_clone.lock().unwrap().push(event);
            Ok(())
        });
    let lifecycle_emitter = Arc::new(lifecycle_emitter) as Arc<dyn LifecycleEventEmitter>;

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

    let states = mocks.status_states.lock().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0], LifecycleState::Pending);
    assert_eq!(states[1], LifecycleState::Ready);

    let lifecycle_events = mocks.lifecycle_events.lock().unwrap();
    assert_eq!(lifecycle_events.as_slice(), [LifecycleEvent::Ready]);

    let session_events = mocks.session_events.lock().unwrap();
    assert_eq!(session_events.as_slice(), ["ready"]);
}

#[tokio::test]
async fn coordinator_records_status_and_failure_event_on_network_fail() {
    let (mocks, coordinator) = build_coordinator_with_network_error(Some("network mock failure"));

    let result = coordinator.ensure_ready().await;

    assert!(result.is_err(), "should fail when network fails");
    assert_eq!(mocks.network_calls.load(Ordering::SeqCst), 1);

    assert!(
        mocks.session_events.lock().unwrap().is_empty(),
        "session ready should not be emitted on failure"
    );

    let states = mocks.status_states.lock().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0], LifecycleState::Pending);
    assert_eq!(states[1], LifecycleState::NetworkFailed);

    let events = mocks.lifecycle_events.lock().unwrap();
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
