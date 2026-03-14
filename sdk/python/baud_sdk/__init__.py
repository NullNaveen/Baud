"""
baud-sdk: Python SDK for the Baud M2M Agent Ledger.

Provides key management, transaction construction, and a REST client
for interacting with Baud nodes. Uses the `baud` CLI binary for
offline signing to ensure exact bincode/Ed25519 compatibility.
"""

__version__ = "0.1.0"

from baud_sdk.client import BaudClient
from baud_sdk.keys import KeyPair
from baud_sdk.constants import QUANTA_PER_BAUD

__all__ = ["BaudClient", "KeyPair", "QUANTA_PER_BAUD"]
