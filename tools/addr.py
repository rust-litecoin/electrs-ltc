#!/usr/bin/env python3
import hashlib
import sys
import argparse

from pycoin.symbols.ltc import network as LitecoinMainnet
from pycoin.symbols.xlt import network as LitecoinTestnet

import client

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--testnet', action='store_true')
    parser.add_argument('address', nargs='+')
    args = parser.parse_args()

    if args.testnet:
        Network = LitecoinTestnet
        port = 60001
    else:
        Network = LitecoinMainnet
        port = 50001

    conn = client.Connection(('localhost', port))
    for addr in args.address:
        script = Network.contract.for_address(addr)
        script_hash = hashlib.sha256(script).digest()[::-1].hex()
        reply = conn.call('blockchain.scripthash.get_balance', script_hash)
        result = reply['result']
        print('{} has {} litoshis'.format(addr, result))


if __name__ == '__main__':
    main()
