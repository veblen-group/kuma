use std::{collections::HashMap, ops::DerefMut, sync::Arc};

use color_eyre::eyre::{self, ContextCompat, OptionExt};
use tycho_simulation::protocol::{models::BlockUpdate, state::ProtocolSim};

use crate::pair::{Pair, PairState};

#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub(crate) block_number: u64,
    // TODO: idnogre the modified/unmodified thing and just save the new blocks. protocolsims cant rly be compared without type nonsense
    // The pools that have been modified in the latest block update
    pub(crate) modified_pools_by_pair: HashMap<Pair, Arc<HashMap<String, Box<dyn ProtocolSim>>>>,
    // The pools that have not been modified in the latest block update
    pub(crate) unmodified_pools_by_pair: HashMap<Pair, Arc<HashMap<String, Box<dyn ProtocolSim>>>>,
}

impl Block {
    pub fn apply_update(self, block_update: BlockUpdate) -> Self {
        let Self {
            mut modified_pools_by_pair,
            mut unmodified_pools_by_pair,
            ..
        } = self;

        let BlockUpdate {
            block_number,
            states,
            new_pairs,
            removed_pairs,
        } = block_update;

        // remove pools that are no longer active
        for (id, component) in removed_pairs {
            for (pair, pools) in &mut unmodified_pools_by_pair {
                if pair.subset(&component.tokens) {
                    pools.remove(&id);
                }
            }

            for (pair, pools) in &mut modified_pools_by_pair {
                if pair.subset(&component.tokens) {
                    pools.remove(&id);
                }
            }
        }

        // add new pools
        // let new_pools: HashMap<Pair, HashMap<String, Box<dyn ProtocolSim>>> = new_pairs
        //     .into_iter()
        //     .map(|(id, component)| {
        //         let pair = Pair::new(component.tokens);
        //         let pools = states
        //             .iter()
        //             .filter(|(state_id, _)| *state_id == id)
        //             .collect();
        //         (pair, pools)
        //     })
        //     .collect();

        // modified_pools_by_pair.extend(new_pools);

        // update existing pools

        Self {
            block_number: block_update.block_number,
            modified_pools_by_pair,
            unmodified_pools_by_pair,
        }
    }

    pub fn get_pair_state(&self, pair: Pair) -> eyre::Result<PairState> {
        let modified_pools = self
            .modified_pools_by_pair
            .get(&pair)
            .cloned()
            .ok_or_eyre("failed to get modified pools for pair {pair:?}")?;

        let unmodified_pools = self
            .unmodified_pools_by_pair
            .get(&pair)
            .cloned()
            .ok_or_eyre("failed to get unmodified pools for pair {pair:?}")?;

        Ok(PairState {
            block_number: self.block_number,
            modified_pools,
            unmodified_pools,
        })
    }
}

struct StateMap(HashMap<String, Box<dyn ProtocolSim>>);

impl StateMap {}
