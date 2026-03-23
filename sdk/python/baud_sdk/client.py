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
from baud_sdk.signing import (
    NativeKeyPair,
    sign_transfer,
    sign_escrow_create,
    sign_escrow_release,
    sign_escrow_refund,
    sign_agent_register,
)


class BaudClient:
    """Client for a Baud node REST API.

    Args:
        node_url: Base URL of the node (e.g., "http://localhost:8080").
        keypair: Optional KeyPair (CLI-based) for signing transactions.
        native_keypair: Optional NativeKeyPair (pure Python) for signing.
        baud_bin: Optional path to the baud CLI binary (only for CLI mode).
    """

    def __init__(
        self,
        node_url: str = "http://localhost:8080",
        keypair: Optional[KeyPair] = None,
        native_keypair: Optional[NativeKeyPair] = None,
        baud_bin: Optional[str] = None,
        chain_id: str = "baud-mainnet",
    ):
        self.node_url = node_url.rstrip("/")
        self.keypair = keypair
        self.native_keypair = native_keypair
        self.baud_bin = baud_bin
        self.chain_id = chain_id

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

    # ── Pure Python signing (no CLI binary needed) ───────────────────

    @classmethod
    def from_secret(
        cls,
        secret_hex: str,
        node_url: str = "http://localhost:8080",
        chain_id: str = "baud-mainnet",
    ) -> BaudClient:
        """Create a client with pure Python signing (no baud CLI needed).

        Args:
            secret_hex: Hex-encoded 32-byte secret key.
            node_url: Base URL of the node.
            chain_id: Chain ID for transaction signing.
        """
        nkp = NativeKeyPair.from_secret_hex(secret_hex)
        return cls(node_url=node_url, native_keypair=nkp, chain_id=chain_id)

    def _require_native_keypair(self) -> NativeKeyPair:
        if self.native_keypair is None:
            raise ValueError(
                "No native keypair set. Use BaudClient.from_secret() or pass native_keypair=."
            )
        return self.native_keypair

    def native_send(
        self,
        to: str,
        amount: int,
        memo: Optional[str] = None,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit a transfer using pure Python signing."""
        nkp = self._require_native_keypair()
        if nonce is None:
            nonce = self.nonce(nkp.address_hex)
        tx = sign_transfer(nkp, to, amount, nonce, self.chain_id, memo)
        return self.submit_raw(tx)

    def native_create_escrow(
        self,
        recipient: str,
        amount: int,
        preimage: str,
        deadline: int,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow using pure Python signing."""
        nkp = self._require_native_keypair()
        if nonce is None:
            nonce = self.nonce(nkp.address_hex)
        tx = sign_escrow_create(nkp, recipient, amount, preimage, deadline, nonce, self.chain_id)
        return self.submit_raw(tx)

    def native_release_escrow(
        self,
        escrow_id: str,
        preimage: str,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow release using pure Python signing."""
        nkp = self._require_native_keypair()
        if nonce is None:
            nonce = self.nonce(nkp.address_hex)
        tx = sign_escrow_release(nkp, escrow_id, preimage, nonce, self.chain_id)
        return self.submit_raw(tx)

    def native_refund_escrow(
        self,
        escrow_id: str,
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an escrow refund using pure Python signing."""
        nkp = self._require_native_keypair()
        if nonce is None:
            nonce = self.nonce(nkp.address_hex)
        tx = sign_escrow_refund(nkp, escrow_id, nonce, self.chain_id)
        return self.submit_raw(tx)

    def native_register_agent(
        self,
        name: str,
        endpoint: str,
        capabilities: list[str],
        nonce: Optional[int] = None,
    ) -> dict:
        """Sign and submit an agent registration using pure Python signing."""
        nkp = self._require_native_keypair()
        if nonce is None:
            nonce = self.nonce(nkp.address_hex)
        tx = sign_agent_register(nkp, name, endpoint, capabilities, nonce, self.chain_id)
        return self.submit_raw(tx)

    @property
    def address(self) -> str:
        """Get the address of the active keypair."""
        if self.native_keypair:
            return self.native_keypair.address_hex
        if self.keypair:
            return self.keypair.address
        raise ValueError("No keypair configured")
