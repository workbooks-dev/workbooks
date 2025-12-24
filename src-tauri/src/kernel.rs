use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use uuid::Uuid;
use chrono::Utc;

/// Represents a Jupyter kernel specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelSpec {
    pub argv: Vec<String>,
    pub display_name: String,
    pub language: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub metadata: Value,
}

/// Kernel specification with its location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelSpecInfo {
    pub name: String,
    pub spec: KernelSpec,
    pub resource_dir: PathBuf,
}

/// Connection information for a Jupyter kernel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub ip: String,
    pub transport: String,
    pub shell_port: u16,
    pub iopub_port: u16,
    pub stdin_port: u16,
    pub control_port: u16,
    pub hb_port: u16,
    pub signature_scheme: String,
    pub key: String,
}

impl ConnectionInfo {
    /// Generate new connection info with random ports
    pub fn generate() -> Self {
        use std::net::TcpListener;

        let get_free_port = || -> u16 {
            TcpListener::bind("127.0.0.1:0")
                .expect("Failed to bind to random port")
                .local_addr()
                .expect("Failed to get local address")
                .port()
        };

        Self {
            ip: "127.0.0.1".to_string(),
            transport: "tcp".to_string(),
            shell_port: get_free_port(),
            iopub_port: get_free_port(),
            stdin_port: get_free_port(),
            control_port: get_free_port(),
            hb_port: get_free_port(),
            signature_scheme: "hmac-sha256".to_string(),
            key: Uuid::new_v4().to_string(),
        }
    }

    /// Get the ZeroMQ connection string for a given channel
    pub fn get_connection_string(&self, port: u16) -> String {
        format!("{}://{}:{}", self.transport, self.ip, port)
    }

    /// Save connection info to a file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// Jupyter message header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    pub msg_id: String,
    pub msg_type: String,
    pub username: String,
    pub session: String,
    pub date: String,
    pub version: String,
}

impl MessageHeader {
    pub fn new(msg_type: &str, session: &str) -> Self {
        Self {
            msg_id: Uuid::new_v4().to_string(),
            msg_type: msg_type.to_string(),
            username: "workbooks".to_string(),
            session: session.to_string(),
            date: Utc::now().to_rfc3339(),
            version: "5.3".to_string(),
        }
    }
}

/// Jupyter message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub header: MessageHeader,
    pub parent_header: Value,
    pub metadata: Value,
    pub content: Value,
}

impl Message {
    pub fn new(msg_type: &str, session: &str, content: Value) -> Self {
        Self {
            header: MessageHeader::new(msg_type, session),
            parent_header: Value::Null,
            metadata: Value::Object(serde_json::Map::new()),
            content,
        }
    }
}

/// Cell output types matching Jupyter notebook format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "output_type")]
pub enum CellOutput {
    #[serde(rename = "stream")]
    Stream {
        name: String,
        text: String,
    },
    #[serde(rename = "execute_result")]
    ExecuteResult {
        data: Value,
        execution_count: i32,
    },
    #[serde(rename = "display_data")]
    DisplayData {
        data: Value,
    },
    #[serde(rename = "error")]
    Error {
        ename: String,
        evalue: String,
        traceback: Vec<String>,
    },
}

/// Result of executing code in a kernel
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub outputs: Vec<CellOutput>,
}

/// Manages a single Jupyter kernel process
pub struct KernelProcess {
    connection_file: PathBuf,
    connection_info: ConnectionInfo,
    process: Child,
    session_id: String,
    shell_socket: zmq::Socket,
    iopub_socket: zmq::Socket,
    stdin_socket: zmq::Socket,
    control_socket: zmq::Socket,
}

impl KernelProcess {
    /// Start a new kernel process with the given spec
    pub fn start(spec: &KernelSpec, project_root: &Path) -> Result<Self> {
        println!("Starting kernel with spec: {:?}", spec);
        println!("Project root: {:?}", project_root);

        let temp_dir = std::env::temp_dir();
        let connection_file = temp_dir.join(format!("kernel-{}.json", Uuid::new_v4()));

        println!("Connection file: {:?}", connection_file);

        // Generate connection info
        let connection_info = ConnectionInfo::generate();
        println!("Generated ports - shell:{}, iopub:{}, stdin:{}, control:{}, hb:{}",
                 connection_info.shell_port, connection_info.iopub_port,
                 connection_info.stdin_port, connection_info.control_port,
                 connection_info.hb_port);
        connection_info.save_to_file(&connection_file)?;

        // Prepare kernel command
        let mut cmd_args = Vec::new();
        for arg in &spec.argv {
            if arg.contains("{connection_file}") {
                cmd_args.push(arg.replace("{connection_file}", connection_file.to_str().unwrap()));
            } else {
                cmd_args.push(arg.clone());
            }
        }

        // Start the kernel process
        let mut command = Command::new(&cmd_args[0]);
        command
            .args(&cmd_args[1..])
            .current_dir(project_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables from spec
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let process = command.spawn()
            .context("Failed to spawn kernel process")?;

        println!("Kernel process spawned with PID: {:?}", process.id());

        // Create ZeroMQ context and sockets
        let context = zmq::Context::new();
        println!("Created ZeroMQ context");

        let shell_socket = context.socket(zmq::DEALER)?;
        let shell_identity = format!("shell-{}", Uuid::new_v4());
        shell_socket.set_identity(shell_identity.as_bytes())?;
        println!("Connecting shell socket to {} with identity: {}",
                 connection_info.get_connection_string(connection_info.shell_port), shell_identity);
        shell_socket.connect(&connection_info.get_connection_string(connection_info.shell_port))?;

        let iopub_socket = context.socket(zmq::SUB)?;
        println!("Connecting iopub socket to {}", connection_info.get_connection_string(connection_info.iopub_port));
        iopub_socket.connect(&connection_info.get_connection_string(connection_info.iopub_port))?;
        iopub_socket.set_subscribe(b"")?; // Subscribe to all messages

        let stdin_socket = context.socket(zmq::DEALER)?;
        let stdin_identity = format!("stdin-{}", Uuid::new_v4());
        stdin_socket.set_identity(stdin_identity.as_bytes())?;
        stdin_socket.connect(&connection_info.get_connection_string(connection_info.stdin_port))?;

        let control_socket = context.socket(zmq::DEALER)?;
        let control_identity = format!("control-{}", Uuid::new_v4());
        control_socket.set_identity(control_identity.as_bytes())?;
        control_socket.connect(&connection_info.get_connection_string(connection_info.control_port))?;

        let session_id = Uuid::new_v4().to_string();
        println!("Session ID: {}", session_id);

        // Wait a moment for kernel to initialize
        println!("Waiting for kernel to initialize...");
        std::thread::sleep(std::time::Duration::from_millis(2000));

        Ok(Self {
            connection_file,
            connection_info,
            process,
            session_id,
            shell_socket,
            iopub_socket,
            stdin_socket,
            control_socket,
        })
    }

    /// Send a message on the shell channel
    fn send_shell_message(&mut self, msg: &Message) -> Result<()> {
        // Jupyter wire protocol for DEALER sockets
        // [ZMQ identity frames]
        // <IDS|MSG> - delimiter
        // <HMAC signature>
        // <serialized header>
        // <serialized parent header>
        // <serialized metadata>
        // <serialized content>

        let header_json = serde_json::to_string(&msg.header)?;
        let parent_header_json = serde_json::to_string(&msg.parent_header)?;
        let metadata_json = serde_json::to_string(&msg.metadata)?;
        let content_json = serde_json::to_string(&msg.content)?;

        // Compute HMAC signature
        let key = &self.connection_info.key;
        let signature = if key.is_empty() {
            String::new()
        } else {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| anyhow::anyhow!("HMAC error: {}", e))?;

            mac.update(header_json.as_bytes());
            mac.update(parent_header_json.as_bytes());
            mac.update(metadata_json.as_bytes());
            mac.update(content_json.as_bytes());

            hex::encode(mac.finalize().into_bytes())
        };

        println!("Sending message parts:");
        println!("  Header: {}", header_json);
        println!("  Content: {}", content_json);

        // Send message parts
        self.shell_socket.send(b"<IDS|MSG>".as_ref(), zmq::SNDMORE)?;
        self.shell_socket.send(signature.as_bytes(), zmq::SNDMORE)?;
        self.shell_socket.send(header_json.as_bytes(), zmq::SNDMORE)?;
        self.shell_socket.send(parent_header_json.as_bytes(), zmq::SNDMORE)?;
        self.shell_socket.send(metadata_json.as_bytes(), zmq::SNDMORE)?;
        self.shell_socket.send(content_json.as_bytes(), 0)?;

        println!("Message sent successfully");
        Ok(())
    }

    /// Receive a message from the shell channel
    fn receive_shell_message(&mut self) -> Result<Message> {
        // Set timeout for shell socket
        self.shell_socket.set_rcvtimeo(10000)?; // 10 second timeout

        let mut parts = Vec::new();

        // Read all message parts
        loop {
            let msg = self.shell_socket.recv_bytes(0)?;
            parts.push(msg);

            if !self.shell_socket.get_rcvmore()? {
                break;
            }
        }

        // Parse message (skip delimiter and signature)
        if parts.len() < 6 {
            anyhow::bail!("Invalid message format");
        }

        let header: MessageHeader = serde_json::from_slice(&parts[2])?;
        let parent_header: Value = serde_json::from_slice(&parts[3])?;
        let metadata: Value = serde_json::from_slice(&parts[4])?;
        let content: Value = serde_json::from_slice(&parts[5])?;

        Ok(Message {
            header,
            parent_header,
            metadata,
            content,
        })
    }

    /// Receive messages from the iopub channel
    fn receive_iopub_messages(&mut self, timeout_ms: i64) -> Result<Vec<Message>> {
        let mut messages = Vec::new();

        // Set socket timeout
        self.iopub_socket.set_rcvtimeo(timeout_ms as i32)?;

        loop {
            match self.iopub_socket.recv_bytes(0) {
                Ok(_) => {
                    let mut parts = vec![];
                    parts.push(vec![]); // Placeholder for first part

                    // Read remaining parts
                    while self.iopub_socket.get_rcvmore()? {
                        let part = self.iopub_socket.recv_bytes(0)?;
                        parts.push(part);
                    }

                    // Parse message
                    if parts.len() >= 6 {
                        let header: MessageHeader = serde_json::from_slice(&parts[2]).ok().unwrap_or_else(|| {
                            MessageHeader::new("unknown", &self.session_id)
                        });
                        let parent_header: Value = serde_json::from_slice(&parts[3]).unwrap_or(Value::Null);
                        let metadata: Value = serde_json::from_slice(&parts[4]).unwrap_or(Value::Null);
                        let content: Value = serde_json::from_slice(&parts[5]).unwrap_or(Value::Null);

                        messages.push(Message {
                            header,
                            parent_header,
                            metadata,
                            content,
                        });
                    }
                }
                Err(zmq::Error::EAGAIN) => break, // Timeout
                Err(e) => return Err(e.into()),
            }
        }

        Ok(messages)
    }

    /// Execute code in the kernel
    pub fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        println!("Executing code in kernel: {}", code);

        // Send execute_request
        let content = serde_json::json!({
            "code": code,
            "silent": false,
            "store_history": true,
            "user_expressions": {},
            "allow_stdin": false,
            "stop_on_error": true,
        });

        let msg = Message::new("execute_request", &self.session_id, content);
        let msg_id = msg.header.msg_id.clone();
        println!("Sending execute_request with msg_id: {}", msg_id);

        match self.send_shell_message(&msg) {
            Ok(_) => println!("Execute request sent successfully"),
            Err(e) => {
                eprintln!("Failed to send execute request: {:?}", e);
                return Err(e);
            }
        }

        // Wait for execute_reply
        println!("Waiting for execute_reply...");
        let _reply = match self.receive_shell_message() {
            Ok(r) => {
                println!("Received shell reply: {:?}", r.header.msg_type);
                r
            }
            Err(e) => {
                eprintln!("Failed to receive shell message: {:?}", e);
                return Err(e);
            }
        };

        // Collect outputs from iopub
        println!("Collecting outputs from iopub...");
        let mut outputs = Vec::new();
        let mut execution_state = "busy".to_string();
        let mut iteration = 0;

        while execution_state != "idle" {
            iteration += 1;
            println!("IOPub collection iteration {}, current state: {}", iteration, execution_state);

            let messages = match self.receive_iopub_messages(5000) {
                Ok(msgs) => {
                    println!("Received {} messages from iopub", msgs.len());
                    msgs
                }
                Err(e) => {
                    eprintln!("Failed to receive iopub messages: {:?}", e);
                    return Err(e);
                }
            };

            for msg in messages {
                println!("Processing iopub message type: {}", msg.header.msg_type);
                // Only process messages related to our execution
                if let Some(parent_msg_id) = msg.parent_header.get("msg_id").and_then(|v| v.as_str()) {
                    if parent_msg_id != msg_id {
                        continue;
                    }
                }

                match msg.header.msg_type.as_str() {
                    "stream" => {
                        if let (Some(name), Some(text)) = (
                            msg.content.get("name").and_then(|v| v.as_str()),
                            msg.content.get("text").and_then(|v| v.as_str()),
                        ) {
                            outputs.push(CellOutput::Stream {
                                name: name.to_string(),
                                text: text.to_string(),
                            });
                        }
                    }
                    "execute_result" => {
                        if let (Some(data), Some(count)) = (
                            msg.content.get("data"),
                            msg.content.get("execution_count").and_then(|v| v.as_i64()),
                        ) {
                            outputs.push(CellOutput::ExecuteResult {
                                data: data.clone(),
                                execution_count: count as i32,
                            });
                        }
                    }
                    "display_data" => {
                        if let Some(data) = msg.content.get("data") {
                            outputs.push(CellOutput::DisplayData {
                                data: data.clone(),
                            });
                        }
                    }
                    "error" => {
                        if let (Some(ename), Some(evalue), Some(traceback)) = (
                            msg.content.get("ename").and_then(|v| v.as_str()),
                            msg.content.get("evalue").and_then(|v| v.as_str()),
                            msg.content.get("traceback").and_then(|v| v.as_array()),
                        ) {
                            outputs.push(CellOutput::Error {
                                ename: ename.to_string(),
                                evalue: evalue.to_string(),
                                traceback: traceback
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect(),
                            });
                        }
                    }
                    "status" => {
                        if let Some(state) = msg.content.get("execution_state").and_then(|v| v.as_str()) {
                            execution_state = state.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Determine success based on whether there were any errors
        let success = !outputs.iter().any(|o| matches!(o, CellOutput::Error { .. }));

        println!("Execution complete. Success: {}, Output count: {}", success, outputs.len());
        for (i, output) in outputs.iter().enumerate() {
            println!("Output {}: {:?}", i, output);
        }

        Ok(ExecutionResult { success, outputs })
    }

    /// Shutdown the kernel
    pub fn shutdown(mut self) -> Result<()> {
        // Send shutdown request
        let content = serde_json::json!({
            "restart": false,
        });

        let msg = Message::new("shutdown_request", &self.session_id, content);
        let _ = self.send_shell_message(&msg);

        // Wait a bit for graceful shutdown
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Kill the process if still running
        let _ = self.process.kill();
        let _ = self.process.wait();

        // Clean up connection file
        let _ = fs::remove_file(&self.connection_file);

        Ok(())
    }
}

/// Discover all available kernel specs on the system
pub fn discover_kernel_specs() -> Result<Vec<KernelSpecInfo>> {
    let mut specs = Vec::new();
    let search_paths = get_kernel_search_paths();

    for path in search_paths {
        if !path.exists() {
            continue;
        }

        // Each subdirectory in the kernels directory is a kernel spec
        if let Ok(entries) = fs::read_dir(&path) {
            for entry in entries.flatten() {
                let kernel_dir = entry.path();
                if !kernel_dir.is_dir() {
                    continue;
                }

                let kernel_json = kernel_dir.join("kernel.json");
                if !kernel_json.exists() {
                    continue;
                }

                // Parse kernel.json
                match fs::read_to_string(&kernel_json) {
                    Ok(content) => {
                        match serde_json::from_str::<KernelSpec>(&content) {
                            Ok(spec) => {
                                let name = kernel_dir
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                specs.push(KernelSpecInfo {
                                    name,
                                    spec,
                                    resource_dir: kernel_dir,
                                });
                            }
                            Err(e) => {
                                eprintln!("Failed to parse {}: {}", kernel_json.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", kernel_json.display(), e);
                    }
                }
            }
        }
    }

    Ok(specs)
}

/// Get standard kernel search paths for the current platform
fn get_kernel_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // User data directory
    if let Some(data_dir) = dirs::data_dir() {
        paths.push(data_dir.join("jupyter").join("kernels"));
    }

    // System-wide directories
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/usr/local/share/jupyter/kernels"));
        paths.push(PathBuf::from("/usr/share/jupyter/kernels"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join("Library/Jupyter/kernels"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/share/jupyter/kernels"));
        paths.push(PathBuf::from("/usr/share/jupyter/kernels"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/share/jupyter/kernels"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
            paths.push(PathBuf::from(program_data).join("jupyter").join("kernels"));
        }
    }

    paths
}
