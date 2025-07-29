use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tracing::{debug, instrument, trace};
use tycho_common::simulation::protocol_sim::ProtocolSim;
use tycho_simulation::protocol::models::{ProtocolComponent, Update};

use super::pair::{Pair, PairState};
use crate::state;

#[derive(Clone, Debug)]
pub struct Block {
    pub height: u64,
    /// The current states
    pub states: HashMap<state::PoolId, Arc<dyn ProtocolSim>>,
    /// The pools that have been modified in the latest block update
    pub modified_pools: Arc<HashSet<state::PoolId>>,
    /// The pools that have not been modified in the latest block update
    pub unmodified_pools: Arc<HashSet<state::PoolId>>,
    pub metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}

impl Block {
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
            modified_pools: Arc::new(metadata.keys().cloned().collect()),
            unmodified_pools: Arc::new(HashSet::new()),
            metadata,
        }
    }

    /// Consume this `Block` and return a new snapshot with `block_update` applied.
    ///
    /// - Evicts `removed_pairs` from `states`, `metadata`, `modified_pools` and `unmodified_pools`
    /// - Inserts `new_pairs` to `states`, `metadata`, and `modified_pools`.
    /// - Replaces states for `updated_states`, moves their IDs into `modified_pools`.
    ///   - Note: Metadata (i.e. `ProtocolComponent`) are immutable data so they are not modified.
    ///
    /// The returned `Block` has `block_number = block_update.block_number`.
    ///
    /// Any `PairState` derived from the old `Block` keeps its own `Arc` handles:
    /// - `modified_pools` and `unmodified_pools` are cloned, leaving old snapshots unchanged
    /// - old snapshots keep their shared references to states and metadata, so those aren't dropped.
    ///
    /// New `PairState`s built after this call will reflect the updated contents.
    ///
    /// # Panics
    /// - if `removed_pairs` contains an ID not present in the original maps
    /// - if `new_pairs` refers to a state missing from `updated_states`
    #[instrument(skip_all)]
    pub fn apply_update(self, block_update: Update) -> Self {
        let Self {
            modified_pools,
            unmodified_pools,
            mut states,
            mut metadata,
            ..
        } = self;

        let Update {
            block_number_or_timestamp: height,
            states: mut updated_states,
            new_pairs,
            removed_pairs,
        } = block_update;

        let mut modified_pools = modified_pools.as_ref().clone();
        let mut unmodified_pools = unmodified_pools.as_ref().clone();

        // remove pools that are no longer active
        for (id, _) in removed_pairs {
            // update block state map
            let id = state::PoolId(id);
            let _removed_state = states
                .remove(&id)
                .expect("BlockUpdate.removed_pairs should only contain existing pairs");

            // update metadata map
            let _removed_metadata = metadata
                .remove(&id)
                .expect("BlockUpdate.removed_pairs should only contain existing pairs");

            // update modified/unmodified maps
            if modified_pools.remove(&id) {
                trace!(block.number = %height, pair.id = %id, "Removed pair from modified pairs");
            } else if unmodified_pools.remove(&id) {
                trace!(block.number = %height, pair.id = %id, "Removed pair from unmodified pairs");
            } else {
                // TODO: maybe fail more gracefully from bad block updates, altho this should never happen if tycho_simulation is well written
                panic!("BlockUpdate.removed_pairs should only contain existing pairs");
            }

            debug!(block.number = %height, pair.id = %id, "Removed pair");
        }

        // add new pools
        for (id, new_pair) in new_pairs {
            // update block state map
            let pair_state = updated_states
                .remove(&id)
                .expect("BlockUpdate.state should contain every new pool's state");
            let pair_id = state::PoolId(id);
            states.insert(pair_id.clone(), Arc::from(pair_state));

            // update metadata map
            if let Some(metadata) = metadata.insert(pair_id.clone(), Arc::new(new_pair)) {
                debug!(block.number = %height, pair.id = %metadata.id, "Updated metadata for pair");
            }

            // Update modified pairs
            modified_pools.insert(pair_id.clone());

            debug!(block.number = %height, pair.id = %pair_id, "Added pair to ");
        }

        // update existing pools
        for (id, state) in updated_states {
            // update block state map
            let pair_id = state::PoolId::from(id);
            states.insert(pair_id.clone(), Arc::from(state));

            // add to modified pairs
            modified_pools.insert(pair_id.clone());
            if unmodified_pools.remove(&pair_id) {
                trace!(block.number = %height, pair.id = %pair_id, "Updated unmodified pair");
            }

            debug!(block.number = %height, pair.id = %pair_id, "Updated pair state");
        }

        Self {
            height: block_update.block_number_or_timestamp,
            modified_pools: Arc::new(modified_pools),
            unmodified_pools: Arc::new(unmodified_pools),
            metadata,
            states,
        }
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
            modified_pools: Arc::clone(&self.modified_pools),
            unmodified_pools: Arc::clone(&self.unmodified_pools),
            states: pair_states,
            metadata: pair_metadata,
        }
    }
}
