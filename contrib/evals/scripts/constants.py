"""Named constants for eval scripts.

No magic numbers.  Every raw value has a name and a comment explaining why.
"""

# ── Server ────────────────────────────────────────────────────────────────────
DEFAULT_SERVER_PORT = 8989       # Default port for llama.cpp HTTP server
HEALTH_CHECK_TIMEOUT = 3          # Seconds to wait for server health check
HEALTH_CHECK_TIMEOUT_GEN = 5      # Seconds for generate-task health check
SERVER_READY_RETRIES = 30         # Max retries waiting for server to start
SERVER_READY_DELAY = 1            # Seconds between server-ready retries

# ── Model ─────────────────────────────────────────────────────────────────────
DEFAULT_GPU_LAYERS = 16           # GPU layers to offload; -1 = all, 0 = CPU only
DEFAULT_CTX_SIZE = 8192           # Context window size in tokens
DEFAULT_MAX_GEN_TOKS = 2048       # Max tokens per generation
DEFAULT_TEMPERATURE = 0.0         # Greedy decoding
DEFAULT_SEED = 1234               # RNG seed for reproducibility
MAX_GEN_TOKS_SERVER = 512         # Cap for generate-task model args

# ── Eval ──────────────────────────────────────────────────────────────────────
DEFAULT_LIMIT = 50                # Default samples per task
RELOAD_INTERVAL = 500             # Reload model every N loglikelihood samples
SCORE_PERCENTILE_FACTOR = 100     # Convert 0-1 score to percentage
