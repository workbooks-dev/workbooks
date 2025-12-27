"""Configuration and global state for the Workbooks engine server."""

import logging

from jupyter_client import AsyncKernelManager

# Logging configuration
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Constants for output limiting
MAX_OUTPUT_LINES = 1000
MAX_OUTPUTS_START = 100  # Keep first N outputs
MAX_OUTPUTS_END = 50  # Keep last M outputs
MAX_OUTPUT_MESSAGES = 100  # Max messages for streaming
MAX_TOTAL_TEXT_LENGTH = 10_000_000  # 10MB

# Global state
# Engine managers per workbook path
engines: dict[str, AsyncKernelManager] = {}

# Secret values for output redaction per workbook
secret_values: dict[str, dict[str, str]] = {}
