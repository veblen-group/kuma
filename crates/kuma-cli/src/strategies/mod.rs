use std::pin::Pin;

use color_eyre::eyre;

use crate::tycho::ChainSpecificAssetState;

mod builder;

struct SingleHopArbitrage {
    // TODO:
    // - token a
    // - token b
    // - slow chain info
    // - fast chain info
    // - slow chain state
    slow_chain_state: Pin<ChainSpecificAssetState>,
    // - fast chain state
    // - arb calculation params
}

impl SingleHopArbitrage {
    pub fn calculate_signal(&self) -> Result<(), eyre::Error> {
        // TODO: Implement signal calculation logic
        // 1. get slow chain state
        //  a. add timer
        //  b. optimal swap caclulation input table
        // 2. get fast chain state
        // 3. calculate optimal swap
        // 4. calculate expected profit
        // 5. create signal object
        Ok(())
    }
}
