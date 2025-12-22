use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::app_credentials::load_anthropic_api_key;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tauri::Emitter;

#[derive(Serialize, Deserialize)]
struct AgentChatRequest {
    session_id: String,
    message: String,
    api_key: String,
    project_root: Option<String>,
}

// Global map to track active requests with cancellation handles
pub type ActiveRequests = Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>;

/// Send a message to the AI agent and get a response
/// This calls the engine server's /agent/chat endpoint
/// Emits events to the app handle for real-time streaming
pub async fn send_message(
    port: u16,
    session_id: String,
    message: String,
    project_root: Option<String>,
    active_requests: ActiveRequests,
    app_handle: tauri::AppHandle,
) -> Result<String> {
    // Get API key from encrypted storage
    let api_key = load_anthropic_api_key()
        .map_err(|e| anyhow::anyhow!("Failed to load API key: {}", e))?
        .context("No Anthropic API key found. Please add one in Settings.")?;

    // Create client with longer timeout for agent requests
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

    let request_body = AgentChatRequest {
        session_id: session_id.clone(),
        message,
        api_key,
        project_root,
    };

    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = oneshot::channel();

    // Store the cancellation sender
    {
        let mut requests = active_requests.lock().await;
        requests.insert(session_id.clone(), cancel_tx);
    }

    // Call engine server's agent endpoint using the dynamic port
    let url = format!("http://127.0.0.1:{}/agent/chat", port);
    log::info!("Sending agent request to: {}", url);

    let response_future = client
        .post(&url)
        .json(&request_body)
        .send();

    // Race between the request and cancellation
    let response = tokio::select! {
        result = response_future => {
            result.context("Failed to send request to agent")?
        }
        _ = &mut cancel_rx => {
            // Clean up the active request
            active_requests.lock().await.remove(&session_id);
            return Err(anyhow::anyhow!("Request cancelled by user"));
        }
    };

    log::info!("Agent response status: {}", response.status());

    if !response.status().is_success() {
        active_requests.lock().await.remove(&session_id);
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow::anyhow!("Agent request failed: {}", error_text));
    }

    // Stream the response and collect it
    let mut full_response = String::new();
    let mut event_source = response.bytes_stream();

    use futures_util::StreamExt;

    log::info!("Starting to read agent response stream...");
    let mut chunk_count = 0;

    loop {
        tokio::select! {
            chunk_opt = event_source.next() => {
                match chunk_opt {
                    None => {
                        // Stream ended
                        log::info!("Agent stream ended. Received {} chunks. Total response length: {}", chunk_count, full_response.len());
                        active_requests.lock().await.remove(&session_id);
                        return Ok(full_response);
                    }
                    Some(chunk_result) => {
                        chunk_count += 1;

                        let chunk = match chunk_result {
                            Ok(c) => c,
                            Err(e) => {
                                log::error!("Failed to read chunk {} from agent stream: {}", chunk_count, e);
                                active_requests.lock().await.remove(&session_id);
                                return Err(anyhow::anyhow!("Failed to read response chunk {}: {}", chunk_count, e));
                            }
                        };

                        let text = String::from_utf8_lossy(&chunk);
                        log::debug!("Received chunk {}: {} bytes", chunk_count, chunk.len());

                        // Parse Server-Sent Events
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                                        log::debug!("Event type: {}", event_type);
                                        match event_type {
                                            "chunk" => {
                                                if let Some(content) = event.get("content").and_then(|v| v.as_str()) {
                                                    full_response.push_str(content);

                                                    // Emit event to frontend for real-time streaming
                                                    let event_name = format!("agent-stream-{}", session_id);
                                                    #[derive(Clone, serde::Serialize)]
                                                    struct ChunkPayload {
                                                        content: String,
                                                    }
                                                    let _ = app_handle.emit(&event_name, ChunkPayload {
                                                        content: content.to_string(),
                                                    });
                                                }
                                            }
                                            "complete" => {
                                                // Done streaming - emit complete event
                                                let event_name = format!("agent-stream-{}", session_id);
                                                #[derive(Clone, serde::Serialize)]
                                                struct CompletePayload {
                                                    complete: bool,
                                                }
                                                let _ = app_handle.emit(&event_name, CompletePayload {
                                                    complete: true,
                                                });

                                                log::info!("Agent stream complete. Total response length: {}", full_response.len());
                                                active_requests.lock().await.remove(&session_id);
                                                return Ok(full_response);
                                            }
                                            "error" => {
                                                let error_msg = event.get("message")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("Unknown error");

                                                // Emit error event
                                                let event_name = format!("agent-stream-{}", session_id);
                                                #[derive(Clone, serde::Serialize)]
                                                struct ErrorPayload {
                                                    error: String,
                                                }
                                                let _ = app_handle.emit(&event_name, ErrorPayload {
                                                    error: error_msg.to_string(),
                                                });

                                                log::error!("Agent returned error: {}", error_msg);
                                                active_requests.lock().await.remove(&session_id);
                                                return Err(anyhow::anyhow!("Agent error: {}", error_msg));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ = &mut cancel_rx => {
                // Request cancelled
                log::info!("Agent request cancelled by user");
                active_requests.lock().await.remove(&session_id);
                return Err(anyhow::anyhow!("Request cancelled by user"));
            }
        }
    }
}

// Tauri command
#[tauri::command]
pub async fn send_agent_message(
    session_id: String,
    message: String,
    project_root: Option<String>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<String, String> {
    // Get the port from the engine server
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    send_message(
        port,
        session_id,
        message,
        project_root,
        state.active_agent_requests.clone(),
        app_handle,
    )
    .await
    .map_err(|e| e.to_string())
}

// Tauri command to cancel an active agent request
#[tauri::command]
pub async fn cancel_agent_request(
    session_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    let mut requests = state.active_agent_requests.lock().await;

    if let Some(cancel_tx) = requests.remove(&session_id) {
        // Send cancellation signal
        let _ = cancel_tx.send(());
        log::info!("Cancelled agent request for session: {}", session_id);
        Ok(())
    } else {
        Err("No active request found for this session".to_string())
    }
}
