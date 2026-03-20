use std::sync::Arc;

use tokio::sync::mpsc;
use uc_core::ports::{HostEventEmitterPort, RealtimeTopicPort};

use crate::usecases::pairing::PairingDomainEvent;

pub mod pairing_consumer {
    use super::*;

    pub async fn run_pairing_realtime_consumer(
        _realtime: Arc<dyn RealtimeTopicPort>,
        _emitter: Arc<dyn HostEventEmitterPort>,
    ) -> anyhow::Result<()> {
        unimplemented!("pairing realtime consumer is implemented in task 46.1-02-02");
    }
}

pub mod peers_consumer {
    use super::*;

    pub async fn run_peers_realtime_consumer(
        _realtime: Arc<dyn RealtimeTopicPort>,
        _emitter: Arc<dyn HostEventEmitterPort>,
    ) -> anyhow::Result<()> {
        unimplemented!("peers realtime consumer is implemented in task 46.1-02-02");
    }
}

pub mod setup_consumer {
    use super::*;

    pub struct SetupPairingEventHub;

    impl SetupPairingEventHub {
        pub fn new(_buffer: usize) -> Self {
            Self
        }

        pub async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PairingDomainEvent>> {
            unimplemented!("setup pairing hub subscribe is implemented in task 46.1-02-02");
        }

        pub async fn publish(&self, _event: PairingDomainEvent) -> anyhow::Result<()> {
            unimplemented!("setup pairing hub publish is implemented in task 46.1-02-02");
        }
    }

    pub async fn run_setup_realtime_consumer(
        _realtime: Arc<dyn RealtimeTopicPort>,
        _hub: Arc<SetupPairingEventHub>,
    ) -> anyhow::Result<()> {
        unimplemented!("setup realtime consumer is implemented in task 46.1-02-02");
    }
}

pub use pairing_consumer::run_pairing_realtime_consumer;
pub use peers_consumer::run_peers_realtime_consumer;
pub use setup_consumer::{run_setup_realtime_consumer, SetupPairingEventHub};
