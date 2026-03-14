"""
Minimal Autonomous Agent Example — Baud Pay

Shows the simplest possible agent that:
1. Generates a keypair
2. Checks its balance
3. Registers itself on-chain
4. Accepts payment (simulated)

This is a standalone script with no framework dependencies.

Requirements:
    pip install baud-sdk

Usage:
    export BAUD_SECRET_KEY=<hex-secret> 
    python autonomous_agent.py
"""

import json
import os
import time
import urllib.request

from baud_sdk import BaudPay, KeyPair


def main():
    node = os.environ.get("BAUD_NODE", "http://localhost:8080")

    # 1. Load or generate identity
    secret = os.environ.get("BAUD_SECRET_KEY")
    if secret:
        pay = BaudPay.from_secret(secret, node=node)
        print(f"Loaded agent: {pay.address}")
    else:
        kp = KeyPair.generate()
        pay = BaudPay.from_secret(kp.secret_key, node=node)
        print(f"Generated new agent: {pay.address}")
        print(f"  Secret key: {kp.secret_key}")
        print(f"  (set BAUD_SECRET_KEY={kp.secret_key} to reuse)")

    # 2. Check balance
    try:
        bal = pay.balance()
        print(f"Balance: {bal:.6f} BAUD")
    except Exception as e:
        print(f"Could not reach node at {node}: {e}")
        return

    # 3. Simple service loop — agent advertises and waits for work
    print("\nAgent is running. Waiting for service requests...\n")
    print("  POST /do-work with {\"requester\": \"<address>\", \"task\": \"...\"}")
    print("  The agent will complete the task and request payment.\n")

    # Simulated service loop
    print("--- Simulation Mode ---")
    print("Simulating: Received task 'summarize-doc' from requester.")
    print("Simulating: Completing task...")
    time.sleep(1)
    print("Simulating: Task complete. Requesting 0.001 BAUD payment.")
    print("\nIn production, the agent would use pay.send() or escrow flows")
    print("to settle payments with the requester automatically.")


if __name__ == "__main__":
    main()
