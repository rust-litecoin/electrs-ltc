use crate::chain::{BlockHash, OutPoint, Transaction, TxIn, TxOut, Txid};
use crate::errors;
use crate::util::{BlockId, IsProvablyUnspendable};

use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct TransactionStatus {
    pub confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_height: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<BlockHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_time: Option<u32>,
}

impl From<Option<BlockId>> for TransactionStatus {
    fn from(blockid: Option<BlockId>) -> TransactionStatus {
        match blockid {
            Some(b) => TransactionStatus {
                confirmed: true,
                block_height: Some(b.height),
                block_hash: Some(b.hash),
                block_time: Some(b.time),
            },
            None => TransactionStatus {
                confirmed: false,
                block_height: None,
                block_hash: None,
                block_time: None,
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct TxInput {
    pub txid: Txid,
    pub vin: u32,
}

pub fn is_coinbase(txin: &TxIn) -> bool {
    txin.previous_output.is_null()
}

pub fn has_prevout(txin: &TxIn) -> bool {
    !txin.previous_output.is_null()
}

pub fn is_spendable(txout: &TxOut) -> bool {
    !txout.script_pubkey.is_provably_unspendable_()
}

/// Extract the previous TxOuts of a Transaction's TxIns
///
/// # Errors
///
/// This function MUST NOT return an error variant when allow_missing is true.
/// If allow_missing is false, it will return an error when any Outpoint is
/// missing from the keys of the txos argument's HashMap.
pub fn extract_tx_prevouts<'a>(
    tx: &Transaction,
    txos: &'a HashMap<OutPoint, TxOut>,
) -> Result<HashMap<u32, &'a TxOut>, errors::Error> {
    tx.input
        .iter()
        .enumerate()
        .filter(|(_, txi)| has_prevout(txi))
        .map(|(index, txi)| {
            Ok((
                index as u32,
                match txos.get(&txi.previous_output) {
                    Some(txo) => txo,
                    None => {
                        return Err(format!("missing outpoint {:?}", txi.previous_output).into());
                    }
                },
            ))
        })
        .collect()
}

pub fn serialize_outpoint<S>(outpoint: &OutPoint, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    use serde::ser::SerializeStruct;
    let mut s = serializer.serialize_struct("OutPoint", 2)?;
    s.serialize_field("txid", &outpoint.txid)?;
    s.serialize_field("vout", &outpoint.vout)?;
    s.end()
}

pub(super) mod sigops {
    use crate::chain::{
        opcodes::all::{OP_CHECKMULTISIG, OP_CHECKMULTISIGVERIFY, OP_CHECKSIG, OP_CHECKSIGVERIFY},
        script::{self, Instruction},
        Transaction, TxOut, Witness,
    };
    use bitcoin::opcodes::Opcode;
    use std::collections::HashMap;

    /// Get sigop count for transaction. prevout_map must have all the prevouts.
    pub fn transaction_sigop_count(
        tx: &Transaction,
        prevout_map: &HashMap<u32, &TxOut>,
    ) -> Result<usize, script::Error> {
        let input_count = tx.input.len();
        let mut prevouts = Vec::with_capacity(input_count);

        let is_coinbase_or_pegin = tx.is_coinbase();

        if !is_coinbase_or_pegin {
            for idx in 0..input_count {
                prevouts.push(
                    *prevout_map
                        .get(&(idx as u32))
                        .ok_or(script::Error::EarlyEndOfScript)?,
                );
            }
        }

        // coinbase tx won't use prevouts so it can be empty.
        get_sigop_cost(tx, &prevouts, true, true)
    }

    fn decode_pushnum(op: &Opcode) -> Option<u8> {
        // 81 = OP_1, 96 = OP_16
        // 81 -> 1, so... 81 - 80 -> 1
        let self_u8 = op.to_u8();
        match self_u8 {
            81..=96 => Some(self_u8 - 80),
            _ => None,
        }
    }

    fn count_sigops(script: &script::Script, accurate: bool) -> usize {
        let mut n = 0;
        let mut pushnum_cache = None;
        for inst in script.instructions() {
            match inst {
                Ok(Instruction::Op(opcode)) => {
                    match opcode {
                        OP_CHECKSIG | OP_CHECKSIGVERIFY => {
                            n += 1;
                        }
                        OP_CHECKMULTISIG | OP_CHECKMULTISIGVERIFY => {
                            match (accurate, pushnum_cache) {
                                (true, Some(pushnum)) => {
                                    // Add the number of pubkeys in the multisig as sigop count
                                    n += usize::from(pushnum);
                                }
                                _ => {
                                    // MAX_PUBKEYS_PER_MULTISIG from Bitcoin Core
                                    // https://github.com/bitcoin/bitcoin/blob/v25.0/src/script/script.h#L29-L30
                                    n += 20;
                                }
                            }
                        }
                        _ => {
                            pushnum_cache = decode_pushnum(&opcode);
                        }
                    }
                }
                // We ignore errors as well as pushdatas
                _ => {
                    pushnum_cache = None;
                }
            }
        }

        n
    }

    /// Get the sigop count for legacy transactions
    fn get_legacy_sigop_count(tx: &Transaction) -> usize {
        let mut n = 0;
        for input in &tx.input {
            n += count_sigops(&input.script_sig, false);
        }
        for output in &tx.output {
            n += count_sigops(&output.script_pubkey, false);
        }
        n
    }

    fn get_p2sh_sigop_count(tx: &Transaction, previous_outputs: &[&TxOut]) -> usize {
        if tx.is_coinbase() {
            return 0;
        }
        let mut n = 0;
        for (input, prevout) in tx.input.iter().zip(previous_outputs.iter()) {
            if prevout.script_pubkey.is_p2sh() {
                if let Some(Ok(script::Instruction::PushBytes(redeem))) =
                    input.script_sig.instructions().last()
                {
                    let script = script::Script::from_bytes(redeem.as_bytes());
                    #[allow(clippy::needless_borrow)]
                    {
                        n += count_sigops(&script, true);
                    }
                }
            }
        }
        n
    }

    fn get_witness_sigop_count(tx: &Transaction, previous_outputs: &[&TxOut]) -> usize {
        let mut n = 0;

        #[inline]
        fn is_push_only(script: &script::Script) -> bool {
            for inst in script.instructions() {
                match inst {
                    Err(_) => return false,
                    Ok(Instruction::Op(_)) => return false,
                    Ok(Instruction::PushBytes(_)) => {}
                }
            }
            true
        }

        #[inline]
        fn last_pushdata(script: &script::Script) -> Option<&[u8]> {
            match script.instructions().last() {
                Some(Ok(Instruction::PushBytes(bytes))) => Some(bytes.as_bytes()),
                _ => None,
            }
        }

        #[inline]
        fn count_with_prevout(
            prevout: &TxOut,
            script_sig: &script::Script,
            witness: &Witness,
        ) -> usize {
            let mut n = 0;

            let script_owned;
            let script: &script::Script = if prevout.script_pubkey.is_witness_program() {
                &prevout.script_pubkey
            } else if prevout.script_pubkey.is_p2sh()
                && is_push_only(script_sig)
                && !script_sig.is_empty()
            {
                script_owned =
                    script::ScriptBuf::from(last_pushdata(script_sig).unwrap().to_vec());
                &script_owned
            } else {
                return 0;
            };

            if script.is_p2wsh() {
                let bytes = script.as_bytes();
                n += sig_ops(witness, bytes[0], &bytes[2..]);
            } else if script.is_p2wpkh() {
                n += 1;
            }
            n
        }

        for (input, prevout) in tx.input.iter().zip(previous_outputs.iter()) {
            n += count_with_prevout(prevout, &input.script_sig, &input.witness);
        }
        n
    }

    /// Get the sigop cost for this transaction.
    fn get_sigop_cost(
        tx: &Transaction,
        previous_outputs: &[&TxOut],
        verify_p2sh: bool,
        verify_witness: bool,
    ) -> Result<usize, script::Error> {
        let mut n_sigop_cost = get_legacy_sigop_count(tx) * 4;
        if tx.is_coinbase() {
            return Ok(n_sigop_cost);
        }
        if tx.input.len() != previous_outputs.len() {
            return Err(script::Error::EarlyEndOfScript);
        }
        if verify_witness && !verify_p2sh {
            return Err(script::Error::EarlyEndOfScript);
        }
        if verify_p2sh {
            n_sigop_cost += get_p2sh_sigop_count(tx, previous_outputs) * 4;
        }
        if verify_witness {
            n_sigop_cost += get_witness_sigop_count(tx, previous_outputs);
        }

        Ok(n_sigop_cost)
    }

    /// Get sigops for the Witness
    ///
    /// witness_version is the raw opcode. OP_0 is 0, OP_1 is 81, etc.
    #[allow(clippy::redundant_closure)]
    fn sig_ops(witness: &Witness, witness_version: u8, witness_program: &[u8]) -> usize {
        let last_witness = witness.last();
        match (witness_version, witness_program.len()) {
            (0, 20) => 1,
            (0, 32) => {
                #[allow(clippy::needless_borrow)]
                last_witness
                    .map(|sl| script::Script::from_bytes(sl))
                    .map(|s| count_sigops(s, true))
                    .unwrap_or_default()
            }
            _ => 0,
        }
    }
}
