//! Sysinternals MCP Server
//!
//! A Model Context Protocol server exposing Sysinternals tools to AI assistants.
//!
//! This server implements manual MCP initialization to support newer protocol versions
//! (like 2025-11-25) that rmcp doesn't yet support.

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout};
use tracing_subscriber::{self, EnvFilter};

use dbgview::{
    DbgViewError, FilterSet, ProcessInfo, RingBuffer, SessionManager, SessionStatus,
};

/// MCP Server for Sysinternals tools
#[derive(Clone)]
pub struct SysinternalsMcpServer {
    session_manager: Arc<SessionManager>,
    tool_router: ToolRouter<Self>,
}

impl SysinternalsMcpServer {
    /// Create a new server instance
    pub fn new() -> Self {
        let buffer = Arc::new(RingBuffer::with_default_capacity());
        let session_manager = Arc::new(SessionManager::new(buffer));
        Self {
            session_manager,
            tool_router: Self::tool_router(),
        }
    }

    /// List available tools for manual protocol handling
    pub fn list_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "create_session",
                "description": "Create a new debug capture session. Returns a session ID that can be used with other tools to get output, set filters, etc. Capture starts automatically when the first session is created.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Optional name for the session"
                        }
                    }
                }
            }),
            json!({
                "name": "destroy_session",
                "description": "Destroy a debug capture session and free its resources.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID"
                        }
                    },
                    "required": ["session_id"]
                }
            }),
            json!({
                "name": "list_sessions",
                "description": "List all active debug capture sessions with their status information.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "get_session_status",
                "description": "Get detailed status of a specific session including filters and pending message count.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID"
                        }
                    },
                    "required": ["session_id"]
                }
            }),
            json!({
                "name": "set_filters",
                "description": "Set include/exclude filters for a session. Include patterns: entries must match at least one. Exclude patterns: matching entries are filtered out. Process filters: filter by process name patterns or specific PIDs. All patterns are regular expressions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID"
                        },
                        "include_patterns": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Include patterns - entry must match at least one (if any specified)"
                        },
                        "exclude_patterns": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Exclude patterns - matching entries are excluded"
                        },
                        "process_names": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Process name patterns - entry must match at least one (if any specified)"
                        },
                        "process_pids": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "description": "Specific process IDs to capture from"
                        }
                    },
                    "required": ["session_id"]
                }
            }),
            json!({
                "name": "get_output",
                "description": "Get captured debug output from a session. Returns filtered entries based on session filters. Each entry includes sequence number, timestamp, process ID, process name, and message text.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of entries to return (default: 100)"
                        }
                    },
                    "required": ["session_id"]
                }
            }),
            json!({
                "name": "clear_session",
                "description": "Clear a session's read position to skip all pending messages. New messages will still be captured.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Session ID"
                        }
                    },
                    "required": ["session_id"]
                }
            }),
            json!({
                "name": "list_processes",
                "description": "List running processes on the system. Optionally filter by process name (case-insensitive substring match). Useful for finding process IDs to filter debug output.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name_filter": {
                            "type": "string",
                            "description": "Optional name filter (case-insensitive substring match)"
                        }
                    }
                }
            })
        ]
    }

    /// Call a tool by name with arguments for manual protocol handling
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
        match name {
            "create_session" => {
                let name = arguments.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                let session = self.session_manager
                    .create_session(name)
                    .map_err(|e| e.to_string())?;
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&CreateSessionResponse {
                            session_id: session.id.clone(),
                            name: session.name.clone(),
                        }).unwrap()
                    }]
                }))
            }
            "destroy_session" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("session_id is required")?;
                self.session_manager
                    .destroy_session(session_id)
                    .map_err(|e| e.to_string())?;
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&DestroySessionResponse {
                            success: true,
                            message: format!("Session {} destroyed", session_id),
                        }).unwrap()
                    }]
                }))
            }
            "list_sessions" => {
                let sessions: Vec<SessionStatus> = self.session_manager.list_sessions();
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&sessions).unwrap()
                    }]
                }))
            }
            "get_session_status" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("session_id is required")?;
                let session = self.session_manager
                    .get_session(session_id)
                    .map_err(|e| e.to_string())?;
                let status = session.status(self.session_manager.is_capture_active());
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&status).unwrap()
                    }]
                }))
            }
            "set_filters" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("session_id is required")?;
                let session = self.session_manager
                    .get_session(session_id)
                    .map_err(|e| e.to_string())?;

                let include_patterns: Vec<String> = arguments.get("include_patterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let exclude_patterns: Vec<String> = arguments.get("exclude_patterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let process_names: Vec<String> = arguments.get("process_names")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let process_pids: Vec<u32> = arguments.get("process_pids")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
                    .unwrap_or_default();

                let filters = FilterSet {
                    include_patterns,
                    exclude_patterns,
                    process_names,
                    process_pids,
                };

                session.set_filters(filters).map_err(|e| e.to_string())?;
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&SetFiltersResponse {
                            success: true,
                            message: "Filters updated".to_string(),
                        }).unwrap()
                    }]
                }))
            }
            "get_output" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("session_id is required")?;
                let limit = arguments.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(100);
                let session = self.session_manager
                    .get_session(session_id)
                    .map_err(|e| e.to_string())?;
                let entries = session.get_output(limit);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&entries).unwrap()
                    }]
                }))
            }
            "clear_session" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("session_id is required")?;
                let session = self.session_manager
                    .get_session(session_id)
                    .map_err(|e| e.to_string())?;
                session.clear();
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&ClearSessionResponse {
                            success: true,
                            message: "Session cleared".to_string(),
                        }).unwrap()
                    }]
                }))
            }
            "list_processes" => {
                let name_filter = arguments.get("name_filter").and_then(|v| v.as_str());
                let processes: Vec<ProcessInfo> = dbgview::list_processes(name_filter);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&processes).unwrap()
                    }]
                }))
            }
            _ => Err(format!("Unknown tool: {}", name))
        }
    }
}

// Tool parameter types

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSessionParams {
    /// Optional name for the session
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionIdParams {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFiltersParams {
    /// Session ID
    pub session_id: String,
    /// Include patterns - entry must match at least one (if any specified)
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Exclude patterns - matching entries are excluded
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Process name patterns - entry must match at least one (if any specified)
    #[serde(default)]
    pub process_names: Vec<String>,
    /// Specific process IDs to capture from
    #[serde(default)]
    pub process_pids: Vec<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetOutputParams {
    /// Session ID
    pub session_id: String,
    /// Maximum number of entries to return (default: 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListProcessesParams {
    /// Optional name filter (case-insensitive substring match)
    #[serde(default)]
    pub name_filter: Option<String>,
}

// Response types

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DestroySessionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SetFiltersResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ClearSessionResponse {
    pub success: bool,
    pub message: String,
}

fn dbgview_error_to_mcp(e: DbgViewError) -> McpError {
    McpError::new(
        ErrorCode::INTERNAL_ERROR,
        e.to_string(),
        None::<serde_json::Value>,
    )
}

#[tool_router]
impl SysinternalsMcpServer {
    /// Create a new debug capture session
    #[tool(description = "Create a new debug capture session. Returns a session ID that can be used with other tools to get output, set filters, etc. Capture starts automatically when the first session is created.")]
    async fn create_session(
        &self,
        Parameters(params): Parameters<CreateSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session_manager
            .create_session(params.name)
            .map_err(dbgview_error_to_mcp)?;

        let response = CreateSessionResponse {
            session_id: session.id.clone(),
            name: session.name.clone(),
        };

        Ok(CallToolResult::success(vec![Content::json(&response)?]))
    }

    /// Destroy a debug capture session
    #[tool(description = "Destroy a debug capture session and free its resources.")]
    async fn destroy_session(
        &self,
        Parameters(params): Parameters<SessionIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.session_manager
            .destroy_session(&params.session_id)
            .map_err(dbgview_error_to_mcp)?;

        let response = DestroySessionResponse {
            success: true,
            message: format!("Session {} destroyed", params.session_id),
        };

        Ok(CallToolResult::success(vec![Content::json(&response)?]))
    }

    /// List all active sessions
    #[tool(description = "List all active debug capture sessions with their status information.")]
    async fn list_sessions(&self) -> Result<CallToolResult, McpError> {
        let sessions: Vec<SessionStatus> = self.session_manager.list_sessions();
        Ok(CallToolResult::success(vec![Content::json(&sessions)?]))
    }

    /// Get session status
    #[tool(description = "Get detailed status of a specific session including filters and pending message count.")]
    async fn get_session_status(
        &self,
        Parameters(params): Parameters<SessionIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session_manager
            .get_session(&params.session_id)
            .map_err(dbgview_error_to_mcp)?;

        let status = session.status(self.session_manager.is_capture_active());
        Ok(CallToolResult::success(vec![Content::json(&status)?]))
    }

    /// Set filters for a session
    #[tool(description = "Set include/exclude filters for a session. Include patterns: entries must match at least one. Exclude patterns: matching entries are filtered out. Process filters: filter by process name patterns or specific PIDs. All patterns are regular expressions.")]
    async fn set_filters(
        &self,
        Parameters(params): Parameters<SetFiltersParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session_manager
            .get_session(&params.session_id)
            .map_err(dbgview_error_to_mcp)?;

        let filters = FilterSet {
            include_patterns: params.include_patterns,
            exclude_patterns: params.exclude_patterns,
            process_names: params.process_names,
            process_pids: params.process_pids,
        };

        session.set_filters(filters).map_err(dbgview_error_to_mcp)?;

        let response = SetFiltersResponse {
            success: true,
            message: "Filters updated".to_string(),
        };

        Ok(CallToolResult::success(vec![Content::json(&response)?]))
    }

    /// Get captured debug output
    #[tool(description = "Get captured debug output from a session. Returns filtered entries based on session filters. Each entry includes sequence number, timestamp, process ID, process name, and message text.")]
    async fn get_output(
        &self,
        Parameters(params): Parameters<GetOutputParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session_manager
            .get_session(&params.session_id)
            .map_err(dbgview_error_to_mcp)?;

        let entries = session.get_output(params.limit);
        Ok(CallToolResult::success(vec![Content::json(&entries)?]))
    }

    /// Clear session output
    #[tool(description = "Clear a session's read position to skip all pending messages. New messages will still be captured.")]
    async fn clear_session(
        &self,
        Parameters(params): Parameters<SessionIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session_manager
            .get_session(&params.session_id)
            .map_err(dbgview_error_to_mcp)?;

        session.clear();

        let response = ClearSessionResponse {
            success: true,
            message: "Session cleared".to_string(),
        };

        Ok(CallToolResult::success(vec![Content::json(&response)?]))
    }

    /// List running processes
    #[tool(description = "List running processes on the system. Optionally filter by process name (case-insensitive substring match). Useful for finding process IDs to filter debug output.")]
    async fn list_processes(
        &self,
        Parameters(params): Parameters<ListProcessesParams>,
    ) -> Result<CallToolResult, McpError> {
        let processes: Vec<ProcessInfo> = dbgview::list_processes(params.name_filter.as_deref());
        Ok(CallToolResult::success(vec![Content::json(&processes)?]))
    }
}

#[tool_handler]
impl ServerHandler for SysinternalsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Sysinternals MCP Server provides Windows system tools for AI assistants. \
                Currently includes DebugView functionality to capture and filter Windows debug \
                output (OutputDebugString). \
                \n\
                Workflow: \
                1. Create a session with create_session. \
                2. If the user mentions a specific process name, application, or PID, \
                   ALWAYS call set_filters with the appropriate process_names or process_pids \
                   BEFORE calling get_output. Use list_processes to find PIDs if needed. \
                3. Call get_output to retrieve filtered debug messages. \
                4. When done, call destroy_session to clean up.".into()
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("sysinternals_mcp=info".parse()?))
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting Sysinternals MCP Server");

    // Use manual initialization to handle newer MCP protocol versions (2025-11-25+)
    // that rmcp doesn't yet support. We handle the initialize handshake ourselves,
    // then create a custom transport for the rest.

    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin);
    let mut stdout = stdout;

    // Read lines until we get the initialize request
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(anyhow::anyhow!("EOF before initialization"));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse as JSON-RPC
        let request: Value = serde_json::from_str(trimmed)?;

        if request.get("method").and_then(|m| m.as_str()) == Some("initialize") {
            tracing::info!("Received initialize request, handling manually");

            let request_id = request.get("id").cloned().unwrap_or(json!(1));
            let client_protocol = request
                .pointer("/params/protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");

            tracing::info!("Client protocol version: {}", client_protocol);

            // Respond with our capabilities using a compatible protocol version
            // We'll use the latest version we know rmcp supports
            let response = json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "sysinternals-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "Sysinternals MCP Server provides Windows system tools for AI assistants. Currently includes DebugView functionality to capture and filter Windows debug output (OutputDebugString).\n\nWorkflow:\n1. Create a session with create_session.\n2. If the user mentions a specific process name, application, or PID, ALWAYS call set_filters with the appropriate process_names or process_pids BEFORE calling get_output. Use list_processes to find PIDs if needed.\n3. Call get_output to retrieve filtered debug messages.\n4. When done, call destroy_session to clean up.\n\nWhen the user asks for debug output from a specific app (e.g. 'get debug output from myapp'), set process_names filter to ['myapp'] so only that app's messages are returned."
                }
            });

            let response_str = serde_json::to_string(&response)?;
            stdout.write_all(response_str.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;

            tracing::info!("Sent initialize response");
            break;
        } else {
            tracing::warn!("Unexpected message before initialize: {}", trimmed);
        }
    }

    // Wait for initialized notification
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(anyhow::anyhow!("EOF before initialized notification"));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = serde_json::from_str(trimmed)?;

        if msg.get("method").and_then(|m| m.as_str()) == Some("notifications/initialized") {
            tracing::info!("Received initialized notification");
            break;
        } else {
            tracing::warn!("Unexpected message during init: {}", trimmed);
        }
    }

    tracing::info!("Initialization complete, starting main server loop");

    // Now run the server with manual message handling
    let server = SysinternalsMcpServer::new();

    // Process remaining messages manually
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            tracing::info!("EOF received, shutting down");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to parse JSON: {}", e);
                continue;
            }
        };

        let method = request.get("method").and_then(|m| m.as_str());
        let request_id = request.get("id").cloned();

        let response = match method {
            Some("tools/list") => {
                // Get tools from the server
                let tools = server.list_tools();
                json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "tools": tools
                    }
                })
            }
            Some("tools/call") => {
                let tool_name = request
                    .pointer("/params/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = request
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or(json!({}));

                match server.call_tool(tool_name, arguments).await {
                    Ok(result) => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "result": result
                        })
                    }
                    Err(e) => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "error": {
                                "code": -32603,
                                "message": e.to_string()
                            }
                        })
                    }
                }
            }
            Some("ping") => {
                json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {}
                })
            }
            Some(m) if m.starts_with("notifications/") => {
                // Notifications don't get responses
                tracing::debug!("Received notification: {}", m);
                continue;
            }
            Some(m) => {
                tracing::warn!("Unknown method: {}", m);
                if request_id.is_some() {
                    json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", m)
                        }
                    })
                } else {
                    continue;
                }
            }
            None => {
                if request_id.is_some() {
                    json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {
                            "code": -32600,
                            "message": "Invalid request: missing method"
                        }
                    })
                } else {
                    continue;
                }
            }
        };

        let response_str = serde_json::to_string(&response)?;
        stdout.write_all(response_str.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    tracing::info!("Server shutting down");
    Ok(())
}
