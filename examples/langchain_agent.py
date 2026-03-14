"""
LangChain + Baud Integration Example

Shows how to create a LangChain tool that lets an LLM agent pay for
services on the Baud network. The agent can:
  - Check its balance
  - Send payments to other agents
  - Create escrow contracts for trustless exchanges

Requirements:
    pip install langchain langchain-openai baud-sdk

Usage:
    export OPENAI_API_KEY=sk-...
    export BAUD_SECRET_KEY=<your-hex-secret>
    python langchain_agent.py
"""

import os

from langchain.agents import AgentExecutor, create_openai_tools_agent
from langchain.tools import StructuredTool
from langchain_core.prompts import ChatPromptTemplate, MessagesPlaceholder
from langchain_openai import ChatOpenAI
from pydantic import BaseModel, Field

from baud_sdk import BaudPay


# ─── Baud Payment Tools ─────────────────────────────────────────────────────

baud = BaudPay.from_secret(
    os.environ["BAUD_SECRET_KEY"],
    node=os.environ.get("BAUD_NODE", "http://localhost:8080"),
)


class SendPaymentInput(BaseModel):
    to: str = Field(description="Hex-encoded recipient address")
    amount: float = Field(description="Amount in BAUD to send")
    memo: str = Field(default="", description="Optional memo describing the payment")


class EscrowInput(BaseModel):
    recipient: str = Field(description="Hex-encoded recipient address")
    amount: float = Field(description="Amount in BAUD to escrow")
    preimage: str = Field(description="Secret preimage for the hash-lock")
    hours: float = Field(default=24.0, description="Hours until escrow expires")


class AddressInput(BaseModel):
    address: str = Field(description="Hex-encoded address to check")


def check_balance() -> str:
    """Check the agent's own BAUD balance."""
    bal = baud.balance()
    return f"Balance: {bal:.6f} BAUD (address: {baud.address})"


def send_payment(to: str, amount: float, memo: str = "") -> str:
    """Send BAUD to another agent."""
    receipt = baud.send(to, amount, memo=memo or None)
    return f"Sent {amount} BAUD to {to[:16]}... tx_hash={receipt.tx_hash[:16]}..."


def create_escrow(recipient: str, amount: float, preimage: str, hours: float = 24.0) -> str:
    """Create a trustless escrow payment locked by a hash-lock."""
    receipt = baud.escrow(recipient, amount, preimage, hours)
    return f"Escrow created: {amount} BAUD, escrow_id={receipt.escrow_id[:16]}..."


def check_other_balance(address: str) -> str:
    """Check another address's BAUD balance."""
    bal = baud.balance_of(address)
    return f"Balance of {address[:16]}...: {bal:.6f} BAUD"


# ─── Register Tools ─────────────────────────────────────────────────────────

tools = [
    StructuredTool.from_function(check_balance, name="baud_balance",
                                 description="Check the agent's own BAUD balance"),
    StructuredTool.from_function(send_payment, name="baud_send",
                                 description="Send BAUD to another agent",
                                 args_schema=SendPaymentInput),
    StructuredTool.from_function(create_escrow, name="baud_escrow",
                                 description="Create a trustless escrow payment",
                                 args_schema=EscrowInput),
    StructuredTool.from_function(check_other_balance, name="baud_balance_of",
                                 description="Check another address's balance",
                                 args_schema=AddressInput),
]

# ─── Agent Setup ─────────────────────────────────────────────────────────────

prompt = ChatPromptTemplate.from_messages([
    ("system",
     "You are an AI agent with a Baud cryptocurrency wallet. "
     "You can check balances, send payments, and create escrow contracts. "
     "Always confirm payment amounts before sending."),
    MessagesPlaceholder(variable_name="chat_history", optional=True),
    ("human", "{input}"),
    MessagesPlaceholder(variable_name="agent_scratchpad"),
])

llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)
agent = create_openai_tools_agent(llm, tools, prompt)
executor = AgentExecutor(agent=agent, tools=tools, verbose=True)

# ─── Run ─────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    print("\n⚡ Baud + LangChain Agent")
    print(f"   Address: {baud.address}")
    print(f"   Node:    {baud.node}\n")

    while True:
        try:
            user_input = input("You: ").strip()
            if not user_input:
                continue
            if user_input.lower() in ("quit", "exit"):
                break
            result = executor.invoke({"input": user_input})
            print(f"\nAgent: {result['output']}\n")
        except KeyboardInterrupt:
            break
