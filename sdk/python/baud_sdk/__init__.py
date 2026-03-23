"""
baud-sdk: Python SDK for the Baud M2M Agent Ledger.

Provides key management, transaction construction, and a REST client
for interacting with Baud nodes.

Two signing modes:
  1. Pure Python (recommended) — uses PyNaCl + blake3, no external binary needed
  2. CLI fallback — uses the `baud` CLI binary for signing
"""

__version__ = "0.2.0"

from baud_sdk.client import BaudClient
from baud_sdk.keys import KeyPair
from baud_sdk.constants import QUANTA_PER_BAUD
from baud_sdk.pay import BaudPay, PaymentReceipt
from baud_sdk.signing import NativeKeyPair

__all__ = [
    "BaudClient",
    "KeyPair",
    "NativeKeyPair",
    "QUANTA_PER_BAUD",
    "BaudPay",
    "PaymentReceipt",
]
