#!/usr/bin/env python3
"""
FastAPI-based Jupyter kernel manager for Tether.
Manages kernel lifecycle and code execution.
"""
import sys
import asyncio
from contextlib import asynccontextmanager
from typing import Dict, List, Any
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from jupyter_client import AsyncKernelManager
import uvicorn

# Store kernel managers per notebook path
kernels: Dict[str, AsyncKernelManager] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lifespan context manager for startup and shutdown events."""
    # Startup
    yield
    # Shutdown: Clean up all kernels
    for notebook_path, km in list(kernels.items()):
        try:
            await km.shutdown_kernel()
        except Exception:
            pass
    kernels.clear()


app = FastAPI(title="Tether Kernel Server", lifespan=lifespan)

# Enable CORS for Tauri
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


class StartKernelRequest(BaseModel):
    notebook_path: str
    project_root: str
    kernel_name: str = "python3"


class ExecuteRequest(BaseModel):
    notebook_path: str
    code: str


class Output(BaseModel):
    output_type: str
    name: str | None = None
    text: str | None = None
    data: Dict[str, Any] | None = None
    execution_count: int | None = None
    ename: str | None = None
    evalue: str | None = None
    traceback: List[str] | None = None


class ExecuteResponse(BaseModel):
    success: bool
    outputs: List[Output]


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {"status": "healthy", "active_kernels": len(kernels)}


@app.post("/kernel/start")
async def start_kernel(request: StartKernelRequest):
    """Start a new Jupyter kernel for a notebook."""
    notebook_path = request.notebook_path
    project_root = request.project_root
    kernel_name = request.kernel_name

    print(f"Starting kernel for notebook: {notebook_path}")
    print(f"Project root: {project_root}")
    print(f"Kernel name: {kernel_name}")

    if notebook_path in kernels:
        return {"status": "already_running", "notebook_path": notebook_path}

    try:
        # Point to the project's Python executable in its venv
        import os
        import platform
        import traceback

        if platform.system() == "Windows":
            venv_python = os.path.join(project_root, ".venv", "Scripts", "python.exe")
        else:
            venv_python = os.path.join(project_root, ".venv", "bin", "python")

        print(f"Looking for Python at: {venv_python}")
        print(f"Python exists: {os.path.exists(venv_python)}")

        if not os.path.exists(venv_python):
            error_msg = f"Project Python not found at {venv_python}. Ensure the project has a .venv"
            print(f"ERROR: {error_msg}")
            raise HTTPException(status_code=400, detail=error_msg)

        # Check if ipykernel is installed
        import subprocess
        check_result = subprocess.run(
            [venv_python, "-c", "import ipykernel"],
            capture_output=True,
            text=True
        )
        if check_result.returncode != 0:
            error_msg = f"ipykernel not installed in venv. Error: {check_result.stderr}"
            print(f"ERROR: {error_msg}")
            raise HTTPException(status_code=500, detail=error_msg)

        print("ipykernel is installed")

        # Install kernel spec in the project's venv if not already installed
        import subprocess
        import json

        print("Installing/checking kernel spec in project venv...")
        kernel_spec_name = f"tether_{os.path.basename(project_root)}"
        print(f"DEBUG: kernel_spec_name = '{kernel_spec_name}' (type: {type(kernel_spec_name)})")

        # Use ipython kernel install to create a kernel spec pointing to this venv
        install_result = subprocess.run(
            [venv_python, "-m", "ipykernel", "install", "--user", "--name", kernel_spec_name, "--display-name", f"Tether ({os.path.basename(project_root)})"],
            capture_output=True,
            text=True
        )

        if install_result.returncode != 0:
            print(f"Warning: Could not install kernel spec: {install_result.stderr}")
            # Continue anyway, it might already be installed
        else:
            print(f"Kernel spec '{kernel_spec_name}' installed successfully")
            print(f"Install output: {install_result.stdout}")

        print(f"Creating kernel manager with kernel_name='{kernel_spec_name}'...")
        km = AsyncKernelManager(kernel_name=kernel_spec_name)

        print("Starting kernel process...")
        await km.start_kernel(cwd=project_root)

        # Wait for kernel to be ready
        print("Getting kernel client...")
        kc = km.client()
        kc.start_channels()

        print("Waiting for kernel to be ready...")
        try:
            await kc.wait_for_ready(timeout=30)
            print("Kernel is ready!")
        except RuntimeError as e:
            print(f"Kernel failed to become ready: {e}")
            await km.shutdown_kernel()
            raise HTTPException(status_code=500, detail=f"Kernel failed to start: {str(e)}")

        kernels[notebook_path] = km
        print(f"Kernel started successfully for {notebook_path}")

        return {
            "status": "started",
            "notebook_path": notebook_path,
            "kernel_name": kernel_name,
        }
    except HTTPException:
        raise
    except Exception as e:
        import traceback
        error_detail = f"{str(e)}\n\nTraceback:\n{traceback.format_exc()}"
        print(f"ERROR starting kernel: {error_detail}")
        raise HTTPException(status_code=500, detail=error_detail)


@app.post("/kernel/execute", response_model=ExecuteResponse)
async def execute_code(request: ExecuteRequest):
    """Execute code in a notebook's kernel."""
    notebook_path = request.notebook_path
    code = request.code

    km = kernels.get(notebook_path)
    if not km:
        raise HTTPException(status_code=404, detail="No kernel found for this notebook")

    try:
        kc = km.client()

        # Execute code
        msg_id = kc.execute(code, store_history=True, silent=False)

        # Collect outputs
        outputs: List[Output] = []
        has_error = False

        while True:
            try:
                msg = await asyncio.wait_for(kc.get_iopub_msg(), timeout=30.0)

                # Only process messages for our execution
                if msg.get("parent_header", {}).get("msg_id") != msg_id:
                    continue

                msg_type = msg["msg_type"]
                content = msg["content"]

                if msg_type == "stream":
                    outputs.append(
                        Output(
                            output_type="stream",
                            name=content["name"],
                            text=content["text"],
                        )
                    )

                elif msg_type == "execute_result":
                    outputs.append(
                        Output(
                            output_type="execute_result",
                            data=content["data"],
                            execution_count=content["execution_count"],
                        )
                    )

                elif msg_type == "display_data":
                    outputs.append(
                        Output(
                            output_type="display_data",
                            data=content["data"],
                        )
                    )

                elif msg_type == "error":
                    has_error = True
                    outputs.append(
                        Output(
                            output_type="error",
                            ename=content["ename"],
                            evalue=content["evalue"],
                            traceback=content["traceback"],
                        )
                    )

                elif msg_type == "status" and content["execution_state"] == "idle":
                    break

            except asyncio.TimeoutError:
                break

        return ExecuteResponse(success=not has_error, outputs=outputs)

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/kernel/stop")
async def stop_kernel(notebook_path: str):
    """Stop a notebook's kernel."""
    km = kernels.pop(notebook_path, None)
    if km:
        try:
            await km.shutdown_kernel()
            return {"status": "stopped", "notebook_path": notebook_path}
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e))

    return {"status": "not_found", "notebook_path": notebook_path}


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="info")
