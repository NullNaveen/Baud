"""
Pure Python transaction signing for Baud.

Replicates the exact bincode serialization + BLAKE3 hash + Ed25519 signature
that the Rust CLI produces. No external binary needed.

Dependencies: PyNaCl (ed25519), blake3
"""

from __future__ import annotations

import struct
import time
from dataclasses import dataclass
from typing import Optional

try:
    import blake3 as _blake3

    def _hash_blake3(data: bytes) -> bytes:
        return _blake3.blake3(data).digest()

except ImportError:
    _blake3 = None

    def _hash_blake3(data: bytes) -> bytes:
        raise ImportError(
            "blake3 package required for pure-Python signing. "
            "Install with: pip install blake3"
        )

try:
    from nacl.signing import SigningKey as _NaClSigningKey

    _HAS_NACL = True
except ImportError:
    _HAS_NACL = False


# ── Bincode helpers (bincode v1 compat) ──────────────────────────────────

def _encode_u32(v: int) -> bytes:
    return struct.pack("<I", v)

def _encode_u64(v: int) -> bytes:
    return struct.pack("<Q", v)

def _encode_u128(v: int) -> bytes:
    return struct.pack("<QQ", v & 0xFFFFFFFFFFFFFFFF, v >> 64)

def _encode_bytes_fixed(data: bytes) -> bytes:
    """Encode a fixed-size byte array (Address, Hash — no length prefix)."""
    return data

def _encode_vec_u8(data: bytes) -> bytes:
    """Encode a Vec<u8> — u64 length prefix + raw bytes."""
    return _encode_u64(len(data)) + data

def _encode_string(s: str) -> bytes:
    """Encode a Rust String — u64 length prefix + UTF-8 bytes."""
    encoded = s.encode("utf-8")
    return _encode_u64(len(encoded)) + encoded

def _encode_option_bytes(data: Optional[bytes]) -> bytes:
    """Encode Option<Vec<u8>>."""
    if data is None:
        return b"\x00"
    return b"\x01" + _encode_vec_u8(data)

def _encode_vec_vec_u8(items: list[bytes]) -> bytes:
    """Encode Vec<Vec<u8>>."""
    result = _encode_u64(len(items))
    for item in items:
        result += _encode_vec_u8(item)
    return result


# ── Payload serialization ────────────────────────────────────────────────

# TxPayload discriminants (enum variant index as u32 LE)
_VARIANT_TRANSFER = 0
_VARIANT_ESCROW_CREATE = 1
_VARIANT_ESCROW_RELEASE = 2
_VARIANT_ESCROW_REFUND = 3
_VARIANT_AGENT_REGISTER = 4
_VARIANT_MILESTONE_CREATE = 5
_VARIANT_MILESTONE_RELEASE = 6
_VARIANT_SET_SPENDING_POLICY = 7


def _encode_transfer(to: bytes, amount: int, memo: Optional[bytes]) -> bytes:
    return (
        _encode_u32(_VARIANT_TRANSFER)
        + _encode_bytes_fixed(to)
        + _encode_u128(amount)
        + _encode_option_bytes(memo)
    )


def _encode_escrow_create(
    recipient: bytes, amount: int, hash_lock: bytes, deadline: int
) -> bytes:
    return (
        _encode_u32(_VARIANT_ESCROW_CREATE)
        + _encode_bytes_fixed(recipient)
        + _encode_u128(amount)
        + _encode_bytes_fixed(hash_lock)
        + _encode_u64(deadline)
    )


def _encode_escrow_release(escrow_id: bytes, preimage: bytes) -> bytes:
    return (
        _encode_u32(_VARIANT_ESCROW_RELEASE)
        + _encode_bytes_fixed(escrow_id)
        + _encode_vec_u8(preimage)
    )


def _encode_escrow_refund(escrow_id: bytes) -> bytes:
    return (
        _encode_u32(_VARIANT_ESCROW_REFUND)
        + _encode_bytes_fixed(escrow_id)
    )


def _encode_agent_register(
    name: bytes, endpoint: bytes, capabilities: list[bytes]
) -> bytes:
    return (
        _encode_u32(_VARIANT_AGENT_REGISTER)
        + _encode_vec_u8(name)
        + _encode_vec_u8(endpoint)
        + _encode_vec_vec_u8(capabilities)
    )


# ── Signable hash computation ───────────────────────────────────────────

def compute_signable_hash(
    sender: bytes,
    nonce: int,
    payload_bytes: bytes,
    timestamp: int,
    chain_id: str,
) -> bytes:
    """
    Compute the BLAKE3 hash of the bincode-serialized signing tuple:
        (sender, nonce, payload, timestamp, chain_id)

    This matches the Rust `signable_hash()` method exactly.
    """
    data = (
        _encode_bytes_fixed(sender)
        + _encode_u64(nonce)
        + payload_bytes
        + _encode_u64(timestamp)
        + _encode_string(chain_id)
    )
    return _hash_blake3(data)


def compute_tx_hash(
    sender: bytes,
    nonce: int,
    payload_bytes: bytes,
    timestamp: int,
    chain_id: str,
    signature: bytes,
) -> bytes:
    """Compute the transaction hash (hash of the full serialized tx)."""
    data = (
        _encode_bytes_fixed(sender)
        + _encode_u64(nonce)
        + payload_bytes
        + _encode_u64(timestamp)
        + _encode_string(chain_id)
        + _encode_bytes_fixed(signature)
    )
    return _hash_blake3(data)


# ── Ed25519 Keypair ─────────────────────────────────────────────────────

@dataclass(frozen=True)
class NativeKeyPair:
    """Pure Python Ed25519 keypair (no baud CLI needed)."""

    secret_key: bytes  # 32-byte seed
    address: bytes     # 32-byte public key

    @classmethod
    def generate(cls) -> NativeKeyPair:
        """Generate a new random keypair."""
        if not _HAS_NACL:
            raise ImportError(
                "PyNaCl required for key generation. Install with: pip install PyNaCl"
            )
        sk = _NaClSigningKey.generate()
        return cls(
            secret_key=bytes(sk),
            address=bytes(sk.verify_key),
        )

    @classmethod
    def from_secret_hex(cls, secret_hex: str) -> NativeKeyPair:
        """Restore keypair from hex-encoded 32-byte secret."""
        if not _HAS_NACL:
            raise ImportError(
                "PyNaCl required. Install with: pip install PyNaCl"
            )
        secret_bytes = bytes.fromhex(secret_hex)
        if len(secret_bytes) != 32:
            raise ValueError(f"Secret key must be 32 bytes, got {len(secret_bytes)}")
        sk = _NaClSigningKey(secret_bytes)
        return cls(
            secret_key=bytes(sk),
            address=bytes(sk.verify_key),
        )

    @property
    def address_hex(self) -> str:
        return self.address.hex()

    @property
    def secret_hex(self) -> str:
        return self.secret_key.hex()

    def sign(self, message: bytes) -> bytes:
        """Sign a message and return 64-byte Ed25519 signature."""
        sk = _NaClSigningKey(self.secret_key)
        signed = sk.sign(message)
        return signed.signature  # first 64 bytes


# ── High-level signing functions ─────────────────────────────────────────

def now_ms() -> int:
    """Current time in milliseconds."""
    return int(time.time() * 1000)


def sign_transfer(
    keypair: NativeKeyPair,
    to_hex: str,
    amount: int,
    nonce: int,
    chain_id: str = "baud-mainnet",
    memo: Optional[str] = None,
    timestamp: Optional[int] = None,
) -> dict:
    """Create and sign a Transfer transaction. Returns JSON-ready dict."""
    ts = timestamp or now_ms()
    to_bytes = bytes.fromhex(to_hex)
    memo_bytes = memo.encode("utf-8") if memo else None

    payload_enc = _encode_transfer(to_bytes, amount, memo_bytes)
    sig_hash = compute_signable_hash(keypair.address, nonce, payload_enc, ts, chain_id)
    signature = keypair.sign(sig_hash)
    tx_hash = compute_tx_hash(keypair.address, nonce, payload_enc, ts, chain_id, signature)

    payload_dict: dict = {
        "type": "Transfer",
        "to": to_hex,
        "amount": amount,
    }
    if memo is not None:
        payload_dict["memo"] = memo

    return {
        "sender": keypair.address_hex,
        "nonce": nonce,
        "payload": payload_dict,
        "timestamp": ts,
        "chain_id": chain_id,
        "signature": signature.hex(),
        "tx_hash": tx_hash.hex(),
    }


def sign_escrow_create(
    keypair: NativeKeyPair,
    recipient_hex: str,
    amount: int,
    preimage: str,
    deadline: int,
    nonce: int,
    chain_id: str = "baud-mainnet",
    timestamp: Optional[int] = None,
) -> dict:
    """Create and sign an EscrowCreate transaction."""
    ts = timestamp or now_ms()
    recipient_bytes = bytes.fromhex(recipient_hex)
    hash_lock = _hash_blake3(preimage.encode("utf-8"))

    payload_enc = _encode_escrow_create(recipient_bytes, amount, hash_lock, deadline)
    sig_hash = compute_signable_hash(keypair.address, nonce, payload_enc, ts, chain_id)
    signature = keypair.sign(sig_hash)
    tx_hash = compute_tx_hash(keypair.address, nonce, payload_enc, ts, chain_id, signature)

    return {
        "sender": keypair.address_hex,
        "nonce": nonce,
        "payload": {
            "type": "EscrowCreate",
            "recipient": recipient_hex,
            "amount": amount,
            "hash_lock": hash_lock.hex(),
            "deadline": deadline,
        },
        "timestamp": ts,
        "chain_id": chain_id,
        "signature": signature.hex(),
        "tx_hash": tx_hash.hex(),
    }


def sign_escrow_release(
    keypair: NativeKeyPair,
    escrow_id_hex: str,
    preimage: str,
    nonce: int,
    chain_id: str = "baud-mainnet",
    timestamp: Optional[int] = None,
) -> dict:
    """Create and sign an EscrowRelease transaction."""
    ts = timestamp or now_ms()
    escrow_id = bytes.fromhex(escrow_id_hex)

    payload_enc = _encode_escrow_release(escrow_id, preimage.encode("utf-8"))
    sig_hash = compute_signable_hash(keypair.address, nonce, payload_enc, ts, chain_id)
    signature = keypair.sign(sig_hash)
    tx_hash = compute_tx_hash(keypair.address, nonce, payload_enc, ts, chain_id, signature)

    return {
        "sender": keypair.address_hex,
        "nonce": nonce,
        "payload": {
            "type": "EscrowRelease",
            "escrow_id": escrow_id_hex,
            "preimage": preimage,
        },
        "timestamp": ts,
        "chain_id": chain_id,
        "signature": signature.hex(),
        "tx_hash": tx_hash.hex(),
    }


def sign_escrow_refund(
    keypair: NativeKeyPair,
    escrow_id_hex: str,
    nonce: int,
    chain_id: str = "baud-mainnet",
    timestamp: Optional[int] = None,
) -> dict:
    """Create and sign an EscrowRefund transaction."""
    ts = timestamp or now_ms()
    escrow_id = bytes.fromhex(escrow_id_hex)

    payload_enc = _encode_escrow_refund(escrow_id)
    sig_hash = compute_signable_hash(keypair.address, nonce, payload_enc, ts, chain_id)
    signature = keypair.sign(sig_hash)
    tx_hash = compute_tx_hash(keypair.address, nonce, payload_enc, ts, chain_id, signature)

    return {
        "sender": keypair.address_hex,
        "nonce": nonce,
        "payload": {
            "type": "EscrowRefund",
            "escrow_id": escrow_id_hex,
        },
        "timestamp": ts,
        "chain_id": chain_id,
        "signature": signature.hex(),
        "tx_hash": tx_hash.hex(),
    }


def sign_agent_register(
    keypair: NativeKeyPair,
    name: str,
    endpoint: str,
    capabilities: list[str],
    nonce: int,
    chain_id: str = "baud-mainnet",
    timestamp: Optional[int] = None,
) -> dict:
    """Create and sign an AgentRegister transaction."""
    ts = timestamp or now_ms()
    name_bytes = name.encode("utf-8")
    endpoint_bytes = endpoint.encode("utf-8")
    caps_bytes = [c.encode("utf-8") for c in capabilities]

    payload_enc = _encode_agent_register(name_bytes, endpoint_bytes, caps_bytes)
    sig_hash = compute_signable_hash(keypair.address, nonce, payload_enc, ts, chain_id)
    signature = keypair.sign(sig_hash)
    tx_hash = compute_tx_hash(keypair.address, nonce, payload_enc, ts, chain_id, signature)

    return {
        "sender": keypair.address_hex,
        "nonce": nonce,
        "payload": {
            "type": "AgentRegister",
            "name": name,
            "endpoint": endpoint,
            "capabilities": capabilities,
        },
        "timestamp": ts,
        "chain_id": chain_id,
        "signature": signature.hex(),
        "tx_hash": tx_hash.hex(),
    }
