#!/usr/bin/env python3
"""
FastAPI-based Jupyter engine manager for Workbooks.
Manages engine lifecycle and code execution.
"""

import asyncio
import json
import logging
import os
import re
import sys
from contextlib import asynccontextmanager
from typing import Any

import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from jupyter_client import AsyncKernelManager
from pydantic import BaseModel

# Configure logging with timestamps
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger(__name__)

# Store engine managers per workbook path
engines: dict[str, AsyncKernelManager] = {}

# Store secret values per workbook for output redaction
secret_values: dict[str, dict[str, str]] = {}  # workbook_path -> {key: value}


def normalize_path(path: str) -> str:
    """
    Normalize a file path to ensure consistent dictionary keys.
    Converts to absolute path and normalizes separators/redundant parts.
    """
    return os.path.abspath(os.path.normpath(path))


def slugify_kernel_name(name: str) -> str:
    """
    Slugify a name to be a valid kernel spec name.
    Jupyter kernel names must be alphanumeric with dashes/underscores only.
    """
    # Convert to lowercase and replace spaces/special chars with underscores
    slug = re.sub(r"[^a-z0-9]+", "_", name.lower())
    # Remove leading/trailing underscores
    slug = slug.strip("_")
    # Ensure we have a valid name
    if not slug:
        slug = "project"
    return slug


def mask_secret_value(value: str) -> str:
    """
    Mask a secret value for logging.
    Shows first 4 and last 4 characters for values longer than 10 chars.
    """
    if not value:
        return "***"
    if len(value) > 10:
        return value[:4] + "..." + value[-4:]
    else:
        return "***"


def preprocess_code(code: str) -> str:
    """
    Preprocess code before execution to fix common issues.

    Currently handles:
    - Converting `!cd` to `%cd` so directory changes persist across cells
    """
    if not code:
        return code

    lines = code.split("\n")
    processed_lines = []

    for line in lines:
        # Convert !cd to %cd so directory changes persist
        # Match: !cd followed by space or end of line
        # This preserves other shell commands like !ls, !pwd, etc.
        if re.match(r"^\s*!cd(\s|$)", line):
            # Replace !cd with %cd
            processed_line = re.sub(r"^(\s*)!cd(\s|$)", r"\1%cd\2", line)
            processed_lines.append(processed_line)
        else:
            processed_lines.append(line)

    return "\n".join(processed_lines)


def contains_secret(text: str, secrets: dict[str, str]) -> bool:
    """
    Check if text contains any secret values.
    Returns True if any secret value is found in the text.
    """
    if not text or not secrets:
        return False

    for secret_value in secrets.values():
        if secret_value and secret_value in text:
            return True

    return False


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lifespan context manager for startup and shutdown events."""
    # Startup
    yield
    # Shutdown: Clean up all engines
    for workbook_path, km in list(engines.items()):
        try:
            await km.shutdown_kernel()
        except Exception:
            pass
    engines.clear()
    secret_values.clear()


app = FastAPI(title="Workbooks Engine Server", lifespan=lifespan)

# Enable CORS for Tauri
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


class StartEngineRequest(BaseModel):
    workbook_path: str
    project_root: str
    venv_path: str
    engine_name: str = "python3"
    env_vars: dict[str, str] | None = None  # Optional environment variables to inject
    secrets: dict[str, str] | None = None  # Secret key-value pairs for output redaction


class ExecuteRequest(BaseModel):
    workbook_path: str
    code: str


class Output(BaseModel):
    output_type: str
    name: str | None = None
    text: str | None = None
    data: dict[str, Any] | None = None
    execution_count: int | None = None
    ename: str | None = None
    evalue: str | None = None
    traceback: list[str] | None = None
    metadata: dict[str, Any] | None = None  # For contains_secrets and other flags


class ExecuteResponse(BaseModel):
    success: bool
    outputs: list[Output]
    execution_count: int | None = None


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {"status": "healthy", "active_engines": len(engines)}


@app.post("/cleanup/orphaned_kernels")
async def cleanup_orphaned_kernels():
    """
    Kill all orphaned Jupyter kernel processes.
    This finds and kills ipykernel processes that are not currently managed by this server.
    """
    import subprocess
    import signal
    import platform

    killed_count = 0
    errors = []

    try:
        # Get list of all ipykernel processes
        if platform.system() == "Windows":
            # Windows: Use tasklist to find python.exe running ipykernel
            result = subprocess.run(
                ["tasklist", "/FI", "IMAGENAME eq python.exe", "/FO", "CSV"],
                capture_output=True,
                text=True
            )
            # Parse CSV output and check command line for ipykernel
            # This is a simplified approach - production code would need more robust parsing
            pass  # TODO: Implement Windows kernel cleanup
        else:
            # Unix-like systems (macOS, Linux)
            # Find all python processes running ipykernel_launcher
            result = subprocess.run(
                ["ps", "aux"],
                capture_output=True,
                text=True
            )

            if result.returncode == 0:
                lines = result.stdout.split("\n")
                for line in lines:
                    # Look for ipykernel_launcher processes
                    if "ipykernel_launcher" in line and "python" in line.lower():
                        # Extract PID (second column in ps aux output)
                        parts = line.split()
                        if len(parts) >= 2:
                            try:
                                pid = int(parts[1])

                                # Check if this PID is managed by our server
                                is_managed = False
                                for km in engines.values():
                                    if hasattr(km, 'kernel') and km.kernel and hasattr(km.kernel, 'pid'):
                                        if km.kernel.pid == pid:
                                            is_managed = True
                                            break

                                # If not managed, kill it
                                if not is_managed:
                                    print(f"Killing orphaned kernel process: PID {pid}")
                                    try:
                                        os.kill(pid, signal.SIGKILL)
                                        killed_count += 1
                                    except ProcessLookupError:
                                        print(f"Process {pid} already dead")
                                    except Exception as kill_err:
                                        error_msg = f"Failed to kill PID {pid}: {kill_err}"
                                        print(error_msg)
                                        errors.append(error_msg)

                            except (ValueError, IndexError) as e:
                                print(f"Error parsing line: {e}")
                                continue

        return {
            "status": "success",
            "killed_count": killed_count,
            "errors": errors if errors else None
        }

    except Exception as e:
        return {
            "status": "error",
            "message": str(e),
            "killed_count": killed_count
        }


@app.post("/engine/start")
async def start_engine(request: StartEngineRequest):
    """Start a new Jupyter engine for a workbook."""
    # Normalize paths to ensure consistent dictionary keys
    workbook_path = normalize_path(request.workbook_path)
    project_root = request.project_root
    venv_path = request.venv_path
    engine_name = request.engine_name

    print(f"Starting engine for workbook: {workbook_path}")
    print(f"Project root: {project_root}")
    print(f"Venv path: {venv_path}")
    print(f"Engine name: {engine_name}")

    if workbook_path in engines:
        print(f"Engine already running for {workbook_path}")
        return {"status": "already_running", "workbook_path": workbook_path}

    try:
        # Point to the project's Python executable in its centralized venv
        import os
        import platform
        import traceback

        if platform.system() == "Windows":
            venv_python = os.path.join(venv_path, "Scripts", "python.exe")
        else:
            venv_python = os.path.join(venv_path, "bin", "python")

        print(f"Looking for Python at: {venv_python}")
        print(f"Python exists: {os.path.exists(venv_python)}")

        if not os.path.exists(venv_python):
            error_msg = f"Project Python not found at {venv_python}. Ensure the project's virtual environment is initialized."
            print(f"ERROR: {error_msg}")
            raise HTTPException(status_code=400, detail=error_msg)

        # Check if ipykernel is installed
        import subprocess

        check_result = subprocess.run(
            [venv_python, "-c", "import ipykernel"], capture_output=True, text=True
        )
        if check_result.returncode != 0:
            error_msg = f"ipykernel not installed in venv. Error: {check_result.stderr}"
            print(f"ERROR: {error_msg}")
            raise HTTPException(status_code=500, detail=error_msg)

        print("ipykernel is installed")

        # Install kernel spec in the project's venv with proper PATH
        import json
        import subprocess

        print("Installing/checking engine spec in project venv...")
        # Slugify the project name to ensure it's a valid kernel spec name (no spaces)
        project_name = os.path.basename(project_root)
        slugified_name = slugify_kernel_name(project_name)
        engine_spec_name = f"workbooks_{slugified_name}"
        # print(f"DEBUG: project_name = '{project_name}', slugified = '{slugified_name}', engine_spec_name = '{engine_spec_name}'")

        # First install the basic kernel spec
        install_result = subprocess.run(
            [
                venv_python,
                "-m",
                "ipykernel",
                "install",
                "--user",
                "--name",
                engine_spec_name,
                "--display-name",
                f"Workbooks ({os.path.basename(project_root)})",
            ],
            capture_output=True,
            text=True,
        )

        if install_result.returncode != 0:
            print(f"Warning: Could not install engine spec: {install_result.stderr}")
            # Continue anyway, it might already be installed
        else:
            print(f"Engine spec '{engine_spec_name}' installed successfully")
            print(f"Install output: {install_result.stdout}")

        # Now modify the kernel.json to set PATH environment variable
        # This ensures ! commands use the venv's executables
        kernel_dir = os.path.expanduser(
            f"~/.local/share/jupyter/kernels/{engine_spec_name}"
        )
        if not os.path.exists(kernel_dir):
            kernel_dir = os.path.expanduser(
                f"~/Library/Jupyter/kernels/{engine_spec_name}"
            )

        kernel_json_path = os.path.join(kernel_dir, "kernel.json")

        if os.path.exists(kernel_json_path):
            try:
                with open(kernel_json_path) as f:
                    kernel_spec = json.load(f)

                # Add environment variables to prepend venv bin to PATH
                venv_bin = os.path.dirname(venv_python)
                if "env" not in kernel_spec:
                    kernel_spec["env"] = {}

                # Prepend venv bin directory to PATH
                # This ensures shell commands like !pip use the venv's executables
                # Use actual system PATH instead of {PATH} placeholder which doesn't always expand
                current_path = os.environ.get(
                    "PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
                )
                kernel_spec["env"]["PATH"] = f"{venv_bin}:{current_path}"

                # Inject custom environment variables (like WORKBOOKS_PROJECT_FOLDER)
                if request.env_vars:
                    for key, value in request.env_vars.items():
                        kernel_spec["env"][key] = value
                    # Note: Not logging env var injection for security (secrets may be present)

                with open(kernel_json_path, "w") as f:
                    json.dump(kernel_spec, f, indent=2)

                print(f"Updated kernel spec with PATH={venv_bin}:$PATH")
            except Exception as e:
                print(f"Warning: Could not update kernel spec PATH: {e}")
        else:
            print(f"Warning: kernel.json not found at {kernel_json_path}")

        print(f"Creating engine manager with engine_name='{engine_spec_name}'...")
        km = AsyncKernelManager(kernel_name=engine_spec_name)

        # Prepare environment variables for the kernel
        # Start with the current environment and add our custom vars
        kernel_env = os.environ.copy()

        # Inject custom environment variables (secrets, project folder, etc.)
        if request.env_vars:
            for key, value in request.env_vars.items():
                kernel_env[key] = value
            # Note: Not logging env var injection for security (secrets may be present)

        # Set kernel working directory to notebook's directory (standard Jupyter behavior)
        # This allows local imports from the same directory as the notebook
        notebook_dir = os.path.dirname(workbook_path)
        print(f"Starting engine process with cwd={notebook_dir}...")
        await km.start_kernel(cwd=notebook_dir, env=kernel_env)

        # Wait for engine to be ready
        print("Getting engine client...")
        kc = km.client()
        kc.start_channels()

        print("Waiting for engine to be ready...")
        try:
            await kc.wait_for_ready(timeout=60)  # Increased from 30 to 60 seconds
            print("Engine is ready!")
        except RuntimeError as e:
            print(f"Engine failed to become ready after 60 seconds: {e}")
            await km.shutdown_kernel()
            raise HTTPException(
                status_code=500,
                detail=f"Engine failed to start after 60 seconds: {e!s}",
            )

        engines[workbook_path] = km

        # Store secret values for output redaction
        if request.secrets:
            secret_values[workbook_path] = request.secrets
            print(f"Stored {len(request.secrets)} secret values for output redaction")
        else:
            secret_values[workbook_path] = {}

        print(f"Engine started successfully for {workbook_path}")

        return {
            "status": "started",
            "workbook_path": workbook_path,
            "engine_name": engine_name,
        }
    except HTTPException:
        raise
    except Exception as e:
        import traceback

        error_detail = f"{e!s}\n\nTraceback:\n{traceback.format_exc()}"
        print(f"ERROR starting engine: {error_detail}")
        raise HTTPException(status_code=500, detail=error_detail)


# Output limiting constants
MAX_OUTPUT_LINES = 1000  # Maximum number of output lines to keep
MAX_OUTPUTS_START = 100  # Keep first N outputs
MAX_OUTPUTS_END = 50  # Keep last M outputs


@app.post("/engine/execute", response_model=ExecuteResponse)
async def execute_code(request: ExecuteRequest):
    """Execute code in a workbook's engine."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)
    code = request.code

    km = engines.get(workbook_path)
    if not km:
        print(f"ERROR: No engine found for workbook: {workbook_path}")
        print(f"Available engines: {list(engines.keys())}")
        raise HTTPException(status_code=404, detail="No engine found for this workbook")

    try:
        kc = km.client()

        # Preprocess code (e.g., convert !cd to %cd)
        code = preprocess_code(code)

        # Execute code
        msg_id = kc.execute(code, store_history=True, silent=False)

        # Collect outputs with limiting
        outputs: list[Output] = []
        tail_outputs: list[Output] = []  # Ring buffer for last N outputs
        has_error = False
        output_count = 0
        truncated = False
        total_text_length = 0
        MAX_TOTAL_TEXT_LENGTH = 10_000_000  # 10MB of text
        exec_count = None

        while True:
            try:
                # Increased timeout to 300 seconds for long-running cells
                msg = await asyncio.wait_for(kc.get_iopub_msg(), timeout=300.0)

                # Only process messages for our execution
                if msg.get("parent_header", {}).get("msg_id") != msg_id:
                    continue

                msg_type = msg["msg_type"]
                content = msg["content"]

                new_output = None

                if msg_type == "stream":
                    text = content["text"]
                    total_text_length += len(text)

                    # If total text is too large, truncate
                    if total_text_length > MAX_TOTAL_TEXT_LENGTH:
                        if not truncated:
                            outputs.append(
                                Output(
                                    output_type="stream",
                                    name="stderr",
                                    text=f"\n... Output truncated (exceeded {MAX_TOTAL_TEXT_LENGTH} bytes) ...\n",
                                )
                            )
                            truncated = True
                        continue

                    # Check if output contains secrets
                    secrets = secret_values.get(workbook_path, {})
                    metadata = None
                    if contains_secret(text, secrets):
                        metadata = {"contains_secrets": True}

                    new_output = Output(
                        output_type="stream",
                        name=content["name"],
                        text=text,
                        metadata=metadata,
                    )

                elif msg_type == "execute_result":
                    # Check if output contains secrets (check text/plain repr)
                    secrets = secret_values.get(workbook_path, {})
                    metadata = None
                    data = content["data"]
                    if "text/plain" in data:
                        text_repr = data["text/plain"]
                        if isinstance(text_repr, list):
                            text_repr = "".join(text_repr)
                        if contains_secret(text_repr, secrets):
                            metadata = {"contains_secrets": True}

                    new_output = Output(
                        output_type="execute_result",
                        data=data,
                        execution_count=content["execution_count"],
                        metadata=metadata,
                    )

                elif msg_type == "display_data":
                    new_output = Output(
                        output_type="display_data",
                        data=content["data"],
                    )

                elif msg_type == "error":
                    has_error = True

                    # Check if error output contains secrets
                    secrets = secret_values.get(workbook_path, {})
                    metadata = None
                    traceback_text = (
                        "\n".join(content["traceback"]) if content["traceback"] else ""
                    )
                    error_text = (
                        f"{content['ename']}: {content['evalue']}\n{traceback_text}"
                    )
                    if contains_secret(error_text, secrets):
                        metadata = {"contains_secrets": True}

                    new_output = Output(
                        output_type="error",
                        ename=content["ename"],
                        evalue=content["evalue"],
                        traceback=content["traceback"],
                        metadata=metadata,
                    )

                elif msg_type == "status" and content["execution_state"] == "idle":
                    break

                # Add output with smart limiting
                if new_output:
                    output_count += 1

                    if output_count <= MAX_OUTPUTS_START:
                        # Keep first N outputs
                        outputs.append(new_output)
                    else:
                        # After first N, start filling tail buffer
                        if not truncated and len(outputs) == MAX_OUTPUTS_START:
                            outputs.append(
                                Output(
                                    output_type="stream",
                                    name="stdout",
                                    text=f"\n... Output truncated (showing first {MAX_OUTPUTS_START} and last {MAX_OUTPUTS_END} outputs) ...\n",
                                )
                            )
                            truncated = True

                        # Ring buffer for last M outputs
                        tail_outputs.append(new_output)
                        if len(tail_outputs) > MAX_OUTPUTS_END:
                            tail_outputs.pop(0)

            except TimeoutError:
                # Add a message indicating timeout
                outputs.append(
                    Output(
                        output_type="stream",
                        name="stderr",
                        text="\n... Cell execution timeout (300s) ...\n",
                    )
                )
                break

        # Get execution count from the execute_reply message
        try:
            reply = await asyncio.wait_for(kc.get_shell_msg(), timeout=1.0)
            if reply.get("parent_header", {}).get("msg_id") == msg_id:
                exec_count = reply.get("content", {}).get("execution_count")
        except TimeoutError:
            pass

        # Combine outputs: first N + truncation message (if any) + last M
        if tail_outputs:
            final_outputs = outputs + tail_outputs
        else:
            final_outputs = outputs

        return ExecuteResponse(
            success=not has_error, outputs=final_outputs, execution_count=exec_count
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/engine/execute_stream")
async def execute_code_stream(request: ExecuteRequest):
    """Execute code and stream outputs in real-time via Server-Sent Events."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)
    code = request.code

    km = engines.get(workbook_path)
    if not km:
        print(f"ERROR: No engine found for workbook: {workbook_path}")
        print(f"Available engines: {list(engines.keys())}")
        raise HTTPException(status_code=404, detail="No engine found for this workbook")

    async def generate():
        """Generate SSE events as outputs arrive."""
        try:
            kc = km.client()

            # Preprocess code (e.g., convert !cd to %cd)
            processed_code = preprocess_code(code)

            # Execute code
            msg_id = kc.execute(processed_code, store_history=True, silent=False)

            output_count = 0
            truncated = False
            skip_outputs = False
            exec_count = None
            MAX_OUTPUT_MESSAGES = 100  # Stop actively processing after this many

            # Send a start event
            yield f"data: {json.dumps({'type': 'start'})}\n\n"

            while True:
                try:
                    # Use shorter timeout when skipping to drain queue faster
                    timeout = 0.01 if skip_outputs else 300.0
                    msg = await asyncio.wait_for(kc.get_iopub_msg(), timeout=timeout)

                    # Only process messages for our execution
                    if msg.get("parent_header", {}).get("msg_id") != msg_id:
                        continue

                    msg_type = msg["msg_type"]
                    content = msg["content"]

                    # Check for completion first
                    if msg_type == "status" and content["execution_state"] == "idle":
                        # Get execution count from shell reply
                        try:
                            reply = await asyncio.wait_for(
                                kc.get_shell_msg(), timeout=1.0
                            )
                            if reply.get("parent_header", {}).get("msg_id") == msg_id:
                                exec_count = reply.get("content", {}).get(
                                    "execution_count"
                                )
                        except TimeoutError:
                            pass

                        # Send completion event with execution count
                        yield f"data: {json.dumps({'type': 'complete', 'success': True, 'execution_count': exec_count})}\n\n"
                        break

                    # If skipping, just drain the queue and wait for completion
                    if skip_outputs:
                        continue

                    output_data = None

                    if msg_type == "stream":
                        text = content["text"]
                        # Check if output contains secrets
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        if contains_secret(text, secrets):
                            metadata = {"contains_secrets": True}

                        output_data = {
                            "output_type": "stream",
                            "name": content["name"],
                            "text": text,
                        }
                        if metadata:
                            output_data["metadata"] = metadata

                    elif msg_type == "execute_result":
                        # Check if output contains secrets
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        data = content["data"]
                        if "text/plain" in data:
                            text_repr = data["text/plain"]
                            if isinstance(text_repr, list):
                                text_repr = "".join(text_repr)
                            if contains_secret(text_repr, secrets):
                                metadata = {"contains_secrets": True}

                        output_data = {
                            "output_type": "execute_result",
                            "data": data,
                            "execution_count": content["execution_count"],
                        }
                        if metadata:
                            output_data["metadata"] = metadata

                    elif msg_type == "display_data":
                        output_data = {
                            "output_type": "display_data",
                            "data": content["data"],
                        }

                    elif msg_type == "error":
                        # Check if error output contains secrets
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        traceback_text = (
                            "\n".join(content["traceback"])
                            if content["traceback"]
                            else ""
                        )
                        error_text = (
                            f"{content['ename']}: {content['evalue']}\n{traceback_text}"
                        )
                        if contains_secret(error_text, secrets):
                            metadata = {"contains_secrets": True}

                        output_data = {
                            "output_type": "error",
                            "ename": content["ename"],
                            "evalue": content["evalue"],
                            "traceback": content["traceback"],
                        }
                        if metadata:
                            output_data["metadata"] = metadata

                    # Send output event
                    if output_data:
                        output_count += 1
                        event = {
                            "type": "output",
                            "output": output_data,
                            "index": output_count,
                        }
                        yield f"data: {json.dumps(event)}\n\n"

                        # After limit, stop processing outputs - just wait for completion
                        if output_count >= MAX_OUTPUT_MESSAGES and not truncated:
                            truncation_msg = {
                                "type": "output",
                                "output": {
                                    "output_type": "stream",
                                    "name": "stdout",
                                    "text": f"\n... Output limit reached ({MAX_OUTPUT_MESSAGES} messages). Execution continues in background ...\n",
                                },
                            }
                            yield f"data: {json.dumps(truncation_msg)}\n\n"
                            truncated = True
                            skip_outputs = True

                except TimeoutError:
                    # If skipping and timeout, keep waiting for completion
                    if skip_outputs:
                        continue
                    # Real timeout - execution took too long
                    yield f"data: {json.dumps({'type': 'timeout'})}\n\n"
                    break

        except Exception as e:
            # Send error event
            error_event = {"type": "error", "message": str(e)}
            yield f"data: {json.dumps(error_event)}\n\n"

    return StreamingResponse(generate(), media_type="text/event-stream")


@app.post("/engine/stop")
async def stop_engine(workbook_path: str):
    """Stop a workbook's engine."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(workbook_path)
    km = engines.pop(workbook_path, None)
    secret_values.pop(workbook_path, None)  # Clean up secret values

    if km:
        try:
            await km.shutdown_kernel()
            print(f"Engine stopped for {workbook_path}")
            return {"status": "stopped", "workbook_path": workbook_path}
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e))

    print(f"Engine not found for stopping: {workbook_path}")
    return {"status": "not_found", "workbook_path": workbook_path}


@app.post("/engine/interrupt")
async def interrupt_engine(workbook_path: str):
    """Interrupt the currently executing cell in a workbook's engine."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(workbook_path)
    km = engines.get(workbook_path)
    if not km:
        print(f"ERROR: No engine found for interrupt: {workbook_path}")
        print(f"Available engines: {list(engines.keys())}")
        raise HTTPException(
            status_code=404, detail=f"No engine found for {workbook_path}"
        )

    try:
        await km.interrupt_kernel()
        return {"status": "interrupted", "workbook_path": workbook_path}
    except Exception as e:
        raise HTTPException(
            status_code=500, detail=f"Failed to interrupt engine: {e!s}"
        )


@app.post("/engine/restart")
async def restart_engine(request: StartEngineRequest):
    """Restart a workbook's engine (stop and start fresh)."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)

    # Stop existing engine if running
    km = engines.pop(workbook_path, None)
    secret_values.pop(workbook_path, None)  # Clean up secret values
    if km:
        try:
            await km.shutdown_kernel()
            print(f"Stopped existing engine for {workbook_path}")
        except Exception as e:
            print(f"Warning: Error stopping engine: {e}")

    # Start new engine (reuse start_engine logic)
    return await start_engine(request)


@app.post("/engine/force_restart")
async def force_restart_engine(request: StartEngineRequest):
    """Force restart a workbook's engine by killing the kernel process."""
    import signal

    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)

    # Stop existing engine if running
    km = engines.pop(workbook_path, None)
    secret_values.pop(workbook_path, None)  # Clean up secret values

    if km:
        try:
            # Get the kernel process ID if available
            if hasattr(km, 'kernel') and km.kernel and hasattr(km.kernel, 'pid'):
                kernel_pid = km.kernel.pid
                if kernel_pid:
                    print(f"Force killing kernel process {kernel_pid} for {workbook_path}")
                    try:
                        os.kill(kernel_pid, signal.SIGKILL)
                        print(f"Kernel process {kernel_pid} killed successfully")
                    except ProcessLookupError:
                        print(f"Kernel process {kernel_pid} already dead")
                    except Exception as kill_err:
                        print(f"Error killing kernel process: {kill_err}")

            # Also try graceful shutdown (in case process is already dead)
            try:
                await km.shutdown_kernel()
                print(f"Shutdown engine for {workbook_path}")
            except Exception as shutdown_err:
                print(f"Error during shutdown: {shutdown_err}")

        except Exception as e:
            print(f"Warning: Error force stopping engine: {e}")

    # Give the OS a moment to clean up
    import asyncio
    await asyncio.sleep(0.5)

    # Start new engine (reuse start_engine logic)
    return await start_engine(request)


class Cell(BaseModel):
    """A single cell from a notebook."""

    source: str  # Cell source code
    cell_type: str = "code"  # code or markdown


class ExecuteAllRequest(BaseModel):
    """Request to execute all cells in a workbook."""

    workbook_path: str
    cells: list[Cell]


class CellExecutionResult(BaseModel):
    """Result of executing a single cell."""

    cell_index: int
    success: bool
    outputs: list[Output]
    execution_count: int | None = None
    error: str | None = None


class ExecuteAllResponse(BaseModel):
    """Response with results from executing all cells."""

    success: bool  # True if all cells succeeded
    cell_results: list[CellExecutionResult]
    total_cells: int
    successful_cells: int
    failed_cells: int


@app.post("/engine/execute-all", response_model=ExecuteAllResponse)
async def execute_all_cells(request: ExecuteAllRequest):
    """Execute all cells in a workbook sequentially."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)
    cells = request.cells

    km = engines.get(workbook_path)
    if not km:
        print(f"ERROR: No engine found for workbook: {workbook_path}")
        print(f"Available engines: {list(engines.keys())}")
        raise HTTPException(status_code=404, detail="No engine found for this workbook")

    logger.info(f"Executing {len(cells)} cells for {workbook_path}")

    cell_results = []
    successful_cells = 0
    failed_cells = 0

    for cell_index, cell in enumerate(cells):
        # Skip markdown cells
        if cell.cell_type != "code":
            continue

        # Skip empty cells
        if not cell.source or not cell.source.strip():
            continue

        logger.info(f"Executing cell {cell_index + 1}/{len(cells)}")

        try:
            kc = km.client()

            # Preprocess code (e.g., convert !cd to %cd)
            processed_code = preprocess_code(cell.source)

            # Execute code
            msg_id = kc.execute(processed_code, store_history=True, silent=False)

            # Collect outputs
            outputs: list[Output] = []
            has_error = False
            exec_count = None

            while True:
                try:
                    msg = await asyncio.wait_for(kc.get_iopub_msg(), timeout=300.0)

                    # Only process messages for our execution
                    if msg.get("parent_header", {}).get("msg_id") != msg_id:
                        continue

                    msg_type = msg["msg_type"]
                    content = msg["content"]

                    new_output = None

                    if msg_type == "stream":
                        text = content["text"]
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        if contains_secret(text, secrets):
                            metadata = {"contains_secrets": True}

                        new_output = Output(
                            output_type="stream",
                            name=content["name"],
                            text=text,
                            metadata=metadata,
                        )

                    elif msg_type == "execute_result":
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        data = content["data"]
                        if "text/plain" in data:
                            text_repr = data["text/plain"]
                            if isinstance(text_repr, list):
                                text_repr = "".join(text_repr)
                            if contains_secret(text_repr, secrets):
                                metadata = {"contains_secrets": True}

                        new_output = Output(
                            output_type="execute_result",
                            data=data,
                            execution_count=content["execution_count"],
                            metadata=metadata,
                        )

                    elif msg_type == "display_data":
                        new_output = Output(
                            output_type="display_data",
                            data=content["data"],
                        )

                    elif msg_type == "error":
                        has_error = True
                        secrets = secret_values.get(workbook_path, {})
                        metadata = None
                        traceback_text = (
                            "\n".join(content["traceback"])
                            if content["traceback"]
                            else ""
                        )
                        error_text = (
                            f"{content['ename']}: {content['evalue']}\n{traceback_text}"
                        )
                        if contains_secret(error_text, secrets):
                            metadata = {"contains_secrets": True}

                        new_output = Output(
                            output_type="error",
                            ename=content["ename"],
                            evalue=content["evalue"],
                            traceback=content["traceback"],
                            metadata=metadata,
                        )

                    elif msg_type == "status" and content["execution_state"] == "idle":
                        break

                    if new_output:
                        outputs.append(new_output)

                except TimeoutError:
                    outputs.append(
                        Output(
                            output_type="stream",
                            name="stderr",
                            text="\n... Cell execution timeout (300s) ...\n",
                        )
                    )
                    has_error = True
                    break

            # Get execution count
            try:
                reply = await asyncio.wait_for(kc.get_shell_msg(), timeout=1.0)
                if reply.get("parent_header", {}).get("msg_id") == msg_id:
                    exec_count = reply.get("content", {}).get("execution_count")
            except TimeoutError:
                pass

            # Create result for this cell
            error_message = None
            if has_error and outputs:
                # Extract error message from error output
                for output in outputs:
                    if output.output_type == "error":
                        error_message = f"{output.ename}: {output.evalue}"
                        break

            cell_result = CellExecutionResult(
                cell_index=cell_index,
                success=not has_error,
                outputs=outputs,
                execution_count=exec_count,
                error=error_message,
            )

            cell_results.append(cell_result)

            if has_error:
                failed_cells += 1
                logger.warning(f"Cell {cell_index + 1} failed: {error_message}")
                # Stop execution on first error
                break
            else:
                successful_cells += 1
                logger.info(f"Cell {cell_index + 1} succeeded")

        except Exception as e:
            logger.error(f"Error executing cell {cell_index + 1}: {e}")
            cell_result = CellExecutionResult(
                cell_index=cell_index,
                success=False,
                outputs=[],
                error=str(e),
            )
            cell_results.append(cell_result)
            failed_cells += 1
            # Stop on error
            break

    logger.info(
        f"Execution complete: {successful_cells} successful, {failed_cells} failed"
    )

    return ExecuteAllResponse(
        success=(failed_cells == 0),
        cell_results=cell_results,
        total_cells=len(cells),
        successful_cells=successful_cells,
        failed_cells=failed_cells,
    )


class CompleteRequest(BaseModel):
    workbook_path: str
    code: str
    cursor_pos: int


class CompletionMatch(BaseModel):
    text: str
    start: int
    end: int
    type: str | None = None


class CompleteResponse(BaseModel):
    matches: list[CompletionMatch]
    cursor_start: int
    cursor_end: int


@app.post("/engine/complete", response_model=CompleteResponse)
async def complete_code(request: CompleteRequest):
    """Get code completions from the Jupyter kernel."""
    # Normalize path to match start_engine's normalized key
    workbook_path = normalize_path(request.workbook_path)
    code = request.code
    cursor_pos = request.cursor_pos

    km = engines.get(workbook_path)
    if not km:
        print(f"ERROR: No engine found for workbook: {workbook_path}")
        print(f"Available engines: {list(engines.keys())}")
        raise HTTPException(status_code=404, detail="No engine found for this workbook")

    try:
        kc = km.client()

        # Request completions from kernel
        msg_id = kc.complete(code, cursor_pos)

        # Wait for completion reply
        while True:
            try:
                msg = await asyncio.wait_for(kc.get_shell_msg(), timeout=5.0)
                if msg.get("parent_header", {}).get("msg_id") == msg_id:
                    content = msg["content"]
                    if msg["msg_type"] == "complete_reply":
                        matches = content.get("matches", [])
                        cursor_start = content.get("cursor_start", cursor_pos)
                        cursor_end = content.get("cursor_end", cursor_pos)

                        # Convert matches to CompletionMatch objects
                        completion_matches = [
                            CompletionMatch(
                                text=match,
                                start=cursor_start,
                                end=cursor_end,
                                type=None,  # Jupyter doesn't provide type info by default
                            )
                            for match in matches
                        ]

                        return CompleteResponse(
                            matches=completion_matches,
                            cursor_start=cursor_start,
                            cursor_end=cursor_end,
                        )
            except TimeoutError:
                # If timeout, return empty completions
                return CompleteResponse(
                    matches=[], cursor_start=cursor_pos, cursor_end=cursor_pos
                )

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    logger.info("=== Workbooks Engine Server Starting ===")
    logger.info(f"Port: {port}")
    logger.info(f"Python: {sys.executable}")
    logger.info(f"Working directory: {os.getcwd()}")
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="info")
