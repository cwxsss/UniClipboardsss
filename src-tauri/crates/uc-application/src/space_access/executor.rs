use uc_core::crypto::domain::Passphrase;
use uc_core::ports::space::{
    PersistencePort, ProofPort, SpaceAccessPort, SpaceAccessTransportPort,
};
use uc_core::ports::TimerPort;

pub struct SpaceAccessExecutor<'a> {
    pub space_access: &'a dyn SpaceAccessPort,
    pub passphrase: &'a Passphrase,
    pub transport: &'a mut dyn SpaceAccessTransportPort,
    pub proof: &'a dyn ProofPort,
    pub timer: &'a mut dyn TimerPort,
    pub store: &'a mut dyn PersistencePort,
}
