#!/usr/bin/env python3
"""
Jupyter kernel manager for Tether notebooks.
Manages kernel lifecycle and code execution with state preservation.
"""
import sys
import json
import jupyter_client

class KernelManager:
    def __init__(self):
        self.kernel_manager = jupyter_client.KernelManager()
        self.kernel_client = None

    def start(self):
        """Start a new kernel"""
        self.kernel_manager.start_kernel()
        self.kernel_client = self.kernel_manager.client()
        self.kernel_client.start_channels()
        # Wait for kernel to be ready
        self.kernel_client.wait_for_ready(timeout=10)
        return {"status": "started"}

    def execute(self, code):
        """Execute code in the kernel and return results"""
        if not self.kernel_client:
            return {
                "success": False,
                "error": "Kernel not started"
            }

        # Execute the code
        msg_id = self.kernel_client.execute(code, store_history=True)

        # Collect outputs
        outputs = []

        while True:
            try:
                msg = self.kernel_client.get_iopub_msg(timeout=30)
                msg_type = msg['header']['msg_type']
                content = msg['content']

                if msg_type == 'stream':
                    outputs.append({
                        'output_type': 'stream',
                        'name': content['name'],
                        'text': content['text']
                    })

                elif msg_type == 'execute_result':
                    outputs.append({
                        'output_type': 'execute_result',
                        'data': content['data'],
                        'execution_count': content['execution_count']
                    })

                elif msg_type == 'display_data':
                    outputs.append({
                        'output_type': 'display_data',
                        'data': content['data']
                    })

                elif msg_type == 'error':
                    outputs.append({
                        'output_type': 'error',
                        'ename': content['ename'],
                        'evalue': content['evalue'],
                        'traceback': content['traceback']
                    })

                elif msg_type == 'status':
                    if content['execution_state'] == 'idle':
                        break

            except jupyter_client.queue.Empty:
                break

        # Get the execution reply
        reply = self.kernel_client.get_shell_msg(timeout=5)
        success = reply['content']['status'] == 'ok'

        return {
            'success': success,
            'outputs': outputs
        }

    def shutdown(self):
        """Shutdown the kernel"""
        if self.kernel_client:
            self.kernel_client.stop_channels()
        if self.kernel_manager:
            self.kernel_manager.shutdown_kernel()
        return {"status": "shutdown"}

def main():
    """Main loop for kernel manager"""
    manager = KernelManager()

    # Write ready signal
    print(json.dumps({"status": "ready"}), flush=True)
    sys.stderr.write("Kernel manager ready, waiting for commands...\n")
    sys.stderr.flush()

    for line in sys.stdin:
        try:
            sys.stderr.write(f"Received line: {line.strip()}\n")
            sys.stderr.flush()

            command = json.loads(line.strip())
            action = command.get('action')
            sys.stderr.write(f"Action: {action}\n")
            sys.stderr.flush()

            if action == 'start':
                sys.stderr.write("Starting kernel...\n")
                sys.stderr.flush()
                result = manager.start()
                sys.stderr.write(f"Kernel started: {result}\n")
                sys.stderr.flush()
            elif action == 'execute':
                code = command.get('code', '')
                sys.stderr.write(f"Executing code: {code[:50]}...\n")
                sys.stderr.flush()
                result = manager.execute(code)
            elif action == 'shutdown':
                sys.stderr.write("Shutting down kernel...\n")
                sys.stderr.flush()
                result = manager.shutdown()
                print(json.dumps(result), flush=True)
                break
            else:
                result = {"error": f"Unknown action: {action}"}

            print(json.dumps(result), flush=True)
            sys.stderr.write(f"Sent response: {json.dumps(result)}\n")
            sys.stderr.flush()

        except Exception as e:
            sys.stderr.write(f"Error: {str(e)}\n")
            sys.stderr.flush()
            error_result = {"error": str(e)}
            print(json.dumps(error_result), flush=True)

if __name__ == '__main__':
    main()
