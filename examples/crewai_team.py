"""
CrewAI + Baud Integration Example

Shows how to create a CrewAI team where agents can pay each other
for completed tasks using the Baud network.

Requirements:
    pip install crewai baud-sdk

Usage:
    export OPENAI_API_KEY=sk-...
    export BAUD_BUYER_SECRET=<hex-secret-of-buyer-agent>
    export BAUD_SELLER_SECRET=<hex-secret-of-seller-agent>
    python crewai_team.py
"""

import os

from crewai import Agent, Crew, Task
from crewai.tools import BaseTool
from pydantic import Field

from baud_sdk import BaudPay


# ─── Baud Payment Tools for CrewAI ──────────────────────────────────────────

buyer_pay = BaudPay.from_secret(
    os.environ["BAUD_BUYER_SECRET"],
    node=os.environ.get("BAUD_NODE", "http://localhost:8080"),
)

seller_pay = BaudPay.from_secret(
    os.environ["BAUD_SELLER_SECRET"],
    node=os.environ.get("BAUD_NODE", "http://localhost:8080"),
)


class CheckBalanceTool(BaseTool):
    name: str = "check_balance"
    description: str = "Check the agent's BAUD balance"
    pay: BaudPay = Field(exclude=True)

    def _run(self) -> str:
        bal = self.pay.balance()
        return f"Balance: {bal:.6f} BAUD (address: {self.pay.address})"


class SendPaymentTool(BaseTool):
    name: str = "send_payment"
    description: str = "Send BAUD payment. Input: 'recipient_address amount memo'"
    pay: BaudPay = Field(exclude=True)

    def _run(self, input_str: str) -> str:
        parts = input_str.strip().split(maxsplit=2)
        if len(parts) < 2:
            return "Error: Provide 'recipient_address amount [memo]'"
        to = parts[0]
        amount = float(parts[1])
        memo = parts[2] if len(parts) > 2 else None
        receipt = self.pay.send(to, amount, memo=memo)
        return f"Sent {amount} BAUD to {to[:16]}... tx={receipt.tx_hash[:16]}..."


class CreateEscrowTool(BaseTool):
    name: str = "create_escrow"
    description: str = "Create escrow. Input: 'recipient amount preimage hours'"
    pay: BaudPay = Field(exclude=True)

    def _run(self, input_str: str) -> str:
        parts = input_str.strip().split()
        if len(parts) < 3:
            return "Error: Provide 'recipient amount preimage [hours]'"
        recipient, amount, preimage = parts[0], float(parts[1]), parts[2]
        hours = float(parts[3]) if len(parts) > 3 else 24.0
        receipt = self.pay.escrow(recipient, amount, preimage, hours)
        return f"Escrow created: {amount} BAUD, id={receipt.escrow_id[:16]}..."


# ─── Agents ──────────────────────────────────────────────────────────────────

buyer_agent = Agent(
    role="Buyer Agent",
    goal="Purchase data analysis services and pay for them using BAUD",
    backstory="You are an AI agent that needs data analyzed. You have BAUD tokens "
              "to pay for services. Always verify the seller's work before paying.",
    tools=[
        CheckBalanceTool(pay=buyer_pay),
        SendPaymentTool(pay=buyer_pay),
        CreateEscrowTool(pay=buyer_pay),
    ],
    verbose=True,
)

seller_agent = Agent(
    role="Seller Agent",
    goal="Provide data analysis services and receive BAUD payments",
    backstory="You are an AI agent that offers data analysis. You perform work "
              "and expect payment in BAUD for completed tasks.",
    tools=[
        CheckBalanceTool(pay=seller_pay),
    ],
    verbose=True,
)

# ─── Tasks ───────────────────────────────────────────────────────────────────

analyze_task = Task(
    description=(
        f"Analyze the following dataset summary and provide key insights: "
        f"'Monthly revenue: $100K, $120K, $95K, $140K, $160K, $180K'. "
        f"Provide a trend analysis and forecast."
    ),
    expected_output="A brief trend analysis with a forecast.",
    agent=seller_agent,
)

pay_task = Task(
    description=(
        f"Review the analysis provided by the Seller Agent. If it looks reasonable, "
        f"pay {seller_pay.address} 0.001 BAUD with memo 'data-analysis-job'. "
        f"Use the send_payment tool."
    ),
    expected_output="Confirmation of payment with transaction hash.",
    agent=buyer_agent,
    context=[analyze_task],
)

# ─── Crew ────────────────────────────────────────────────────────────────────

crew = Crew(
    agents=[seller_agent, buyer_agent],
    tasks=[analyze_task, pay_task],
    verbose=True,
)

if __name__ == "__main__":
    print("\n⚡ Baud + CrewAI Multi-Agent Team")
    print(f"   Buyer:  {buyer_pay.address}")
    print(f"   Seller: {seller_pay.address}\n")

    result = crew.kickoff()
    print(f"\n{'='*50}")
    print(f"Result: {result}")
