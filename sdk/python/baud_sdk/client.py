"""REST client for interacting with a Baud node."""

from __future__ import annotations

from typing import Optional

import urllib.request
import urllib.error
import json

from baud_sdk.keys import KeyPair
from baud_sdk.transactions import (
    SignedTransaction,
    transfer,
    escrow_create,
    escrow_release,
    escrow_refund,
    agent_register,
)


class BaudClient:
    """Client for a Baud node REST API.

    Args:
        node_url: Base URL of the node (e.g., "http://localhost:8080").
        keypair: Optional keypair for signing transactions.
        baud_bin: Optional path to the baud CLI binary.
    """

    def __init__(
        self,
        node_url: str = "http://localhost:8080",
        keypair: Optional[KeyPair] = None,
        baud_bin: Optional[str] = None,
    ):
        self.node_url = node_url.rstrip("/")
        self.keypair = keypair
        self.baud_bin = baud_bin

    def _get(self, path: str) -> dict:
        url = f"{self.node_url}{path}"
        req = urllib.request.Request(url)
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read().decode())
        except urllib.error.HTTPError as e:
            body = e.read().decode() if e.fp else ""
            raise RuntimeError(f"HTTP {e.code}: {body}") from e

    def _post(self, path: str, data: dict) -> dict:
        url = f"{self.node_url}{path}"
        payload = json.dumps(data).encode()
        req = urllib.request.Request(
            url, data=payload, headers={"Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read().decode())
        except urllib.error.HTTPError as e:
            body = e.read().decode() if e.fp else ""
            raise RuntimeError(f"HTTP {e.code}: {body}") from e

    # ── Query endpoints ──────────────────────────────────────────────────

    def status(self) -> dict:
        """Get node status (chain_id, height, state_root, etc.)."""
        return self._get("/v1/status")

    def account(self, address: str) -> dict:
        """Get account balance, nonce, and agent metadata."""
        return self._get(f"/v1/account/{address}")

    def balance(self, address: str) -> int:
        """Get account balance in quanta."""
        data = self.account(address)
        return int(data["balance"])

    def nonce(self, address: str) -> int:
        """Get current account nonce."""
        return self.account(address)["nonce"]

    def get_escrow(self, escrow_id: str) -> dict:
        """Get escrow details by ID."""
        return self._get(f"/v1/escrow/{escrow_id}")

    def get_tx(self, tx_hash: str) -> dict:
        """Look up a transaction by hash (from mempool)."""
        return self._get(f"/v1/tx/{tx_hash}")

    def mempool(self) -> dict:
        """List pending transactions in the mempool."""
        return self._get("/v1/mempool")

    # ── Transaction submission ───────────────────────────────────────────

    def submit(self, tx: SignedTransaction) -> dict:
        """Submit a signed transaction to the node."""
        return self._post("/v1/tx", tx.to_submit_dict())

    def submit_raw(self, tx_dict: dict) -> dict:
        """Submit a raw transaction dict to the node."""
        return self._post("/v1/tx", tx_dict)

    # ── High-level convenience methods ───────────────────────────────────

    def _require_keypair(self) -> KeyPair:
        if self.keypair is None:
            raise ValueError("No keypair set on this client. Pass keypair= to BaudClient().")
        return self.keypair

    def send(
        self,
        to: str,
        amount: int,
        memo: Optional[str] = None,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit a transfer transaction.

        Args:
            to: Hex recipient address.
            amount: Amount in quanta.
            memo: Optional memo string.
            nonce: Account nonce (auto-fetched if omitted).

        Returns:
            Submission response (tx_hash, status).
        """
        kp = self._require_keypair()
        if nonce is None:
            nonce = self.nonce(kp.address)
        tx = transfer(kp.secret_key, to, amount, nonce, memo, self.baud_bin)
        return self.submit(tx)

    def create_escrow(
        self,
        recipient: str,
        amount: int,
        preimage: str,
        deadline: int,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow-create transaction."""
        kp = self._require_keypair()
        if nonce is None:
            nonce = self.nonce(kp.address)
        tx = escrow_create(kp.secret_key, recipient, amount, preimage, deadline, nonce, self.baud_bin)
        return self.submit(tx)

    def release_escrow(
        self,
        escrow_id: str,
        preimage: str,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow-release transaction."""
        kp = self._require_keypair()
        if nonce is None:
            nonce = self.nonce(kp.address)
        tx = escrow_release(kp.secret_key, escrow_id, preimage, nonce, self.baud_bin)
        return self.submit(tx)

    def refund_escrow(
        self,
        escrow_id: str,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow-refund transaction."""
        kp = self._require_keypair()
        if nonce is None:
            nonce = self.nonce(kp.address)
        tx = escrow_refund(kp.secret_key, escrow_id, nonce, self.baud_bin)
        return self.submit(tx)

    def register_agent(
        self,
        name: str,
        endpoint: str,
        capabilities: list[str],
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an agent-register transaction."""
        kp = self._require_keypair()
        if nonce is None:
            nonce = self.nonce(kp.address)
        tx = agent_register(kp.secret_key, name, endpoint, capabilities, nonce, self.baud_bin)
        return self.submit(tx)
