pub use bitcoin::{
    address,
    block::Header as BlockHeader,
    blockdata::{opcodes, script},
    consensus::deserialize,
    hashes, Block, BlockHash, OutPoint, ScriptBuf as Script, Transaction, TxIn, TxOut, Txid,
    Witness,
};

use bitcoin::blockdata::constants::genesis_block;
pub use bitcoin::Network as BNetwork;

// Extension trait for getting txid in a cross-compatible way
pub trait TxidCompat {
    fn get_txid(&self) -> Txid;
}

impl TxidCompat for Transaction {
    fn get_txid(&self) -> Txid {
        self.compute_txid()
    }
}

// Extension trait for getting block size in a cross-compatible way
pub trait BlockSizeCompat {
    fn get_block_size(&self) -> usize;
}

impl BlockSizeCompat for Block {
    fn get_block_size(&self) -> usize {
        self.total_size()
    }
}

pub type Value = u64;

#[derive(Debug, Copy, Clone, PartialEq, Hash, Serialize, Ord, PartialOrd, Eq)]
pub enum Network {
    Bitcoin,
    Testnet,
    Testnet4,
    Regtest,
    Signet,
}

impl Network {
    pub fn magic(self) -> u32 {
        let magic = BNetwork::from(self).magic();
        u32::from_le_bytes(magic.to_bytes())
    }

    pub fn is_regtest(self) -> bool {
        match self {
            Network::Regtest => true,
            _ => false,
        }
    }

    pub fn names() -> Vec<String> {
        return vec![
            "mainnet".to_string(),
            "testnet".to_string(),
            "regtest".to_string(),
            "signet".to_string(),
        ];
    }
}

pub fn genesis_hash(network: Network) -> BlockHash {
    return litecoin_genesis_hash(network);
}

pub fn litecoin_genesis_hash(network: Network) -> bitcoin::BlockHash {
    genesis_block(BNetwork::from(network)).block_hash()
}

impl From<&str> for Network {
    fn from(network_name: &str) -> Self {
        match network_name {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "testnet4" => Network::Testnet4,
            "regtest" => Network::Regtest,
            "signet" => Network::Signet,

            _ => panic!("unsupported Litecoin network: {:?}", network_name),
        }
    }
}

impl From<Network> for BNetwork {
    fn from(network: Network) -> Self {
        match network {
            Network::Bitcoin => BNetwork::Bitcoin,
            // litecoin crate has no legacy Testnet variant; treat both as Testnet4
            Network::Testnet => BNetwork::Testnet4,
            Network::Testnet4 => BNetwork::Testnet4,
            Network::Regtest => BNetwork::Regtest,
            Network::Signet => BNetwork::Signet,
        }
    }
}

impl From<BNetwork> for Network {
    fn from(network: BNetwork) -> Self {
        match network {
            BNetwork::Bitcoin => Network::Bitcoin,
            BNetwork::Testnet4 => Network::Testnet4,
            BNetwork::Regtest => Network::Regtest,
            BNetwork::Signet => Network::Signet,
        }
    }
}
