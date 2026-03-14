"""
baud-pay: High-level payment wrappers for AI agents using Baud.

Provides fire-and-forget payment functions that agents can call with
minimal setup. Handles nonce management, signing, and submission.

Usage:
    from baud_pay import BaudPay

    pay = BaudPay.from_secret("your_hex_secret", node="http://localhost:8080")
    tx_hash = pay.send("recipient_address", 1.5)  # 1.5 BAUD
    receipt = pay.pay_for_service("agent_addr", 0.001, memo="image-gen-job-42")
    escrow_id = pay.escrow("agent_addr", 1.0, preimage="secret123", hours=24)
    pay.release_escrow(escrow_id, preimage="secret123")
"""

from __future__ import annotations

import json
import subprocess
import time
import urllib.request
from dataclasses import dataclass, field
from typing import Optional


QUANTA_PER_BAUD = 10**18


@dataclass
class PaymentReceipt:
    """Result of a payment operation."""
    tx_hash: str
    sender: str
    recipient: str
    amount_baud: float
    memo: Optional[str] = None
    escrow_id: Optional[str] = None
    status: str = "pending"


@dataclass
class BaudPay:
    """High-level payment interface for AI agents."""

    secret_key: str
    address: str
    node: str = "http://localhost:8080"
    baud_bin: str = "baud"

    @classmethod
    def from_secret(cls, secret_hex: str, node: str = "http://localhost:8080",
                    baud_bin: str = "baud") -> BaudPay:
        """Create a BaudPay instance from a hex-encoded secret key."""
        result = subprocess.run(
            [baud_bin, "address", "--secret", secret_hex],
            capture_output=True, text=True, check=True, timeout=10,
        )
        address = result.stdout.strip()
        return cls(secret_key=secret_hex, address=address, node=node, baud_bin=baud_bin)

    def _get_nonce(self) -> int:
        """Fetch current nonce from the node."""
        url = f"{self.node}/account/{self.address}"
        with urllib.request.urlopen(url, timeout=10) as resp:
            data = json.loads(resp.read())
        return data.get("nonce", 0) + 1

    def _submit(self, tx_json: str) -> dict:
        """Submit a signed transaction JSON to the node."""
        url = f"{self.node}/tx"
        req = urllib.request.Request(
            url, data=tx_json.encode(), method="POST",
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read())

    def _run_cli(self, args: list[str]) -> dict:
        """Run a baud CLI command and parse JSON output."""
        result = subprocess.run(
            [self.baud_bin] + args,
            capture_output=True, text=True, check=True, timeout=15,
        )
        return json.loads(result.stdout)

    # ─── Simple Payments ─────────────────────────────────────────────

    def send(self, to: str, amount_baud: float, memo: Optional[str] = None) -> PaymentReceipt:
        """
        Send BAUD to another address.

        Args:
            to: Recipient hex address.
            amount_baud: Amount in BAUD (e.g., 1.5 = 1.5 BAUD).
            memo: Optional text memo.

        Returns:
            PaymentReceipt with tx_hash.
        """
        quanta = int(amount_baud * QUANTA_PER_BAUD)
        nonce = self._get_nonce()

        args = [
            "transfer",
            "--secret", self.secret_key,
            "--to", to,
            "--amount", str(quanta),
            "--nonce", str(nonce),
        ]
        if memo:
            args.extend(["--memo", memo])

        tx_data = self._run_cli(args)
        result = self._submit(json.dumps(tx_data))

        return PaymentReceipt(
            tx_hash=result.get("tx_hash", tx_data.get("tx_hash", "")),
            sender=self.address,
            recipient=to,
            amount_baud=amount_baud,
            memo=memo,
        )

    def pay_for_service(self, agent_address: str, amount_baud: float,
                        memo: str = "") -> PaymentReceipt:
        """
        Pay an agent for a service. Alias for send() with a required memo
        documenting the service being paid for.

        Args:
            agent_address: The service provider's address.
            amount_baud: Payment amount.
            memo: Description of the service (e.g., "image-gen-job-42").

        Returns:
            PaymentReceipt.
        """
        return self.send(agent_address, amount_baud, memo=memo or "service-payment")

    # ─── Escrow Payments ─────────────────────────────────────────────

    def escrow(self, recipient: str, amount_baud: float, preimage: str,
               hours: float = 24) -> PaymentReceipt:
        """
        Create an escrow payment. Funds are locked until the recipient
        reveals the preimage, or the deadline passes (refund).

        Args:
            recipient: Recipient hex address.
            amount_baud: Amount to escrow.
            preimage: Secret string; BLAKE3 hash becomes the hash-lock.
            hours: Time until the escrow expires (default 24h).

        Returns:
            PaymentReceipt with escrow_id.
        """
        quanta = int(amount_baud * QUANTA_PER_BAUD)
        nonce = self._get_nonce()
        deadline = int((time.time() + hours * 3600) * 1000)

        tx_data = self._run_cli([
            "escrow-create",
            "--secret", self.secret_key,
            "--recipient", recipient,
            "--amount", str(quanta),
            "--preimage", preimage,
            "--deadline", str(deadline),
            "--nonce", str(nonce),
        ])
        result = self._submit(json.dumps(tx_data))

        return PaymentReceipt(
            tx_hash=result.get("tx_hash", tx_data.get("tx_hash", "")),
            sender=self.address,
            recipient=recipient,
            amount_baud=amount_baud,
            escrow_id=tx_data.get("escrow_id"),
        )

    def release_escrow(self, escrow_id: str, preimage: str) -> PaymentReceipt:
        """
        Release an escrow by revealing the preimage.

        Args:
            escrow_id: Hex escrow ID.
            preimage: The secret preimage that unlocks the hash-lock.

        Returns:
            PaymentReceipt.
        """
        nonce = self._get_nonce()
        tx_data = self._run_cli([
            "escrow-release",
            "--secret", self.secret_key,
            "--escrow-id", escrow_id,
            "--preimage", preimage,
            "--nonce", str(nonce),
        ])
        result = self._submit(json.dumps(tx_data))

        return PaymentReceipt(
            tx_hash=result.get("tx_hash", tx_data.get("tx_hash", "")),
            sender=self.address,
            recipient="",
            amount_baud=0,
            escrow_id=escrow_id,
        )

    def refund_escrow(self, escrow_id: str) -> PaymentReceipt:
        """
        Refund an expired escrow.

        Args:
            escrow_id: Hex escrow ID.

        Returns:
            PaymentReceipt.
        """
        nonce = self._get_nonce()
        tx_data = self._run_cli([
            "escrow-refund",
            "--secret", self.secret_key,
            "--escrow-id", escrow_id,
            "--nonce", str(nonce),
        ])
        result = self._submit(json.dumps(tx_data))

        return PaymentReceipt(
            tx_hash=result.get("tx_hash", tx_data.get("tx_hash", "")),
            sender=self.address,
            recipient="",
            amount_baud=0,
            escrow_id=escrow_id,
        )

    # ─── Balance Queries ─────────────────────────────────────────────

    def balance(self) -> float:
        """Get own balance in BAUD."""
        url = f"{self.node}/account/{self.address}"
        with urllib.request.urlopen(url, timeout=10) as resp:
            data = json.loads(resp.read())
        return int(data.get("balance", "0")) / QUANTA_PER_BAUD

    def balance_of(self, address: str) -> float:
        """Get another address's balance in BAUD."""
        url = f"{self.node}/account/{address}"
        with urllib.request.urlopen(url, timeout=10) as resp:
            data = json.loads(resp.read())
        return int(data.get("balance", "0")) / QUANTA_PER_BAUD
