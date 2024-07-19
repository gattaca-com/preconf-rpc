use eyre::{bail, Result};
use tokio::{sync::broadcast, task::JoinHandle};

use super::{provider::LookaheadProvider, Lookahead, LookaheadEntry};

enum LookaheadProviderManager {
    Initialized(LookaheadProvider),
    Running(JoinHandle<()>),
}

pub struct LookaheadManager {
    lookahead: Lookahead,
    provider_manager: Option<LookaheadProviderManager>,
}

impl LookaheadManager {
    pub fn new(lookahead: Lookahead, lookahead_provider: LookaheadProvider) -> Self {
        Self {
            lookahead,
            provider_manager: Some(LookaheadProviderManager::Initialized(lookahead_provider)),
        }
    }
    pub fn run_provider(&mut self) -> Result<()> {
        let provider_manager =
            self.provider_manager.take().expect("provider manager should never be None");
        match provider_manager {
            LookaheadProviderManager::Initialized(provider) => {
                let handle = tokio::spawn(async move {
                    provider.run().await;
                });
                self.provider_manager = Some(LookaheadProviderManager::Running(handle));
                Ok(())
            }
            _ => bail!("context provider is already running."),
        }
    }
    pub fn get_next_elected_preconfer(&self) -> Option<LookaheadEntry> {
        self.lookahead.get_next_elected_preconfer()
    }
}
