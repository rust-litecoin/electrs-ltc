use crate::chain::{script, Network, Script, TxIn, TxOut};
use script::Instruction::PushBytes;

pub struct InnerScripts {
    pub redeem_script: Option<Script>,
    pub witness_script: Option<Script>,
}

pub trait IsProvablyUnspendable {
    fn is_provably_unspendable_(&self) -> bool;
}

impl IsProvablyUnspendable for bitcoin::Script {
    // is_provably_unspendable() is deprecated in rust-bitcoin
    // so we re-implement it here. Copy pasted.
    fn is_provably_unspendable_(&self) -> bool {
        use bitcoin::blockdata::opcodes::{
            Class::{IllegalOp, ReturnOp},
            ClassifyContext, Opcode,
        };

        match self.as_bytes().first() {
            Some(b) => {
                let first = Opcode::from(*b);
                let class = first.classify(ClassifyContext::Legacy);

                class == ReturnOp || class == IllegalOp
            }
            None => false,
        }
    }
}

pub trait SegwitDetection {
    fn segwit_is_p2wpkh(&self) -> bool;
    fn segwit_is_p2wsh(&self) -> bool;
    fn segwit_is_p2tr(&self) -> bool;
}

impl SegwitDetection for bitcoin::Script {
    fn segwit_is_p2wpkh(&self) -> bool {
        self.is_p2wpkh()
    }
    fn segwit_is_p2wsh(&self) -> bool {
        self.is_p2wsh()
    }
    fn segwit_is_p2tr(&self) -> bool {
        self.is_p2tr()
    }
}

impl SegwitDetection for bitcoin::ScriptBuf {
    fn segwit_is_p2wpkh(&self) -> bool {
        self.is_p2wpkh()
    }
    fn segwit_is_p2wsh(&self) -> bool {
        self.is_p2wsh()
    }
    fn segwit_is_p2tr(&self) -> bool {
        self.is_p2tr()
    }
}

pub trait ScriptToAsm: std::fmt::Debug {
    fn to_asm(&self) -> String {
        let asm = format!("{:?}", self);
        asm[7..asm.len() - 1].to_string()
    }
}
impl ScriptToAsm for bitcoin::Script {}
impl ScriptToAsm for bitcoin::ScriptBuf {}

pub trait ScriptToAddr {
    fn to_address_str(&self, network: Network) -> Option<String>;
}
impl ScriptToAddr for bitcoin::Script {
    fn to_address_str(&self, network: Network) -> Option<String> {
        bitcoin::Address::from_script(self, bitcoin::Network::from(network))
            .ok()
            .map(|s| s.to_string())
    }
}

// Returns the witnessScript in the case of p2wsh, or the redeemScript in the case of p2sh.
pub fn get_innerscripts(txin: &TxIn, prevout: &TxOut) -> InnerScripts {
    // Wrapped redeemScript for P2SH spends
    let redeem_script = if prevout.script_pubkey.is_p2sh() {
        if let Some(Ok(PushBytes(redeemscript))) = txin.script_sig.instructions().last() {
            let bytes = redeemscript.as_bytes().to_vec();
            Some(Script::from(bytes))
        } else {
            None
        }
    } else {
        None
    };

    // Wrapped witnessScript for P2WSH or P2SH-P2WSH spends
    let witness_script = if prevout.script_pubkey.segwit_is_p2wsh()
        || prevout.script_pubkey.segwit_is_p2tr()
        || redeem_script.as_ref().is_some_and(|s| s.segwit_is_p2wsh())
    {
        let witness = &txin.witness;

        let wit_to_vec = Vec::from;

        let inner_script_slice = if prevout.script_pubkey.segwit_is_p2tr() {
            // Witness stack is potentially very large
            // so we avoid to_vec() or iter().collect() for performance
            let w_len = witness.len();
            witness
                .last()
                // Get the position of the script spend script (if it exists)
                .map(|last_elem| {
                    // From BIP341:
                    // If there are at least two witness elements, and the first byte of
                    // the last element is 0x50, this last element is called annex a
                    // and is removed from the witness stack.
                    if w_len >= 2 && last_elem.first().filter(|&&v| v == 0x50).is_some() {
                        // account for the extra item removed from the end
                        3
                    } else {
                        // otherwise script is 2nd from last
                        2
                    }
                })
                // Convert to None if not script spend
                // Note: Option doesn't have filter_map() method
                .filter(|&script_pos_from_last| w_len >= script_pos_from_last)
                .and_then(|script_pos_from_last| {
                    // Can't use second_to_last() since it might be 3rd to last
                    #[allow(clippy::iter_nth)]
                    witness.iter().nth(w_len - script_pos_from_last)
                })
        } else {
            witness.last()
        };

        inner_script_slice.map(wit_to_vec).map(Script::from)
    } else {
        None
    };

    InnerScripts {
        redeem_script,
        witness_script,
    }
}
