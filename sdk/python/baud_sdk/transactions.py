"""Transaction construction using the baud CLI for signing."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from baud_sdk.keys import _run_baud


@dataclass(frozen=True)
class SignedTransaction:
    """A signed Baud transaction ready for submission."""

    sender: str
    nonce: int
    payload: dict
    timestamp: int
    signature: str
    tx_hash: str

    def to_submit_dict(self) -> dict:
        """Convert to the JSON format expected by POST /tx."""
        return {
            "sender": self.sender,
            "nonce": self.nonce,
            "payload": self.payload,
            "timestamp": self.timestamp,
            "signature": self.signature,
        }


def transfer(
    secret_key: str,
    to: str,
    amount: int,
    nonce: int,
    memo: Optional[str] = None,
    baud_bin: Optional[str] = None,
) -> SignedTransaction:
    """Create a signed transfer transaction."""
    args = [
        "transfer",
        "--secret", secret_key,
        "--to", to,
        "--amount", str(amount),
        "--nonce", str(nonce),
    ]
    if memo is not None:
        args.extend(["--memo", memo])
    data = _run_baud(args, baud_bin)
    return SignedTransaction(
        sender=data["sender"],
        nonce=data["nonce"],
        payload=data["payload"],
        timestamp=data["timestamp"],
        signature=data["signature"],
        tx_hash=data["tx_hash"],
    )


def escrow_create(
    secret_key: str,
    recipient: str,
    amount: int,
    preimage: str,
    deadline: int,
    nonce: int,
    baud_bin: Optional[str] = None,
) -> SignedTransaction:
    """Create a signed escrow-create transaction."""
    data = _run_baud([
        "escrow-create",
        "--secret", secret_key,
        "--recipient", recipient,
        "--amount", str(amount),
        "--preimage", preimage,
        "--deadline", str(deadline),
        "--nonce", str(nonce),
    ], baud_bin)
    return SignedTransaction(
        sender=data["sender"],
        nonce=data["nonce"],
        payload=data["payload"],
        timestamp=data["timestamp"],
        signature=data["signature"],
        tx_hash=data["tx_hash"],
    )


def escrow_release(
    secret_key: str,
    escrow_id: str,
    preimage: str,
    nonce: int,
    baud_bin: Optional[str] = None,
) -> SignedTransaction:
    """Create a signed escrow-release transaction."""
    data = _run_baud([
        "escrow-release",
        "--secret", secret_key,
        "--escrow-id", escrow_id,
        "--preimage", preimage,
        "--nonce", str(nonce),
    ], baud_bin)
    return SignedTransaction(
        sender=data["sender"],
        nonce=data["nonce"],
        payload=data["payload"],
        timestamp=data["timestamp"],
        signature=data["signature"],
        tx_hash=data["tx_hash"],
    )


def escrow_refund(
    secret_key: str,
    escrow_id: str,
    nonce: int,
    baud_bin: Optional[str] = None,
) -> SignedTransaction:
    """Create a signed escrow-refund transaction."""
    data = _run_baud([
        "escrow-refund",
        "--secret", secret_key,
        "--escrow-id", escrow_id,
        "--nonce", str(nonce),
    ], baud_bin)
    return SignedTransaction(
        sender=data["sender"],
        nonce=data["nonce"],
        payload=data["payload"],
        timestamp=data["timestamp"],
        signature=data["signature"],
        tx_hash=data["tx_hash"],
    )


def agent_register(
    secret_key: str,
    name: str,
    endpoint: str,
    capabilities: list[str],
    nonce: int,
    baud_bin: Optional[str] = None,
) -> SignedTransaction:
    """Create a signed agent-register transaction."""
    data = _run_baud([
        "agent-register",
        "--secret", secret_key,
        "--name", name,
        "--endpoint", endpoint,
        "--capabilities", ",".join(capabilities),
        "--nonce", str(nonce),
    ], baud_bin)
    return SignedTransaction(
        sender=data["sender"],
        nonce=data["nonce"],
        payload=data["payload"],
        timestamp=data["timestamp"],
        signature=data["signature"],
        tx_hash=data["tx_hash"],
    )
