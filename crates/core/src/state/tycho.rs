//! DEX protocol state snapshot and update management.
//!
//! `BlockSim` represents a snapshot of all DEX pool states at a specific block height.
//! Supports incremental updates from Tycho's protocol stream and pair-specific state extraction.
//!
//! Tycho provides a full pool-state snapshot on every block (`Update.states`), so
//! `apply_update` replaces the state map wholesale and only patches `metadata` for
//! added/removed pools.

use std::{
    collections::HashMap,
    sync::Arc,
};

use color_eyre::eyre::{self, OptionExt};
use tracing::{instrument, trace};
use tycho_simulation::{
    protocol::models::{ProtocolComponent, Update},
    tycho_core::simulation::protocol_sim::ProtocolSim,
};

use super::pair::{Pair, PairState};
use crate::state;

#[derive(Clone, Debug)]
pub struct BlockSim {
    pub height: u64,
    pub states: HashMap<state::PoolId, Arc<dyn ProtocolSim>>,
    pub metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}

impl BlockSim {
    pub fn new(block_update: Update) -> Self {
        let Update {
            block_number_or_timestamp,
            states,
            new_pairs,
            ..
        } = block_update;

        let states = states
            .into_iter()
            .map(|(id, state)| (state::PoolId::from(id), Arc::from(state)))
            .collect();

        let metadata: HashMap<state::PoolId, Arc<ProtocolComponent>> = new_pairs
            .into_iter()
            .map(|(id, metadata)| (state::PoolId::from(id), Arc::from(metadata)))
            .collect();

        Self {
            height: block_number_or_timestamp,
            states,
            metadata,
        }
    }

    /// Consume this `BlockSim` and return a new snapshot with `block_update` applied.
    ///
    /// - `Update.states` replaces the entire state map (Tycho sends a full snapshot each block).
    /// - `new_pairs` are inserted into `metadata`.
    /// - `removed_pairs` are evicted from `metadata`.
    /// - Pool metadata is treated as immutable unless a pool is removed and re-added.
    ///
    /// Any `PairState` derived from the old `BlockSim` keeps its own `Arc` handles, so
    /// old snapshots are unaffected by this call.
    #[instrument(skip_all)]
    pub fn apply_update(self, block_update: Update) -> eyre::Result<Self> {
        let Self { mut metadata, .. } = self;

        let Update {
            block_number_or_timestamp: height,
            states,
            new_pairs: new_pools,
            removed_pairs: removed_pools,
            ..
        } = block_update;

        // Remove metadata for pools that are no longer active.
        for (id, _) in removed_pools {
            metadata
                .remove(&state::PoolId(id))
                .ok_or_eyre("BlockUpdate.removed_pools should only contain existing pairs")?;
        }

        // Add metadata for new pools.
        for (id, new_pair) in new_pools {
            if let Some(prev) = metadata.insert(state::PoolId(id), Arc::new(new_pair)) {
                trace!(block.number = %height, pair.id = %prev.id, "Updated metadata for pair");
            }
        }

        // Replace the full state map — Tycho sends a complete snapshot every block.
        let new_states = states
            .into_iter()
            .map(|(id, state)| (state::PoolId(id), Arc::from(state)))
            .collect();

        Ok(Self {
            height,
            states: new_states,
            metadata,
        })
    }

    pub fn get_pair_state(&self, pair: &Pair) -> PairState {
        let pair_metadata: HashMap<state::PoolId, Arc<ProtocolComponent>> = self
            .metadata
            .iter()
            .filter(|(_id, metadata)| pair.in_token_vec(&metadata.tokens))
            .map(|(id, metadata)| (id.clone(), Arc::clone(metadata)))
            .collect();

        let pair_states = self
            .states
            .iter()
            .filter(|(id, _)| pair_metadata.contains_key(id))
            .map(|(id, state)| (id.clone(), Arc::clone(state)))
            .collect();

        PairState {
            block_height: self.height,
            states: pair_states,
            metadata: pair_metadata,
        }
    }
}
