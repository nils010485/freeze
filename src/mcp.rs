/*!
MCP (Model Context Protocol) server implementation for freeze.

This module implements an MCP server that exposes freeze's functionality
as MCP tools, allowing AI assistants to interact with the freeze snapshot system.
*/

use crate::db::Database;
use crate::snapshot::Snapshot;
use crate::utils::format_size;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct ToolResult {
    content: Vec<ToolContent>,
    is_error: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct ToolContent {
    r#type: String,
    text: String,
}

pub async fn run_server() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut lines = stdin.lock().lines();

    let capabilities = json!({
        "tools": get_tools()
    });

    loop {
        if let Some(Ok(line)) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(request) => {
                    let response = handle_request(&request, &capabilities).await;
                    let response_str = serde_json::to_string(&response)?;
                    writeln!(stdout, "{}", response_str)?;
                    stdout.flush()?;
                }
                Err(e) => {
                    eprintln!("Failed to parse request: {}", e);
                }
            }
        }
    }
}

async fn handle_request(
    request: &JsonRpcRequest,
    capabilities: &serde_json::Value,
) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => {
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": capabilities,
                    "serverInfo": {
                        "name": "freeze",
                        "version": "0.1.1"
                    }
                })),
                error: None,
            }
        }
        "notifications/initialized" => {
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: None,
            }
        }
        "tools/list" => {
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "tools": get_tools()
                })),
                error: None,
            }
        }
        "tools/call" => {
            if let Some(params) = &request.params {
                let result = call_tool(params).await;
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!(result)),
                    error: None,
                }
            } else {
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid Request: missing params".to_string(),
                    }),
                }
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
            }),
        },
    }
}

fn get_tools() -> Vec<serde_json::Value> {
    vec![
        json!({
            "name": "freeze_save",
            "description": "Save a snapshot of a file or directory",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file or directory to snapshot"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeze_restore",
            "description": "Restore a file or directory from a snapshot. Use checksum to specify which snapshot, or leave empty to use the latest",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file or directory to restore"
                    },
                    "checksum": {
                        "type": "string",
                        "description": "Checksum (or partial checksum) of the snapshot to restore. If not provided, uses the latest snapshot"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeze_list",
            "description": "List all snapshots with their IDs and checksums",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {
                        "type": "integer",
                        "description": "Page number (10 items per page)",
                        "default": 1
                    }
                }
            }
        }),
        json!({
            "name": "freeze_list_directory",
            "description": "List snapshots in current directory with IDs",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {
                        "type": "integer",
                        "description": "Page number (10 items per page)",
                        "default": 1
                    }
                }
            }
        }),
        json!({
            "name": "freeze_search",
            "description": "Search snapshots by pattern",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Pattern to search for in snapshot paths"
                    }
                },
                "required": ["pattern"]
            }
        }),
        json!({
            "name": "freeze_check",
            "description": "Check if files have changed since last snapshot",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to check for changes"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeze_view",
            "description": "View the contents of a snapshot. Use checksum to specify which snapshot",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the snapshot to view"
                    },
                    "max_size": {
                        "type": "integer",
                        "description": "Maximum size to display in MB (default: 5)",
                        "default": 5
                    },
                    "checksum": {
                        "type": "string",
                        "description": "Checksum of the snapshot to view (optional, uses latest if not provided)"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeze_export",
            "description": "Export a snapshot to a specified path. Use checksum to specify which snapshot",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "snapshot_path": {
                        "type": "string",
                        "description": "Path of the snapshot to export"
                    },
                    "destination": {
                        "type": "string",
                        "description": "Export destination (optional, defaults to current directory)"
                    },
                    "checksum": {
                        "type": "string",
                        "description": "Checksum of the snapshot to export (optional, uses latest if not provided)"
                    }
                },
                "required": ["snapshot_path"]
            }
        }),
        json!({
            "name": "freeze_clear",
            "description": "Clear snapshots",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "all": {
                        "type": "boolean",
                        "description": "Clear all snapshots",
                        "default": false
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to clear snapshots for (if not clearing all)"
                    }
                }
            }
        }),
        json!({
            "name": "freeze_snapshot_info",
            "description": "Get detailed information about a specific snapshot by checksum",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "checksum": {
                        "type": "string",
                        "description": "Checksum (or partial checksum) of the snapshot"
                    }
                },
                "required": ["checksum"]
            }
        }),
        json!({
            "name": "freeze_compare",
            "description": "Compare two snapshots or a snapshot with current file state. Use checksums to specify snapshots, or 'current' for current state",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the file to compare"
                    },
                    "source": {
                        "type": "string",
                        "description": "Source: checksum, 'current', or leave empty for latest snapshot"
                    },
                    "target": {
                        "type": "string",
                        "description": "Target: checksum, 'current', or leave empty for latest snapshot"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeze_exclusion_add",
            "description": "Add an exclusion pattern",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Pattern to exclude"
                    },
                    "exclusion_type": {
                        "type": "string",
                        "description": "Type of exclusion (directory, extension, file)",
                        "enum": ["directory", "extension", "file"]
                    }
                },
                "required": ["pattern", "exclusion_type"]
            }
        }),
        json!({
            "name": "freeze_exclusion_list",
            "description": "List all exclusion patterns",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "freeze_exclusion_remove",
            "description": "Remove an exclusion pattern",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Pattern to remove"
                    }
                },
                "required": ["pattern"]
            }
        }),
    ]
}

async fn call_tool(params: &serde_json::Value) -> ToolResult {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match name {
        "freeze_save" => freeze_save(&arguments).await,
        "freeze_restore" => freeze_restore(&arguments).await,
        "freeze_list" => freeze_list(&arguments).await,
        "freeze_list_directory" => freeze_list_directory(&arguments).await,
        "freeze_search" => freeze_search(&arguments).await,
        "freeze_check" => freeze_check(&arguments).await,
        "freeze_view" => freeze_view(&arguments).await,
        "freeze_export" => freeze_export(&arguments).await,
        "freeze_clear" => freeze_clear(&arguments).await,
        "freeze_snapshot_info" => freeze_snapshot_info(&arguments).await,
        "freeze_compare" => freeze_compare(&arguments).await,
        "freeze_exclusion_add" => freeze_exclusion_add(&arguments).await,
        "freeze_exclusion_list" => freeze_exclusion_list().await,
        "freeze_exclusion_remove" => freeze_exclusion_remove(&arguments).await,
        _ => ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: format!("Unknown tool: {}", name),
            }],
            is_error: Some(true),
        },
    }
}

async fn freeze_save(args: &serde_json::Value) -> ToolResult {
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
    if path_str.is_none() || path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let path_str = path_str.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&path_str).canonicalize();
        match path {
            Ok(path) => {
                let db = Database::new();
                match db {
                    Ok(db) => {
                        let save_result = Snapshot::save_recursive(&path, &db);
                        match save_result {
                            Ok(_) => format!("Successfully saved snapshot for: {}", path.display()),
                            Err(e) => format!("Error saving snapshot: {}", e),
                        }
                    }
                    Err(e) => format!("Error opening database: {}", e),
                }
            }
            Err(e) => format!("Error resolving path: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error executing save".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_restore(args: &serde_json::Value) -> ToolResult {
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let checksum = args.get("checksum").and_then(|v| v.as_str()).map(|s| s.to_string());

    if path_str.is_none() || path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let path_str = path_str.unwrap();
    let checksum = checksum.clone();
    let result = tokio::task::spawn_blocking(move || {
        let path_buf = PathBuf::from(&path_str);
        let path = if path_buf.is_absolute() {
            path_buf
        } else {
            std::env::current_dir().unwrap_or_default().join(&path_str)
        };

        let db = Database::new().context("Failed to open database")?;
        let snapshots = db.get_snapshots_for_path(&path).context("Failed to get snapshots")?;
        if snapshots.is_empty() {
            return Ok::<String, anyhow::Error>(format!("No snapshots found for: {}", path.display()));
        }

        let target_checksum = if let Some(ref cs) = checksum {
            let matching: Vec<_> = snapshots.iter()
                .filter(|s| s.checksum.starts_with(cs))
                .collect();
            if matching.is_empty() {
                return Ok(format!("No snapshot found with checksum starting with: {}", cs));
            }
            matching[0].checksum.clone()
        } else {
            snapshots[0].checksum.clone()
        };

        let target_snapshot = db.get_snapshot_by_checksum(&target_checksum)?
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found"))?;

        let temp_path = target_snapshot.content_path.clone();
        let content = fs::read(&temp_path).context("Failed to read snapshot content")?;
        
        if target_snapshot.content_path.extension().and_then(|s| s.to_str()) == Some("zstd") {
            let decompressed = zstd::stream::decode_all(&content[..]).context("Failed to decompress")?;
            let final_path = path.with_extension("tmp");
            fs::write(&final_path, &decompressed).context("Failed to write restored file")?;
            fs::rename(&final_path, &path).context("Failed to rename restored file")?;
        } else {
            fs::copy(&temp_path, &path).context("Failed to copy restored file")?;
        }
        Ok(format!("Successfully restored: {} from snapshot {}",
            path.display(),
            &target_checksum[..16]))
    })
    .await;

    match result {
        Ok(Ok(text)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text }],
            is_error: None,
        },
        Ok(Err(e)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
        Err(e) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
    }
}

async fn freeze_list(args: &serde_json::Value) -> ToolResult {
    let page = args.get("page").and_then(|v| v.as_u64()).unwrap_or(1);

    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                let snapshots = db.list_all_snapshots_with_id();
                match snapshots {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            "No snapshots found.".to_string()
                        } else {
                            format_snapshots_list_with_id(&snapshots, Some(page as u32))
                        }
                    }
                    Err(e) => format!("Error listing snapshots: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error listing snapshots".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_list_directory(args: &serde_json::Value) -> ToolResult {
    let page = args.get("page").and_then(|v| v.as_u64()).unwrap_or(1);

    let result = tokio::task::spawn_blocking(move || {
        let current_dir = std::env::current_dir();
        match current_dir {
            Ok(dir) => {
                let db = Database::new();
                match db {
                    Ok(db) => {
                        let snapshots = db.list_current_directory_snapshots_with_id(&dir);
                        match snapshots {
                            Ok(snapshots) => {
                                if snapshots.is_empty() {
                                    format!("No snapshots found in current directory: {}", dir.display())
                                } else {
                                    format_snapshots_list_with_id(&snapshots, Some(page as u32))
                                }
                            }
                            Err(e) => format!("Error listing snapshots: {}", e),
                        }
                    }
                    Err(e) => format!("Error opening database: {}", e),
                }
            }
            Err(e) => format!("Error getting current directory: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error listing directory snapshots".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_search(args: &serde_json::Value) -> ToolResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    if pattern.is_none() || pattern.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: pattern is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let pattern = pattern.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                let snapshots = db.search_snapshots(&pattern);
                match snapshots {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            format!("No snapshots found matching: {}", pattern)
                        } else {
                            format_snapshots_list(&snapshots, None)
                        }
                    }
                    Err(e) => format!("Error searching snapshots: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error searching snapshots".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_check(args: &serde_json::Value) -> ToolResult {
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
    if path_str.is_none() || path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let path_str = path_str.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                let path = PathBuf::from(&path_str).canonicalize();
                match path {
                    Ok(path) => {
                        if path.is_file() {
                            check_single_file(&path, &db)
                        } else {
                            check_directory(&path, &db)
                        }
                    }
                    Err(e) => format!("Error resolving path: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error checking path".to_string()),
        }],
        is_error: None,
    }
}

fn check_single_file(path: &PathBuf, db: &Database) -> String {
    let snapshots = db.get_snapshots_for_path(path).ok();
    if snapshots.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        return format!("{} - No snapshot found", path.display());
    }

    let latest_snapshot = snapshots.as_ref().unwrap().first().unwrap();
    format!(
        "{} - {} (Up to date: {})",
        path.display(),
        latest_snapshot.date,
        &latest_snapshot.checksum[..16]
    )
}

fn check_directory(path: &PathBuf, db: &Database) -> String {
    let all_snapshots = db.list_directory_snapshots(path).ok().unwrap_or_default();
    let snapshot_map: std::collections::HashMap<String, String> = all_snapshots
        .into_iter()
        .map(|(p, _, _, c)| (p.display().to_string(), c))
        .collect();

    let mut result = format!("Checking: {}\n", path.display());
    let walker = walkdir::WalkDir::new(path).into_iter();
    let mut files_checked = 0;
    let mut files_modified = 0;
    let mut files_new = 0;

    for entry in walker.filter_entry(|e| !Snapshot::is_excluded(e.path())) {
        if let Ok(entry) = entry
            && entry.file_type().is_file() {
                let entry_path = entry.path();
                let path_str = entry_path.display().to_string();

                files_checked += 1;
                if let Some(saved_checksum) = snapshot_map.get(&path_str) {
                    if let Ok(content) = fs::read(entry_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&content);
                        let current_checksum = format!("{:x}", hasher.finalize());

                        if &current_checksum != saved_checksum {
                            files_modified += 1;
                            result.push_str(&format!("M - {}\n", entry_path.display()));
                        }
                    }
                } else {
                    files_new += 1;
                    result.push_str(&format!("N - {}\n", entry_path.display()));
                }
            }
    }

    result.push_str(&format!(
        "\nSummary: {} checked, {} modified, {} new",
        files_checked, files_modified, files_new
    ));
    result
}

async fn freeze_view(args: &serde_json::Value) -> ToolResult {
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let max_size = args.get("max_size").and_then(|v| v.as_u64()).unwrap_or(5);
    let checksum = args.get("checksum").and_then(|v| v.as_str()).map(|s| s.to_string());

    if path_str.is_none() || path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let path_str = path_str.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let snapshot_path = PathBuf::from(&path_str);
        let db = Database::new()?;
        let snapshots = db.get_snapshots_for_path(&snapshot_path)?;
        
        if snapshots.is_empty() {
            return Ok::<String, anyhow::Error>(format!("No snapshots found for: {}", snapshot_path.display()));
        }

        let target_checksum = if let Some(ref cs) = checksum {
            let matching: Vec<_> = snapshots.iter()
                .filter(|s| s.checksum.starts_with(cs))
                .collect();
            if matching.is_empty() {
                return Ok(format!("No snapshot found with checksum starting with: {}", cs));
            }
            matching[0].checksum.clone()
        } else {
            snapshots[0].checksum.clone()
        };

        let target_snapshot = db.get_snapshot_by_checksum(&target_checksum)?
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found"))?;

        let metadata = fs::metadata(&target_snapshot.content_path).ok();
        let max_bytes = max_size * 1024 * 1024;

        if let Some(md) = metadata
            && md.len() > max_bytes {
                return Ok(format!(
                    "File too large ({} > {} MB limit)\nPath: {}\nDate: {}\nSize: {}\nChecksum: {}",
                    format_size(md.len() as i64),
                    max_size,
                    target_snapshot.path.display(),
                    target_snapshot.date,
                    format_size(target_snapshot.size),
                    target_snapshot.checksum
                ));
            }

        let content = fs::read(&target_snapshot.content_path).map_err(|e| anyhow::anyhow!("{}", e))?;
        
        if content.iter().take(512).any(|&b| b == 0) {
            return Ok(format!(
                "Binary content detected\nPath: {}\nDate: {}\nSize: {}\nChecksum: {}",
                target_snapshot.path.display(),
                target_snapshot.date,
                format_size(target_snapshot.size),
                target_snapshot.checksum
            ));
        }

        match String::from_utf8(content) {
            Ok(content_str) => Ok(content_str),
            Err(_) => Ok(format!(
                "Unable to decode content\nPath: {}\nDate: {}\nSize: {}",
                target_snapshot.path.display(),
                target_snapshot.date,
                format_size(target_snapshot.size)
            )),
        }
    })
    .await;

    match result {
        Ok(Ok(text)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text }],
            is_error: None,
        },
        Ok(Err(e)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
        Err(e) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
    }
}

async fn freeze_export(args: &serde_json::Value) -> ToolResult {
    let snapshot_path_str = args.get("snapshot_path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let destination = args.get("destination").and_then(|v| v.as_str()).map(|s| s.to_string());
    let checksum = args.get("checksum").and_then(|v| v.as_str()).map(|s| s.to_string());

    if snapshot_path_str.is_none() || snapshot_path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: snapshot_path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let snapshot_path_str = snapshot_path_str.unwrap();
    let destination = destination.clone();
    let checksum = checksum.clone();
    let result = tokio::task::spawn_blocking(move || {
        let snapshot_path = PathBuf::from(&snapshot_path_str).canonicalize()?;
        let db = Database::new()?;
        let snapshots = db.get_snapshots_for_path(&snapshot_path)?;
        
        if snapshots.is_empty() {
            return Ok::<String, anyhow::Error>(format!("No snapshots found for: {}", snapshot_path.display()));
        }

        let target_checksum = if let Some(ref cs) = checksum {
            let matching: Vec<_> = snapshots.iter()
                .filter(|s| s.checksum.starts_with(cs))
                .collect();
            if matching.is_empty() {
                return Ok(format!("No snapshot found with checksum starting from: {}", cs));
            }
            matching[0].checksum.clone()
        } else {
            snapshots[0].checksum.clone()
        };

        let target_snapshot = db.get_snapshot_by_checksum(&target_checksum)?
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found"))?;

        let export_path = match destination.as_ref() {
            Some(dest) => {
                let dest_path = PathBuf::from(dest);
                if dest_path.is_dir() {
                    dest_path.join(
                        target_snapshot.path.file_name()
                            .unwrap_or(std::ffi::OsStr::new(&target_snapshot.checksum))
                    )
                } else if dest.contains('/') || dest.contains('\\') {
                    dest_path
                } else {
                    std::env::current_dir().unwrap_or_default().join(dest)
                }
            }
            None => std::env::current_dir()
                .unwrap_or_default()
                .join(
                    target_snapshot.path.file_name()
                        .unwrap_or(std::ffi::OsStr::new(&target_snapshot.checksum))
                )
        };

        if let Some(parent) = export_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        fs::copy(&target_snapshot.content_path, &export_path)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        
        Ok(format!("Exported to: {}", export_path.display()))
    })
    .await;

    match result {
        Ok(Ok(text)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text }],
            is_error: None,
        },
        Ok(Err(e)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
        Err(e) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
    }
}

async fn freeze_clear(args: &serde_json::Value) -> ToolResult {
    let clear_all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                if clear_all {
                    match db.clear_all_snapshots() {
                        Ok(_) => "Cleared all snapshots".to_string(),
                        Err(e) => format!("Error clearing snapshots: {}", e),
                    }
                } else if let Some(path) = path_str {
                    let path_buf = PathBuf::from(path);
                    match path_buf.canonicalize() {
                        Ok(abs_path) => {
                            match db.clear_snapshots(&abs_path) {
                                Ok(_) => format!("Cleared snapshots for: {}", abs_path.display()),
                                Err(e) => format!("Error clearing snapshots: {}", e),
                            }
                        }
                        Err(e) => format!("Error resolving path: {}", e),
                    }
                } else {
                    "Error: either 'all' or 'path' must be specified".to_string()
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error clearing snapshots".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_snapshot_info(args: &serde_json::Value) -> ToolResult {
    let checksum = args.get("checksum").and_then(|v| v.as_str()).map(|s| s.to_string());

    if checksum.is_none() || checksum.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: checksum is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let checksum = checksum.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                let snapshot = db.get_snapshot_by_checksum(&checksum);
                match snapshot {
                    Ok(Some(snapshot)) => {
                        format!(
                            "Snapshot Information:\n\
                             Path: {}\n\
                             Date: {}\n\
                             Size: {}\n\
                             Checksum: {}",
                            snapshot.path.display(),
                            snapshot.date,
                            format_size(snapshot.size),
                            snapshot.checksum
                        )
                    }
                    Ok(None) => format!("No snapshot found with checksum: {}", checksum),
                    Err(e) => format!("Error getting snapshot: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error getting snapshot info".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_compare(args: &serde_json::Value) -> ToolResult {
    let path_str = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let source = args.get("source").and_then(|v| v.as_str()).map(|s| s.to_string());
    let target = args.get("target").and_then(|v| v.as_str()).map(|s| s.to_string());

    if path_str.is_none() || path_str.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: path is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let path_str = path_str.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&path_str);
        let db = Database::new()?;
        let snapshots = db.get_snapshots_for_path(&path).unwrap_or_default();
        
        if snapshots.is_empty() {
            return Ok::<String, anyhow::Error>(format!("No snapshots found for: {}", path.display()));
        }

        let get_content = |path: &PathBuf, snapshot: Option<&Snapshot>, is_current: bool| -> Option<(String, Vec<u8>)> {
            if is_current {
                if path.exists() {
                    fs::read(path).ok().map(|c| ("current".to_string(), c))
                } else {
                    None
                }
            } else if let Some(snap) = snapshot {
                fs::read(&snap.content_path).ok().map(|c| (snap.checksum.clone(), c))
            } else {
                None
            }
        };

        let source_snapshot = match source.as_deref() {
            Some("current") => None,
            Some(cs) => snapshots.iter().find(|s| s.checksum.starts_with(cs)),
            None => snapshots.first(),
        };

        let target_snapshot = match target.as_deref() {
            Some("current") => None,
            Some(cs) => snapshots.iter().find(|s| s.checksum.starts_with(cs)),
            None => snapshots.get(1).or(snapshots.first()),
        };

        let source_name = if source.as_deref() == Some("current") {
            "current".to_string()
        } else {
            source_snapshot.map(|s| s.checksum[..16].to_string()).unwrap_or_else(|| "unknown".to_string())
        };

        let target_name = if target.as_deref() == Some("current") {
            "current".to_string()
        } else {
            target_snapshot.map(|s| s.checksum[..16].to_string()).unwrap_or_else(|| "unknown".to_string())
        };

        let source_content = get_content(&path, source_snapshot, source == Some("current".to_string()));
        let target_content = get_content(&path, target_snapshot, target == Some("current".to_string()));

        match (source_content, target_content) {
            (Some((_, source_bytes)), Some((_, target_bytes))) => {
                let mut source_hasher = Sha256::new();
                source_hasher.update(&source_bytes);
                let source_hash = format!("{:x}", source_hasher.finalize());

                let mut target_hasher = Sha256::new();
                target_hasher.update(&target_bytes);
                let target_hash = format!("{:x}", target_hasher.finalize());

                if source_hash == target_hash {
                    Ok(format!("Comparison: {} vs {} - IDENTICAL\nBoth have checksum: {}",
                        source_name, target_name, &source_hash[..16]))
                } else {
                    let source_size = source_bytes.len();
                    let target_size = target_bytes.len();
                    Ok(format!("Comparison: {} vs {} - DIFFERENT\n\
                             {} size: {} bytes, checksum: {}\n\
                             {} size: {} bytes, checksum: {}",
                        source_name, target_name,
                        source_name, source_size, &source_hash[..16],
                        target_name, target_size, &target_hash[..16]))
                }
            }
            (Some(_), None) => Ok(format!("Target not found: {}", target_name)),
            (None, Some(_)) => Ok(format!("Source not found: {}", source_name)),
            (None, None) => Ok("Both source and target not found".to_string()),
        }
    })
    .await;

    match result {
        Ok(Ok(text)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text }],
            is_error: None,
        },
        Ok(Err(e)) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
        Err(e) => ToolResult {
            content: vec![ToolContent { r#type: "text".to_string(), text: format!("Error: {}", e) }],
            is_error: Some(true),
        },
    }
}

async fn freeze_exclusion_add(args: &serde_json::Value) -> ToolResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    let exclusion_type = args.get("exclusion_type").and_then(|v| v.as_str()).map(|s| s.to_string());

    if pattern.is_none() || exclusion_type.is_none() ||
       pattern.as_ref().unwrap().is_empty() || exclusion_type.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: pattern and exclusion_type are required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let pattern = pattern.unwrap();
    let exclusion_type = exclusion_type.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                match db.add_exclusion(&pattern, &exclusion_type) {
                    Ok(_) => format!("Added exclusion: {} ({})", pattern, exclusion_type),
                    Err(e) => format!("Error adding exclusion: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error adding exclusion".to_string()),
        }],
        is_error: None,
    }
}

async fn freeze_exclusion_list() -> ToolResult {
    let result = tokio::task::spawn_blocking(|| {
        let db = Database::new();
        match db {
            Ok(db) => {
                let exclusions = db.list_exclusions();
                match exclusions {
                    Ok(exclusions) => {
                        if exclusions.is_empty() {
                            "No exclusions configured.".to_string()
                        } else {
                            let mut result = String::from("Exclusions:\n");
                            result.push_str("─".repeat(50).as_str());
                            result.push('\n');
                            for (pattern, exc_type) in exclusions {
                                result.push_str(&format!("  - {} ({})\n", pattern, exc_type));
                            }
                            result
                        }
                    }
                    Err(e) => format!("Error listing exclusions: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error listing exclusions".to_string()),
        }],
        is_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_snapshots_list_with_id() {
        let snapshots = vec![
            (1, PathBuf::from("/test/file1.txt"), "2024-01-15T10:30:00+00:00".to_string(), 1024, "abc123def4567890".to_string()),
            (2, PathBuf::from("/test/file2.txt"), "2024-01-15T11:30:00+00:00".to_string(), 2048, "def456ghi7890123".to_string()),
        ];
        
        let result = format_snapshots_list_with_id(&snapshots, None);
        assert!(result.contains("ID"));
        assert!(result.contains("abc123def4567890"));
        assert!(result.contains("file1.txt"));
    }

    #[test]
    fn test_format_snapshots_list_with_id_pagination() {
        let snapshots: Vec<(i64, PathBuf, String, i64, String)> = (1..=25)
            .map(|i| (i, PathBuf::from(format!("/test/file{}.txt", i)), "2024-01-15T10:00:00+00:00".to_string(), 1024, format!("checksum{:12}", i)))
            .collect();
        
        let page1 = format_snapshots_list_with_id(&snapshots, Some(1));
        let page2 = format_snapshots_list_with_id(&snapshots, Some(2));
        let page3 = format_snapshots_list_with_id(&snapshots, Some(3));
        
        assert!(page1.contains("ID"));
        assert!(page2.contains("Page 2 of 3"));
        assert!(page3.contains("Page 3 of 3"));
    }

    #[test]
    fn test_format_snapshots_list() {
        let snapshots = vec![
            (PathBuf::from("/test/file1.txt"), "2024-01-15T10:30:00+00:00".to_string(), 1024, "abc123def4567890".to_string()),
            (PathBuf::from("/test/file2.txt"), "2024-01-15T11:30:00+00:00".to_string(), 2048, "def456ghi7890123".to_string()),
        ];
        
        let result = format_snapshots_list(&snapshots, None);
        assert!(result.contains("Snapshots:"));
        assert!(result.contains("file1.txt"));
        assert!(result.contains("abc123def4567890"));
    }

    #[test]
    fn test_format_snapshots_list_empty() {
        let snapshots: Vec<(PathBuf, String, i64, String)> = vec![];
        let result = format_snapshots_list(&snapshots, Some(1));
        // When page is provided and list is empty, it should still show the header
        // but no items
        assert!(result.contains("Snapshots:"));
    }

    #[test]
    fn test_format_snapshots_list_with_id_empty() {
        let snapshots: Vec<(i64, PathBuf, String, i64, String)> = vec![];
        let result = format_snapshots_list_with_id(&snapshots, Some(1));
        // When page is provided and list is empty, it should still show the header
        assert!(result.contains("Snapshots:"));
    }

    #[test]
    fn test_json_rpc_request_parse() {
        let json = r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(serde_json::Value::Number(1.into())));
        assert_eq!(request.method, "initialize");
    }

    #[test]
    fn test_json_rpc_request_with_params() {
        let json = r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "freeze_list"}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.method, "tools/call");
        assert!(request.params.is_some());
    }

    #[test]
    fn test_json_rpc_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(json!({"tools": []})),
            error: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("2.0"));
        assert!(json.contains("tools"));
    }

    #[test]
    fn test_json_rpc_error_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid Request".to_string(),
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Invalid Request"));
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Test message".to_string(),
            }],
            is_error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error occurred".to_string(),
            }],
            is_error: Some(true),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("true"));
        assert!(json.contains("Error occurred"));
    }

    #[test]
    fn test_get_tools_returns_all_tools() {
        let tools = get_tools();
        assert!(tools.len() >= 10);
        
        let tool_names: Vec<&str> = tools.iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        
        assert!(tool_names.contains(&"freeze_save"));
        assert!(tool_names.contains(&"freeze_restore"));
        assert!(tool_names.contains(&"freeze_list"));
        assert!(tool_names.contains(&"freeze_view"));
        assert!(tool_names.contains(&"freeze_compare"));
    }

    #[test]
    fn test_freeze_restore_tool_schema() {
        let tools = get_tools();
        let restore_tool = tools.iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("freeze_restore"))
            .unwrap();
        
        let schema = restore_tool.get("inputSchema").unwrap();
        let props = schema.get("properties").unwrap();
        
        assert!(props.get("path").is_some());
        assert!(props.get("checksum").is_some());
        
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("path")));
    }

    #[test]
    fn test_freeze_view_tool_schema() {
        let tools = get_tools();
        let view_tool = tools.iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("freeze_view"))
            .unwrap();
        
        let schema = view_tool.get("inputSchema").unwrap();
        let props = schema.get("properties").unwrap();
        
        assert!(props.get("path").is_some());
        assert!(props.get("max_size").is_some());
        assert!(props.get("checksum").is_some());
    }

    #[test]
    fn test_freeze_compare_tool_schema() {
        let tools = get_tools();
        let compare_tool = tools.iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("freeze_compare"))
            .unwrap();
        
        let schema = compare_tool.get("inputSchema").unwrap();
        let props = schema.get("properties").unwrap();
        
        assert!(props.get("path").is_some());
        assert!(props.get("source").is_some());
        assert!(props.get("target").is_some());
    }

    #[test]
    fn test_freeze_snapshot_info_tool_schema() {
        let tools = get_tools();
        let info_tool = tools.iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("freeze_snapshot_info"))
            .unwrap();
        
        let schema = info_tool.get("inputSchema").unwrap();
        let props = schema.get("properties").unwrap();
        
        assert!(props.get("checksum").is_some());
        
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("checksum")));
    }

    #[test]
    fn test_freeze_exclusion_add_tool_schema() {
        let tools = get_tools();
        let add_tool = tools.iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("freeze_exclusion_add"))
            .unwrap();
        
        let schema = add_tool.get("inputSchema").unwrap();
        let props = schema.get("properties").unwrap();
        
        let exclusion_type = props.get("exclusion_type").unwrap();
        let enum_values = exclusion_type.get("enum").unwrap().as_array().unwrap();
        
        assert!(enum_values.iter().any(|v| v.as_str() == Some("directory")));
        assert!(enum_values.iter().any(|v| v.as_str() == Some("extension")));
        assert!(enum_values.iter().any(|v| v.as_str() == Some("file")));
    }
}

async fn freeze_exclusion_remove(args: &serde_json::Value) -> ToolResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    if pattern.is_none() || pattern.as_ref().unwrap().is_empty() {
        return ToolResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: "Error: pattern is required".to_string(),
            }],
            is_error: Some(true),
        };
    }

    let pattern = pattern.unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let db = Database::new();
        match db {
            Ok(db) => {
                match db.remove_exclusion(&pattern) {
                    Ok(_) => format!("Removed exclusion: {}", pattern),
                    Err(e) => format!("Error removing exclusion: {}", e),
                }
            }
            Err(e) => format!("Error opening database: {}", e),
        }
    })
    .await;

    ToolResult {
        content: vec![ToolContent {
            r#type: "text".to_string(),
            text: result.unwrap_or_else(|_| "Error removing exclusion".to_string()),
        }],
        is_error: None,
    }
}

fn format_snapshots_list_with_id(
    snapshots: &[(i64, PathBuf, String, i64, String)],
    page: Option<u32>,
) -> String {
    const ITEMS_PER_PAGE: usize = 10;

    let mut result = String::from("Snapshots:\n");
    result.push_str("─".repeat(50).as_str());
    result.push('\n');
    result.push_str("ID      | Date/Time                      | Size      | Checksum            | Path\n");
    result.push_str("─".repeat(80).as_str());
    result.push('\n');

    let snapshots_iter: Vec<_> = snapshots.iter().collect();
    let total = snapshots_iter.len();

    let page_num = page.unwrap_or(1) as usize;
    let total_pages = total.div_ceil(ITEMS_PER_PAGE);
    let start = (page_num - 1) * ITEMS_PER_PAGE;
    let end = std::cmp::min(start + ITEMS_PER_PAGE, total);

    if page_num > total_pages && total > 0 {
        return format!("Invalid page number. Total pages: {}", total_pages);
    }

    let page_snapshots: Vec<_> = if page.is_some() {
        snapshots_iter[start..end].to_vec()
    } else {
        snapshots_iter
    };

    for (id, path, date, size, checksum) in page_snapshots {
        let date_short = if date.len() > 22 { &date[..22] } else { date };
        let file_name = path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        result.push_str(&format!(
            "{:6}  | {:28} | {:>8}  | {:16} | {}",
            id,
            date_short,
            format_size(*size),
            &checksum[..16],
            file_name
        ));
        result.push('\n');
    }

    if page.is_some() {
        result.push_str("─".repeat(80).as_str());
        result.push('\n');
        result.push_str(&format!(
            "Page {} of {} ({} items)\n",
            page_num, total_pages, total
        ));
        result.push_str("Use checksum prefix with restore/view/export to select specific snapshot\n");
    }

    result
}

fn format_snapshots_list(
    snapshots: &[(PathBuf, String, i64, String)],
    page: Option<u32>,
) -> String {
    const ITEMS_PER_PAGE: usize = 10;

    let mut result = String::from("Snapshots:\n");
    result.push_str("─".repeat(50).as_str());
    result.push('\n');

    let snapshots_iter: Vec<_> = snapshots.iter().collect();
    let total = snapshots_iter.len();

    let page_num = page.unwrap_or(1) as usize;
    let total_pages = total.div_ceil(ITEMS_PER_PAGE);
    let start = (page_num - 1) * ITEMS_PER_PAGE;
    let end = std::cmp::min(start + ITEMS_PER_PAGE, total);

    if page_num > total_pages && total > 0 {
        return format!("Invalid page number. Total pages: {}", total_pages);
    }

    let page_snapshots: Vec<_> = if page.is_some() {
        snapshots_iter[start..end].to_vec()
    } else {
        snapshots_iter
    };

    for (path, date, size, checksum) in page_snapshots {
        result.push_str(&format!(
            "📁 {}\n  📅 {} | 💾 {} | 🔐 {}\n",
            path.display(),
            date,
            format_size(*size),
            &checksum[..16]
        ));
    }

    if page.is_some() {
        result.push_str("─".repeat(50).as_str());
        result.push('\n');
        result.push_str(&format!(
            "Page {} of {} ({} items)\n",
            page_num, total_pages, total
        ));
    }

    result
}
