"""Key management using the baud CLI binary."""

from __future__ import annotations

import json
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


def _find_baud_cli() -> str:
    """Locate the baud CLI binary."""
    path = shutil.which("baud")
    if path:
        return path
    # Try common cargo install location
    cargo_bin = Path.home() / ".cargo" / "bin" / "baud"
    if cargo_bin.exists():
        return str(cargo_bin)
    cargo_bin_exe = cargo_bin.with_suffix(".exe")
    if cargo_bin_exe.exists():
        return str(cargo_bin_exe)
    raise FileNotFoundError(
        "baud CLI binary not found. Install with: cargo install --path crates/baud-cli"
    )


def _run_baud(args: list[str], baud_bin: Optional[str] = None) -> dict:
    """Run a baud CLI command and return parsed JSON output."""
    bin_path = baud_bin or _find_baud_cli()
    result = subprocess.run(
        [bin_path] + args,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError(f"baud CLI error: {result.stderr.strip()}")
    return json.loads(result.stdout)


@dataclass(frozen=True)
class KeyPair:
    """An Ed25519 keypair for a Baud agent.

    Attributes:
        address: Hex-encoded 32-byte public key (agent address).
        secret_key: Hex-encoded 32-byte secret key.
    """

    address: str
    secret_key: str

    @classmethod
    def generate(cls, baud_bin: Optional[str] = None) -> KeyPair:
        """Generate a new random keypair using the baud CLI."""
        data = _run_baud(["keygen"], baud_bin)
        return cls(address=data["address"], secret_key=data["secret_key"])

    @classmethod
    def from_secret(cls, secret_hex: str, baud_bin: Optional[str] = None) -> KeyPair:
        """Restore a keypair from a hex-encoded secret key."""
        bin_path = baud_bin or _find_baud_cli()
        result = subprocess.run(
            [bin_path, "address", "--secret", secret_hex],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            raise ValueError(f"Invalid secret key: {result.stderr.strip()}")
        address = result.stdout.strip()
        return cls(address=address, secret_key=secret_hex)
