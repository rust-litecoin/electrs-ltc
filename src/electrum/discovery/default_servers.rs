use crate::chain::Network;
#[allow(unused_imports)]
use crate::electrum::discovery::{DiscoveryManager, Service};

#[allow(unused_variables)]
pub fn add_default_servers(discovery: &DiscoveryManager, network: Network) {
    // No default Electrum servers seeded for Litecoin.
    // Operators can use --electrum-public-hosts / electrum announce flows
    // to register peers, or populate this list with reachable Litecoin
    // Electrum nodes if the electrum-discovery feature is enabled.
}
