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
    emitted_events: Arc<Mutex<Vec<String>>>,
    status_states: Arc<Mutex<Vec<LifecycleState>>>,
    lifecycle_events: Arc<Mutex<Vec<LifecycleEvent>>>,
}

fn test_fixtures() -> (TestMocks, AppLifecycleCoordinator) {
    let network_calls = Arc::new(AtomicUsize::new(0));
    let emitted_events = Arc::new(Mutex::new(Vec::new()));
    let status_states = Arc::new(Mutex::new(Vec::new()));
    let lifecycle_events = Arc::new(Mutex::new(Vec::new()));

    let mut network_control = MockNetworkControl::new();
    let network_calls_clone = network_calls.clone();
    network_control.expect_start_network().returning(move || {
        network_calls_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let network = Arc::new(StartNetworkAfterUnlock::new(Arc::new(network_control)));

    let mut emitter = MockSessionReady::new();
    let emitted_events_clone = emitted_events.clone();
    emitter.expect_emit_ready().returning(move || {
        emitted_events_clone
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
            emitted_events,
            status_states,
            lifecycle_events,
        },
        coordinator,
    )
}

#[tokio::test]
async fn coordinator_starts_network_and_emits_ready() {
    let (mocks, coordinator) = test_fixtures();

    let result = coordinator.ensure_ready().await;

    assert!(result.is_ok(), "coordinator should return Ok");
    assert_eq!(mocks.network_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mocks.emitted_events.lock().unwrap().len(), 1);

    let states = mocks.status_states.lock().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0], LifecycleState::Pending);
    assert_eq!(states[1], LifecycleState::Ready);

    let events = mocks.lifecycle_events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], LifecycleEvent::Ready);
}

#[tokio::test]
async fn unlock_triggers_ready_and_network_once() {
    let (mocks, coordinator) = test_fixtures();

    coordinator
        .ensure_ready()
        .await
        .expect("unlock path should reach Ready");

    assert_eq!(
        mocks.network_calls.load(Ordering::SeqCst),
        1,
        "unlock should start network exactly once"
    );

    let lifecycle_states = mocks.status_states.lock().unwrap();
    assert_eq!(
        lifecycle_states.as_slice(),
        [LifecycleState::Pending, LifecycleState::Ready],
        "unlock should transition Pending -> Ready only once"
    );

    let lifecycle_events = mocks.lifecycle_events.lock().unwrap();
    assert_eq!(
        lifecycle_events.as_slice(),
        [LifecycleEvent::Ready],
        "unlock should emit exactly one Ready lifecycle event"
    );

    let ready_events = mocks.emitted_events.lock().unwrap();
    assert_eq!(
        ready_events.as_slice(),
        ["ready"],
        "Ready signal emitted once"
    );
}

#[tokio::test]
async fn repeated_unlock_attempts_do_not_restart_network_when_ready() {
    let (mocks, coordinator) = test_fixtures();

    coordinator
        .ensure_ready()
        .await
        .expect("first unlock should transition to Ready");

    let states_after_first = mocks.status_states.lock().unwrap().len();
    assert_eq!(
        states_after_first, 2,
        "initial unlock should write Pending + Ready"
    );

    let second_attempt = coordinator.ensure_ready().await;
    assert!(
        second_attempt.is_ok(),
        "repeated unlock attempts should be idempotent"
    );

    assert_eq!(
        mocks.network_calls.load(Ordering::SeqCst),
        1,
        "ready coordinator must not restart network after Ready"
    );

    let lifecycle_states = mocks.status_states.lock().unwrap();
    assert_eq!(
        lifecycle_states.as_slice(),
        [LifecycleState::Pending, LifecycleState::Ready],
        "no additional state transitions on repeated calls"
    );
}
