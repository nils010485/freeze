// src/web/api.rs - Simplified API handlers
use crate::snapshot::Snapshot;
use crate::utils::format_size;
use crate::web::server::AppState;
use axum::{response::Json, extract::State};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
pub struct SnapshotDto {
    pub id: i64,
    pub path: String,
    pub checksum: String,
    pub date: String,
    pub size: i64,
    pub size_formatted: String,
}

#[derive(Serialize)]
pub struct ExclusionDto {
    pub id: i64,
    pub pattern: String,
    pub exclusion_type: String,
}

#[derive(Serialize)]
pub struct StatsDto {
    pub total_snapshots: i64,
    pub total_storage: i64,
    pub storage_formatted: String,
    pub total_exclusions: i64,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub err: Option<String>,
}

impl<T> From<Result<T, String>> for ApiResponse<T> {
    fn from(res: Result<T, String>) -> Self {
        match res {
            Ok(data) => ApiResponse { ok: true, data: Some(data), err: None },
            Err(err) => ApiResponse { ok: false, data: None, err: Some(err) },
        }
    }
}

pub async fn api_list_snapshots(State(app_state): State<AppState>) -> Json<Vec<SnapshotDto>> {
    let db = app_state.0.lock().unwrap();
    let snapshots = db.list_all_snapshots_with_id().unwrap_or_default();
    drop(db);
    let result: Vec<SnapshotDto> = snapshots
        .into_iter()
        .map(|(id, path, date, size, checksum)| SnapshotDto {
            id,
            path: path.to_string_lossy().to_string(),
            checksum,
            date,
            size,
            size_formatted: format_size(size),
        })
        .collect();
    Json(result)
}

pub async fn api_search_snapshots(
    State(app_state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<Vec<SnapshotDto>> {
    let pattern = params.get("q").cloned().unwrap_or_default();
    let db = app_state.0.lock().unwrap();
    let results = db.search_snapshots(&pattern).unwrap_or_default();
    let all_with_id = db.list_all_snapshots_with_id().unwrap_or_default();
    drop(db);
    let path_to_id: std::collections::HashMap<String, i64> = all_with_id
        .iter()
        .map(|(id, path, _, _, _)| (path.to_string_lossy().to_string(), *id))
        .collect();

    let result: Vec<SnapshotDto> = results
        .into_iter()
        .map(|(path, date, size, checksum)| {
            let path_str = path.to_string_lossy().to_string();
            SnapshotDto {
                id: path_to_id.get(&path_str).copied().unwrap_or(0),
                path: path_str,
                checksum,
                date,
                size,
                size_formatted: format_size(size),
            }
        })
        .collect();
    Json(result)
}

pub async fn api_get_snapshot(State(app_state): State<AppState>, axum::extract::Path(id): axum::extract::Path<i64>) -> Json<Option<SnapshotDto>> {
    let db = app_state.0.lock().unwrap();
    let snapshot = db.get_snapshot_by_id(id).ok().flatten();
    drop(db);
    Json(snapshot.map(|s| SnapshotDto {
        id,
        path: s.path.to_string_lossy().to_string(),
        checksum: s.checksum,
        date: s.date,
        size: s.size,
        size_formatted: format_size(s.size),
    }))
}

pub async fn api_create_snapshot(State(app_state): State<AppState>, Json(input): Json<CreateSnapshotInput>) -> Json<ApiResponse<SnapshotDto>> {
    // Expand tilde to home directory
    let expanded_path = if input.path.starts_with("~/") {
        match std::env::var("HOME") {
            Ok(home) => input.path.replacen("~", &home, 1),
            Err(_) => input.path,
        }
    } else {
        input.path
    };
    let path = PathBuf::from(&expanded_path);
    let db = app_state.0.lock().unwrap();
    match Snapshot::save_recursive(&path, &db) {
        Ok(_) => {
            let snapshots = db.get_snapshots_for_path_with_id(&path).unwrap_or_default();
            if let Some((id, path, date, size, checksum)) = snapshots.first() {
                let dto = SnapshotDto {
                    id: *id,
                    path: path.to_string_lossy().to_string(),
                    checksum: checksum.clone(),
                    date: date.clone(),
                    size: *size,
                    size_formatted: format_size(*size),
                };
                Json(ApiResponse { ok: true, data: Some(dto), err: None })
            } else {
                Json(ApiResponse { ok: false, data: None, err: Some("Snapshot created but not found".to_string()) })
            }
        }
        Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(e.to_string()) }),
    }
}

pub async fn api_restore_snapshot(State(app_state): State<AppState>, axum::extract::Path(id): axum::extract::Path<i64>) -> Json<ApiResponse<()>> {
    let db = app_state.0.lock().unwrap();
    if let Some(snapshot) = db.get_snapshot_by_id(id).ok().flatten() {
        match Snapshot::restore(&snapshot.path, &db) {
            Ok(_) => Json(ApiResponse { ok: true, data: Some(()), err: None }),
            Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(e.to_string()) }),
        }
    } else {
        Json(ApiResponse { ok: false, data: None, err: Some("Snapshot not found".to_string()) })
    }
}

pub async fn api_delete_snapshot(State(app_state): State<AppState>, axum::extract::Path(id): axum::extract::Path<i64>) -> Json<ApiResponse<()>> {
    let db = app_state.0.lock().unwrap();
    match db.delete_snapshot(id) {
        Ok(_) => Json(ApiResponse { ok: true, data: Some(()), err: None }),
        Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(e.to_string()) }),
    }
}

pub async fn api_list_exclusions(State(app_state): State<AppState>) -> Json<Vec<ExclusionDto>> {
    let db = app_state.0.lock().unwrap();
    let exclusions = db.list_exclusions().unwrap_or_default();
    drop(db);
    let result: Vec<ExclusionDto> = exclusions
        .into_iter()
        .enumerate()
        .map(|(idx, (pattern, exclusion_type))| ExclusionDto {
            id: idx as i64 + 1,
            pattern,
            exclusion_type,
        })
        .collect();
    Json(result)
}

pub async fn api_add_exclusion(State(app_state): State<AppState>, Json(input): Json<AddExclusionInput>) -> Json<ApiResponse<ExclusionDto>> {
    let db = app_state.0.lock().unwrap();
    match db.add_exclusion(&input.pattern, &input.exclusion_type) {
        Ok(_) => {
            let dto = ExclusionDto {
                id: 0,
                pattern: input.pattern,
                exclusion_type: input.exclusion_type,
            };
            Json(ApiResponse { ok: true, data: Some(dto), err: None })
        }
        Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(e.to_string()) }),
    }
}

pub async fn api_remove_exclusion(State(app_state): State<AppState>, axum::extract::Path(pattern): axum::extract::Path<String>) -> Json<ApiResponse<()>> {
    let db = app_state.0.lock().unwrap();
    match db.remove_exclusion(&pattern) {
        Ok(_) => Json(ApiResponse { ok: true, data: Some(()), err: None }),
        Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(e.to_string()) }),
    }
}

pub async fn api_get_stats(State(app_state): State<AppState>) -> Json<StatsDto> {
    let db = app_state.0.lock().unwrap();
    let snapshots = db.list_all_snapshots_with_id().unwrap_or_default();
    let total_storage: i64 = snapshots.iter().map(|(_, _, _, size, _)| *size).sum();
    let exclusions = db.list_exclusions().unwrap_or_default();
    drop(db);

    Json(StatsDto {
        total_snapshots: snapshots.len() as i64,
        total_storage,
        storage_formatted: format_size(total_storage),
        total_exclusions: exclusions.len() as i64,
    })
}

#[derive(Deserialize)]
pub struct ExportInput {
    pub destination: Option<String>,
}

pub async fn api_export_snapshot(State(app_state): State<AppState>, axum::extract::Path(id): axum::extract::Path<i64>, Json(input): Json<ExportInput>) -> Json<ApiResponse<()>> {
    use std::env;

    let db = app_state.0.lock().unwrap();
    let snapshot = db.get_snapshot_by_id(id).ok().flatten();
    drop(db);

    if let Some(s) = snapshot {
        // Determine destination path
        let dest_path = match input.destination {
            Some(dest) => {
                if dest.starts_with("~/") {
                    let home = env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
                    PathBuf::from(dest.replacen("~", &home, 1))
                } else {
                    PathBuf::from(dest)
                }
            }
            None => {
                // Default: export to current directory with original filename
                let file_name = s.path.file_name().unwrap_or_default();
                env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(file_name)
            }
        };

        // Create parent directories if needed
        if let Some(parent) = dest_path.parent()
            && let Err(e) = std::fs::create_dir_all(parent) {
                return Json(ApiResponse { ok: false, data: None, err: Some(format!("Failed to create directories: {}", e)) });
            }

        // Use streaming export
        match s.export(&dest_path) {
            Ok(_) => Json(ApiResponse { ok: true, data: Some(()), err: None }),
            Err(e) => Json(ApiResponse { ok: false, data: None, err: Some(format!("Failed to export: {}", e)) }),
        }
    } else {
        Json(ApiResponse { ok: false, data: None, err: Some("Snapshot not found".to_string()) })
    }
}

#[derive(Deserialize)]
pub struct DiffInput {
    pub first: String,
    pub second: String,
}

pub async fn api_diff_snapshots(State(app_state): State<AppState>, Json(input): Json<DiffInput>) -> Json<ApiResponse<String>> {
    let db = app_state.0.lock().unwrap();

    // Find first snapshot
    let first_snapshot = if input.first.len() == 64 && input.first.chars().all(|c| c.is_ascii_hexdigit()) {
        db.get_snapshot_by_checksum(&input.first).ok().flatten()
    } else {
        let path = PathBuf::from(&input.first);
        let snapshots: Vec<Snapshot> = db.get_snapshots_for_path(&path).ok().unwrap_or_default();
        snapshots.into_iter().last()
    };

    // Find second snapshot
    let second_snapshot = if input.second.len() == 64 && input.second.chars().all(|c| c.is_ascii_hexdigit()) {
        db.get_snapshot_by_checksum(&input.second).ok().flatten()
    } else {
        let path = PathBuf::from(&input.second);
        let snapshots: Vec<Snapshot> = db.get_snapshots_for_path(&path).ok().unwrap_or_default();
        snapshots.into_iter().last()
    };

    drop(db);

    let (first, second) = match (first_snapshot, second_snapshot) {
        (Some(f), Some(s)) => (f, s),
        _ => return Json(ApiResponse { ok: false, data: None, err: Some("Could not find both snapshots".to_string()) }),
    };

    // Check sizes to prevent OOM
    // 5MB limit for diff
    const MAX_DIFF_SIZE: i64 = 5 * 1024 * 1024;
    
    if first.size > MAX_DIFF_SIZE || second.size > MAX_DIFF_SIZE {
        return Json(ApiResponse { 
            ok: false, 
            data: None, 
            err: Some(format!("Files too large for diff (limit {} MB)", MAX_DIFF_SIZE / 1024 / 1024)) 
        });
    }

    // Extract file names before moving snapshots
    let first_name = first.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "first".to_string());
    let second_name = second.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "second".to_string());

    // Read and decompress both contents
    let read_content = |snapshot: Snapshot| -> Result<Vec<u8>, String> {
        if !snapshot.content_path.exists() {
            return Err("Content file not found".to_string());
        }
        match snapshot.get_decompressed_content() {
            Ok(d) => Ok(d),
            Err(e) => Err(e.to_string()),
        }
    };

    let first_content = match read_content(first) {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse { ok: false, data: None, err: Some(e) }),
    };
    let second_content = match read_content(second) {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse { ok: false, data: None, err: Some(e) }),
    };

    let diff = generate_diff(&first_name, &second_name, &first_content, &second_content);

    Json(ApiResponse { ok: true, data: Some(diff), err: None })
}

fn generate_diff(name1: &str, name2: &str, content1: &[u8], content2: &[u8]) -> String {
    let text1 = String::from_utf8_lossy(content1);
    let text2 = String::from_utf8_lossy(content2);

    let lines1: Vec<&str> = text1.lines().collect();
    let lines2: Vec<&str> = text2.lines().collect();

    let mut diff = String::new();
    diff.push_str(&format!("--- {}\n", name1));
    diff.push_str(&format!("+++ {}\n", name2));

    // Simple line-by-line diff
    let mut i = 0usize;
    let mut j = 0usize;

    while i < lines1.len() || j < lines2.len() {
        if i >= lines1.len() {
            // Lines only in second
            diff.push_str(&format!("+{}\n", lines2[j]));
            j += 1;
        } else if j >= lines2.len() {
            // Lines only in first
            diff.push_str(&format!("-{}\n", lines1[i]));
            i += 1;
        } else if lines1[i] == lines2[j] {
            // Same line
            diff.push_str(&format!(" {}\n", lines1[i]));
            i += 1;
            j += 1;
        } else {
            // Different - look ahead to find matches
            let mut found = false;
            let lookahead = 5;
            for k in 1..=lookahead {
                if j + k < lines2.len() && i + k < lines1.len() && lines1[i + k] == lines2[j + k] {
                    // Found a match later - show as changed
                    for l in 0..k {
                        diff.push_str(&format!("-{}\n", lines1[i + l]));
                        diff.push_str(&format!("+{}\n", lines2[j + l]));
                    }
                    i += k;
                    j += k;
                    found = true;
                    break;
                }
            }
            if !found {
                // Just show as removed/added
                diff.push_str(&format!("-{}\n", lines1[i]));
                diff.push_str(&format!("+{}\n", lines2[j]));
                i += 1;
                j += 1;
            }
        }
    }

    diff
}

pub async fn api_get_snapshot_content(State(app_state): State<AppState>, axum::extract::Path(id): axum::extract::Path<i64>) -> Json<Option<String>> {
    let db = app_state.0.lock().unwrap();
    let snapshot = db.get_snapshot_by_id(id).ok().flatten();
    drop(db);

    if let Some(s) = snapshot
        && s.content_path.exists()
    {
        // Read only first 50KB + buffer for truncated message
        match s.peek_decompressed_content(50000) {
            Ok(content) => {
                match String::from_utf8(content) {
                    Ok(text) => {
                        if text.len() >= 50000 {
                            return Json(Some(text + "\n\n[... content truncated ...]"));
                        }
                        return Json(Some(text));
                    }
                    Err(_) => return Json(Some("[Binary content - cannot display as text]".to_string())),
                }
            }
            Err(e) => return Json(Some(format!("[Unable to decompress content: {}]", e))),
        }
    }
    Json(None)
}

#[derive(Deserialize)]
pub struct CreateSnapshotInput {
    pub path: String,
}

#[derive(Deserialize)]
pub struct AddExclusionInput {
    pub pattern: String,
    pub exclusion_type: String,
}
