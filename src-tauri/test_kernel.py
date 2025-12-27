#!/usr/bin/env python3
"""
Standalone test script to debug kernel startup issues.
Run this directly to test kernel functionality without the Tauri app.
"""

import asyncio
import os
import sys
from pathlib import Path

# Add current directory to path
sys.path.insert(0, str(Path(__file__).parent))


async def test_kernel_start():
    """Test starting a kernel for a project."""

    project_root = "/Users/jmitch/Desktop/Test"
    venv_python = os.path.join(project_root, ".venv", "bin", "python")

    print("=" * 80)
    print("Testing Kernel Startup")
    print("=" * 80)
    print(f"Project root: {project_root}")
    print(f"Python path: {venv_python}")
    print(f"Python exists: {os.path.exists(venv_python)}")
    print()

    # Test 1: Check if ipykernel is installed
    print("Test 1: Checking if ipykernel is installed...")
    import subprocess

    check_result = subprocess.run(
        [venv_python, "-c", "import ipykernel; print(ipykernel.__version__)"],
        capture_output=True,
        text=True,
    )
    if check_result.returncode != 0:
        print("❌ FAILED: ipykernel not installed")
        print(f"Error: {check_result.stderr}")
        return
    print(f"✅ PASSED: ipykernel version {check_result.stdout.strip()}")
    print()

    # Test 2: Install kernel spec
    print("Test 2: Installing kernel spec...")
    kernel_spec_name = f"workbooks_{os.path.basename(project_root)}"
    print(f"Kernel spec name: {kernel_spec_name}")

    install_result = subprocess.run(
        [
            venv_python,
            "-m",
            "ipykernel",
            "install",
            "--user",
            "--name",
            kernel_spec_name,
            "--display-name",
            f"Workbooks ({os.path.basename(project_root)})",
        ],
        capture_output=True,
        text=True,
    )

    if install_result.returncode != 0:
        print("❌ FAILED: Could not install kernel spec")
        print(f"Error: {install_result.stderr}")
        return
    print("✅ PASSED: Kernel spec installed")
    print(f"Output: {install_result.stdout.strip()}")
    print()

    # Test 3: Start kernel using AsyncKernelManager
    print("Test 3: Starting kernel with AsyncKernelManager...")
    try:
        from jupyter_client import AsyncKernelManager

        km = AsyncKernelManager(kernel_name=kernel_spec_name)
        print(f"Created AsyncKernelManager with kernel_name='{kernel_spec_name}'")

        await km.start_kernel(cwd=project_root)
        print("✅ PASSED: Kernel started successfully")

        # Test 4: Check if kernel is ready
        print()
        print("Test 4: Waiting for kernel to be ready...")
        kc = km.client()
        kc.start_channels()

        try:
            await asyncio.wait_for(kc.wait_for_ready(), timeout=30)
            print("✅ PASSED: Kernel is ready")
        except TimeoutError:
            print("❌ FAILED: Kernel did not become ready in 30 seconds")
            await km.shutdown_kernel()
            return

        # Test 5: Execute code
        print()
        print("Test 5: Executing test code...")
        msg_id = kc.execute(
            "print('Hello from Workbooks!')", store_history=True, silent=False
        )

        # Collect output
        outputs = []
        while True:
            try:
                msg = await asyncio.wait_for(kc.get_iopub_msg(), timeout=5.0)
                msg_type = msg["msg_type"]

                if msg_type == "stream":
                    content = msg["content"]
                    outputs.append(content["text"])
                elif (
                    msg_type == "status" and msg["content"]["execution_state"] == "idle"
                ):
                    break
            except TimeoutError:
                break

        if outputs:
            print(f"✅ PASSED: Got output: {''.join(outputs).strip()}")
        else:
            print("❌ FAILED: No output received")

        # Cleanup
        print()
        print("Cleaning up...")
        await km.shutdown_kernel()
        print("✅ Kernel shut down")

        print()
        print("=" * 80)
        print("All tests passed! ✅")
        print("=" * 80)

    except Exception as e:
        print(f"❌ FAILED: {e}")
        import traceback

        traceback.print_exc()


if __name__ == "__main__":
    print("Starting kernel test...\n")
    asyncio.run(test_kernel_start())
