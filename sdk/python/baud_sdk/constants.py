"""Constants for the Baud token model."""

# 1 BAUD = 10^6 quanta (smallest indivisible unit)
QUANTA_PER_BAUD: int = 1_000_000

# Structural limits
MAX_TX_SIZE: int = 65_536
MAX_MEMO_LEN: int = 256
MAX_AGENT_NAME_LEN: int = 64
MAX_ENDPOINT_LEN: int = 256
MAX_CAPABILITIES: int = 16
MAX_CAPABILITY_LEN: int = 64
