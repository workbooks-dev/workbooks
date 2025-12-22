"""FastAPI server setup and configuration."""

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager

from tether_engine.config import logger


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lifespan context manager for startup and shutdown events."""
    logger.info("Tether engine server starting...")
    yield
    logger.info("Tether engine server shutting down...")


# Create FastAPI app
app = FastAPI(
    title="Tether Engine Server",
    description="Jupyter kernel management and AI agent",
    version="0.1.0",
    lifespan=lifespan,
)

# Add CORS middleware for Tauri app communication
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Temporary health endpoint (will be moved to routes/health.py)
@app.get("/health")
async def health_check():
    """Health check endpoint."""
    from tether_engine.config import engines
    return {
        "status": "healthy",
        "active_engines": len(engines)
    }


logger.info("Tether engine server initialized")
