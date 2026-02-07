// src/web/server.rs
use crate::db::Database;
use crate::web::api::*;
use axum::{
    routing::{get, post, delete},
    Router,
    response::Html,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::{CorsLayer, Any};

/// AppState wrapper for thread-safe database access
#[derive(Clone)]
pub struct AppState(pub Arc<Mutex<Database>>);

const HTML_PAGE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Freeze - Snapshot Manager</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        :root {
            --bg: #0d0d0d;
            --surface: #161616;
            --surface-hover: #1f1f1f;
            --border: #2a2a2a;
            --text: #ededed;
            --text-muted: #888;
            --accent: #00d4aa;
            --accent-hover: #00e8bb;
            --danger: #ff4444;
            --success: #00ff88;
            --warning: #ffaa00;
        }
        body { font-family: 'Inter', -apple-system, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; min-height: 100vh; }
        a { color: inherit; text-decoration: none; }

        /* Layout */
        .app { display: flex; min-height: 100vh; }
        .sidebar { width: 220px; background: var(--surface); border-right: 1px solid var(--border); position: fixed; height: 100vh; display: flex; flex-direction: column; flex-shrink: 0; }
        .main { flex: 1; margin-left: 220px; padding: 1.5rem; overflow-x: hidden; }

        /* Header */
        .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1.5rem; }
        .logo { font-size: 1.25rem; font-weight: 600; color: var(--accent); padding: 1.25rem 1.25rem 1rem; border-bottom: 1px solid var(--border); }

        /* Nav */
        .nav { padding: 1rem 0.75rem; flex: 1; overflow-y: auto; }
        .nav-section { margin-bottom: 1.25rem; }
        .nav-title { font-size: 0.65rem; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.1em; padding: 0 0.75rem; margin-bottom: 0.5rem; }
        .nav-item { display: flex; align-items: center; gap: 0.6rem; padding: 0.55rem 0.75rem; border-radius: 6px; cursor: pointer; transition: 0.15s; color: var(--text-muted); font-size: 0.85rem; margin-bottom: 0.2rem; }
        .nav-item:hover { background: var(--surface-hover); color: var(--text); }
        .nav-item.active { background: rgba(0, 212, 170, 0.1); color: var(--accent); }

        /* Stats bar */
        .stats-bar { display: flex; gap: 0.5rem; padding: 0.75rem 1rem; border-top: 1px solid var(--border); background: rgba(0,0,0,0.3); margin-top: auto; }
        .stat { flex: 1; text-align: center; min-width: 0; }
        .stat-value { font-size: 0.9rem; font-weight: 600; color: var(--accent); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .stat-label { font-size: 0.6rem; color: var(--text-muted); text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

        /* Page */
        .page { display: none; }
        .page.active { display: block; }

        /* Search bar */
        .search-bar { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
        .search-input { flex: 1; padding: 0.7rem 1rem; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; color: var(--text); font-size: 0.9rem; }
        .search-input:focus { outline: none; border-color: var(--accent); }
        .btn { padding: 0.6rem 1rem; border-radius: 6px; border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; font-size: 0.85rem; transition: 0.15s; }
        .btn:hover { background: var(--surface-hover); }
        .btn-primary { background: var(--accent); color: #000; border-color: var(--accent); font-weight: 500; }
        .btn-primary:hover { background: var(--accent-hover); }
        .btn-primary:disabled { background: #555; border-color: #555; cursor: not-allowed; }
        .btn-sm { padding: 0.35rem 0.65rem; font-size: 0.75rem; }
        .btn-danger { border-color: rgba(255, 68, 68, 0.3); color: var(--danger); }
        .btn-danger:hover { background: var(--danger); color: #fff; }

        /* Table */
        .table-container { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid var(--border); }
        th { font-size: 0.7rem; font-weight: 600; color: var(--text-muted); text-transform: uppercase; background: rgba(0,0,0,0.3); }
        tr { cursor: pointer; transition: 0.1s; }
        tr:hover { background: var(--surface-hover); }
        tr:last-child td { border-bottom: none; }
        .path-cell { max-width: 350px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; }
        .date-cell { font-size: 0.8rem; color: var(--text-muted); }
        .size-cell { font-size: 0.8rem; color: var(--text-muted); font-family: 'JetBrains Mono', monospace; }
        .checksum-cell { font-family: 'JetBrains Mono', monospace; font-size: 0.75rem; color: var(--text-muted); background: rgba(0,0,0,0.3); padding: 0.2rem 0.5rem; border-radius: 4px; }
        .actions-cell { white-space: nowrap; }

        /* Modal */
        .modal { display: none; position: fixed; inset: 0; background: rgba(0,0,0,0.85); z-index: 1000; align-items: center; justify-content: center; padding: 1rem; }
        .modal.active { display: flex; }
        .modal-content { background: var(--surface); border: 1px solid var(--border); border-radius: 12px; max-width: 700px; width: 100%; max-height: 90vh; overflow: auto; animation: modalIn 0.2s ease; }
        @keyframes modalIn { from { opacity: 0; transform: scale(0.95); } to { opacity: 1; transform: scale(1); } }
        .modal-header { padding: 1.25rem 1.5rem; border-bottom: 1px solid var(--border); display: flex; justify-content: space-between; align-items: center; }
        .modal-title { font-size: 1.1rem; font-weight: 600; }
        .modal-close { background: none; border: none; color: var(--text-muted); font-size: 1.5rem; cursor: pointer; padding: 0; line-height: 1; }
        .modal-close:hover { color: var(--text); }
        .modal-body { padding: 1.5rem; }
        .modal-actions { display: flex; gap: 0.75rem; margin-top: 1.25rem; padding-top: 1.25rem; border-top: 1px solid var(--border); }

        /* Detail info */
        .detail-path { font-family: 'JetBrains Mono', monospace; font-size: 0.9rem; word-break: break-all; margin-bottom: 1rem; padding: 0.75rem; background: rgba(0,0,0,0.3); border-radius: 6px; }
        .detail-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-bottom: 1.5rem; }
        .detail-item { text-align: center; padding: 0.75rem; background: rgba(0,0,0,0.2); border-radius: 6px; }
        .detail-value { font-size: 1.1rem; font-weight: 600; color: var(--accent); }
        .detail-label { font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; margin-top: 0.25rem; }

        /* Content viewer */
        .content-section { margin-top: 1.25rem; }
        .content-title { font-size: 0.85rem; font-weight: 600; margin-bottom: 0.75rem; color: var(--text-muted); }
        .content-viewer { background: #0a0a0a; border: 1px solid var(--border); border-radius: 6px; padding: 1rem; font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; white-space: pre-wrap; word-break: break-all; max-height: 300px; overflow: auto; }
        .content-empty { text-align: center; padding: 2rem; color: var(--text-muted); font-size: 0.9rem; }

        /* Form */
        .form-section { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; padding: 1.5rem; margin-bottom: 1.5rem; }
        .form-title { font-size: 1rem; font-weight: 600; margin-bottom: 1rem; }
        .form-row { display: flex; gap: 0.75rem; align-items: flex-end; }
        .form-input { flex: 1; padding: 0.7rem 1rem; background: var(--bg); border: 1px solid var(--border); border-radius: 6px; color: var(--text); font-size: 0.9rem; }
        .form-input:focus { outline: none; border-color: var(--accent); }

        /* Exclusions list */
        .exclusions-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 0.75rem; }

        /* Diff page */
        .diff-selection-bar { display: flex; align-items: center; gap: 1rem; padding: 0.75rem 1rem; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; margin-bottom: 1rem; }
        .diff-selected { flex: 1; min-width: 0; }
        .diff-label { font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; display: block; margin-bottom: 0.2rem; }
        .diff-path { font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; color: var(--accent); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; }
        .diff-container { display: grid; grid-template-columns: 1fr; gap: 1rem; }
        .diff-files-list { display: flex; flex-direction: column; gap: 0.5rem; }
        .diff-file-group { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
        .diff-file-header { display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: rgba(0,0,0,0.3); cursor: pointer; }
        .diff-file-header:hover { background: var(--surface-hover); }
        .diff-file-name { font-family: 'JetBrains Mono', monospace; font-size: 0.9rem; font-weight: 500; }
        .diff-snapshots { display: flex; flex-direction: column; }
        .diff-snapshot-item { display: flex; align-items: center; gap: 0.75rem; padding: 0.6rem 1rem 0.6rem 2rem; border-top: 1px solid var(--border); cursor: pointer; transition: 0.1s; }
        .diff-snapshot-item:hover { background: var(--surface-hover); }
        .diff-snapshot-item.selected-1 { background: rgba(0, 212, 170, 0.15); border-left: 3px solid var(--accent); }
        .diff-snapshot-item.selected-2 { background: rgba(255, 170, 0, 0.15); border-left: 3px solid var(--warning); }
        .diff-snapshot-item.both-selected { background: linear-gradient(90deg, rgba(0, 212, 170, 0.1) 0%, rgba(255, 170, 0, 0.1) 100%); }
        .diff-checkbox { width: 18px; height: 18px; border-radius: 4px; border: 2px solid var(--border); display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
        .diff-snapshot-item.selected-1 .diff-checkbox { background: var(--accent); border-color: var(--accent); }
        .diff-snapshot-item.selected-2 .diff-checkbox { background: var(--warning); border-color: var(--warning); }
        .diff-checkbox::after { content: '✓'; font-size: 0.7rem; color: #000; display: none; }
        .diff-snapshot-item.selected-1 .diff-checkbox::after,
        .diff-snapshot-item.selected-2 .diff-checkbox::after { display: block; }
        .diff-snapshot-info { flex: 1; min-width: 0; }
        .diff-snapshot-date { font-size: 0.85rem; color: var(--text); }
        .diff-snapshot-size { font-size: 0.75rem; color: var(--text-muted); margin-left: 0.5rem; }
        .diff-snapshot-checksum { font-family: 'JetBrains Mono', monospace; font-size: 0.7rem; color: var(--text-muted); background: rgba(0,0,0,0.3); padding: 0.15rem 0.4rem; border-radius: 3px; margin-left: 0.5rem; }

        /* Diff results */
        .diff-output { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
        .diff-header { padding: 0.75rem 1rem; background: rgba(0,0,0,0.3); font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; color: var(--text-muted); border-bottom: 1px solid var(--border); }
        .diff-content { padding: 1rem; font-family: 'JetBrains Mono', monospace; font-size: 0.85rem; white-space: pre-wrap; line-height: 1.6; max-height: 500px; overflow-y: auto; }
        .diff-line { padding: 0.1rem 0.5rem; margin: 0 -0.5rem; border-radius: 3px; }
        .diff-line-removed { background: rgba(255, 68, 68, 0.15); color: #ff6b6b; }
        .diff-line-added { background: rgba(0, 255, 136, 0.15); color: #4ade80; }
        .diff-line-unchanged { color: var(--text-muted); }
        .exclusion-tag { display: flex; align-items: center; justify-content: space-between; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 0.6rem 0.75rem; }
        .exclusion-info { display: flex; align-items: center; gap: 0.5rem; }
        .exclusion-pattern { font-family: 'JetBrains Mono', monospace; font-size: 0.85rem; }
        .exclusion-type { font-size: 0.7rem; color: var(--text-muted); background: rgba(0,0,0,0.3); padding: 0.2rem 0.5rem; border-radius: 4px; }

        /* Empty state */
        .empty { text-align: center; padding: 4rem 2rem; color: var(--text-muted); }
        .empty-icon { font-size: 3rem; margin-bottom: 1rem; opacity: 0.4; }

        /* Toast */
        .toast { position: fixed; bottom: 1.5rem; right: 1.5rem; padding: 0.85rem 1.25rem; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; font-size: 0.9rem; z-index: 2000; opacity: 0; transform: translateY(10px); transition: 0.3s; pointer-events: none; }
        .toast.show { opacity: 1; transform: translateY(0); }
        .toast.success { border-color: var(--success); color: var(--success); }
        .toast.error { border-color: var(--danger); color: var(--danger); }

        /* Responsive */
        @media (max-width: 768px) {
            .sidebar { width: 100%; height: auto; position: relative; }
            .main { margin-left: 0; }
            .nav { display: flex; flex-wrap: wrap; gap: 0.5rem; padding: 0.75rem; }
            .nav-section { margin-bottom: 0; }
            .nav-title { display: none; }
            .nav-item { padding: 0.5rem 0.75rem; background: var(--surface); }
            .stats-bar { display: none; }
            .detail-grid { grid-template-columns: 1fr; }
        }
    </style>
</head>
<body>
    <div class="app">
        <nav class="sidebar">
            <div class="logo">Freeze</div>
            <div class="nav">
                <div class="nav-section">
                    <div class="nav-title">Manage</div>
                    <div class="nav-item active" data-page="snapshots">
                        <span>&#128196;</span> Snapshots
                    </div>
                    <div class="nav-item" data-page="save">
                        <span>&#128190;</span> Save New
                    </div>
                </div>
                <div class="nav-section">
                    <div class="nav-title">Tools</div>
                    <div class="nav-item" data-page="search">
                        <span>&#128269;</span> Search
                    </div>
                    <div class="nav-item" data-page="exclusions">
                        <span>&#128683;</span> Exclusions
                    </div>
                    <div class="nav-item" data-page="diff">
                        <span>&#8614;</span> Compare
                    </div>
                </div>
            </div>
            <div class="stats-bar">
                <div class="stat">
                    <div class="stat-value" id="total-snapshots">0</div>
                    <div class="stat-label">Snapshots</div>
                </div>
                <div class="stat">
                    <div class="stat-value" id="total-storage">0 B</div>
                    <div class="stat-label">Storage</div>
                </div>
                <div class="stat">
                    <div class="stat-value" id="total-exclusions">0</div>
                    <div class="stat-label">Exclusions</div>
                </div>
            </div>
        </nav>

        <main class="main">
            <!-- Snapshots Page -->
            <div id="snapshots" class="page active">
                <div class="header">
                    <div>
                        <h1 style="font-size: 1.5rem; font-weight: 600;">Snapshots</h1>
                        <p style="color: var(--text-muted); font-size: 0.85rem; margin-top: 0.25rem;">Click a snapshot to view details and actions</p>
                    </div>
                </div>
                <div class="search-bar">
                    <input type="text" class="search-input" id="search-snapshots" placeholder="Search snapshots..." oninput="filterSnapshots()">
                    <button class="btn" onclick="loadSnapshots()">Refresh</button>
                    <button class="btn btn-primary" onclick="navigateTo('save')">+ Save New</button>
                </div>
                <div class="table-container">
                    <table>
                        <thead><tr><th>Path</th><th>Size</th><th>Date</th><th>Checksum</th></tr></thead>
                        <tbody id="snapshots-list"></tbody>
                    </table>
                </div>
            </div>

            <!-- Save Page -->
            <div id="save" class="page">
                <div class="header">
                    <h1 style="font-size: 1.5rem; font-weight: 600;">Save Snapshot</h1>
                    <p style="color: var(--text-muted); font-size: 0.85rem; margin-top: 0.25rem;">Save the current state of a file or directory</p>
                </div>
                <div class="form-section">
                    <div class="form-title">Path to save</div>
                    <div class="form-row">
                        <input type="text" class="form-input" id="save-path" placeholder="/path/to/file_or_directory" onkeypress="if(event.key==='Enter')handleSave()">
                        <button class="btn btn-primary" onclick="handleSave()">Save Snapshot</button>
                    </div>
                    <div id="save-message" style="margin-top: 1rem;"></div>
                </div>
                <button class="btn" onclick="navigateTo('snapshots')">Back to Snapshots</button>
            </div>

            <!-- Search Page -->
            <div id="search" class="page">
                <div class="header">
                    <h1 style="font-size: 1.5rem; font-weight: 600;">Search</h1>
                    <p style="color: var(--text-muted); font-size: 0.85rem; margin-top: 0.25rem;">Find snapshots by path pattern</p>
                </div>
                <div class="form-section">
                    <div class="form-row">
                        <input type="text" class="form-input" id="search-input" placeholder="Search pattern (e.g., *.py, /home/)" onkeypress="if(event.key==='Enter')performSearch()">
                        <button class="btn btn-primary" onclick="performSearch()">Search</button>
                    </div>
                </div>
                <div id="search-results"></div>
                <button class="btn" onclick="navigateTo('snapshots')" style="margin-top: 1rem;">Back to Snapshots</button>
            </div>

            <!-- Exclusions Page -->
            <div id="exclusions" class="page">
                <div class="header">
                    <h1 style="font-size: 1.5rem; font-weight: 600;">Exclusions</h1>
                    <p style="color: var(--text-muted); font-size: 0.85rem; margin-top: 0.25rem;">Patterns excluded from snapshots</p>
                </div>
                <div class="form-section">
                    <div class="form-title">Add Exclusion</div>
                    <div class="form-row">
                        <input type="text" class="form-input" id="exclusion-pattern" placeholder="Pattern (e.g., *.log, node_modules)" onkeypress="if(event.key==='Enter')handleAddExclusion()">
                        <select class="form-input" id="exclusion-type" style="width: 120px;">
                            <option value="file">File</option>
                            <option value="directory">Directory</option>
                            <option value="extension">Extension</option>
                        </select>
                        <button class="btn btn-primary" onclick="handleAddExclusion()">Add</button>
                    </div>
                </div>
                <div id="exclusions-list" class="exclusions-grid"></div>
                <button class="btn" onclick="navigateTo('snapshots')" style="margin-top: 1.5rem;">Back to Snapshots</button>
            </div>

            <!-- Diff Page -->
            <div id="diff" class="page">
                <div class="header">
                    <h1 style="font-size: 1.5rem; font-weight: 600;">Compare Snapshots</h1>
                    <p style="color: var(--text-muted); font-size: 0.85rem; margin-top: 0.25rem;">Select two snapshots to compare their contents</p>
                </div>

                <!-- Selection Summary -->
                <div class="diff-selection-bar" id="diff-selection-bar" style="display: none;">
                    <div class="diff-selected">
                        <span class="diff-label">First:</span>
                        <span class="diff-path" id="diff-selected-1">-</span>
                    </div>
                    <div class="diff-selected">
                        <span class="diff-label">Second:</span>
                        <span class="diff-path" id="diff-selected-2">-</span>
                    </div>
                    <button class="btn btn-primary" id="diff-compare-btn" onclick="handleDiff()">Compare</button>
                    <button class="btn" onclick="clearDiffSelection()">Clear</button>
                </div>

                <!-- File List with Snapshots -->
                <div class="diff-container">
                    <div class="diff-files-list" id="diff-files-list">
                        <div style="color: var(--text-muted); text-align: center; padding: 2rem;">Loading snapshots...</div>
                    </div>
                </div>

                <!-- Diff Results -->
                <div id="diff-results" style="margin-top: 1.5rem;"></div>
            </div>
        </main>
    </div>

    <!-- Snapshot Detail Modal -->
    <div id="detail-modal" class="modal">
        <div class="modal-content">
            <div class="modal-header">
                <h3 class="modal-title">Snapshot Details</h3>
                <button class="modal-close" onclick="closeModal()">&times;</button>
            </div>
            <div class="modal-body">
                <div class="detail-path" id="modal-path"></div>
                <div class="detail-grid">
                    <div class="detail-item">
                        <div class="detail-value" id="modal-size">-</div>
                        <div class="detail-label">Size</div>
                    </div>
                    <div class="detail-item">
                        <div class="detail-value" id="modal-date">-</div>
                        <div class="detail-label">Date</div>
                    </div>
                    <div class="detail-item">
                        <div class="detail-value" id="modal-checksum">-</div>
                        <div class="detail-label">Checksum</div>
                    </div>
                </div>
                <div class="content-section">
                    <div class="content-title">Content Preview</div>
                    <div id="modal-content" class="content-viewer"></div>
                </div>
                <div class="modal-actions">
                    <button class="btn btn-primary" onclick="modalAction('restore')">Restore</button>
                    <button class="btn" onclick="modalAction('view')">View Content</button>
                    <button class="btn" onclick="openExportModal()">Export</button>
                    <button class="btn btn-danger" onclick="modalAction('delete')">Delete</button>
                </div>
            </div>
        </div>
    </div>

    <!-- Export Modal -->
    <div id="export-modal" class="modal">
        <div class="modal-content" style="max-width: 400px;">
            <div class="modal-header">
                <h3 class="modal-title">Export Snapshot</h3>
                <button class="modal-close" onclick="closeExportModal()">&times;</button>
            </div>
            <div class="modal-body">
                <div class="form-group">
                    <label class="form-label">Destination path</label>
                    <input type="text" class="form-input" id="export-destination" placeholder="~/Downloads/filename or /path/to/file">
                </div>
                <div style="color: var(--text-muted); font-size: 0.8rem; margin-top: 0.5rem;">Leave empty to export to current directory</div>
                <div class="modal-actions">
                    <button class="btn" onclick="closeExportModal()">Cancel</button>
                    <button class="btn btn-primary" onclick="confirmExport()">Export</button>
                </div>
            </div>
        </div>
    </div>

    <!-- Toast -->
    <div id="toast" class="toast"></div>

    <script>
        var API = "";
        var currentSnapshots = [];
        var selectedSnapshot = null;

        // Navigation
        document.querySelectorAll('.nav-item').forEach(function(item) {
            item.addEventListener('click', function() {
                navigateTo(this.dataset.page);
            });
        });

        function navigateTo(page) {
            document.querySelectorAll('.nav-item').forEach(function(i) { i.classList.remove('active'); });
            document.querySelectorAll('.page').forEach(function(p) { p.classList.remove('active'); });
            document.querySelector('[data-page="' + page + '"]').classList.add('active');
            document.getElementById(page).classList.add('active');
            loadPageData(page);
        }

        function loadPageData(page) {
            if (page === 'snapshots') loadSnapshots();
            if (page === 'exclusions') loadExclusions();
            if (page === 'diff') loadDiffPage();
        }

        // Load stats
        async function loadStats() {
            var stats = await fetch(API + '/api/stats').then(function(r) { return r.json(); });
            document.getElementById('total-snapshots').textContent = stats.total_snapshots;
            document.getElementById('total-storage').textContent = stats.storage_formatted;
            document.getElementById('total-exclusions').textContent = stats.total_exclusions;
        }

        // Load snapshots
        async function loadSnapshots() {
            currentSnapshots = await fetch(API + '/api/snapshots').then(function(r) { return r.json(); });
            renderSnapshots(currentSnapshots);
            loadStats();
        }

        function renderSnapshots(snapshots) {
            var tbody = document.getElementById('snapshots-list');
            if (snapshots.length === 0) {
                tbody.innerHTML = '<tr><td colspan="4"><div class="empty"><div class="empty-icon">&#128196;</div><p>No snapshots found</p></div></td></tr>';
                return;
            }
            var html = '';
            for (var i = 0; i < snapshots.length; i++) {
                var s = snapshots[i];
                html += '<tr onclick="openDetail(' + s.id + ')"><td class="path-cell" title="' + s.path + '">' + s.path + '</td><td class="size-cell">' + s.size_formatted + '</td><td class="date-cell">' + s.date.split('T')[0] + '</td><td><span class="checksum-cell">' + s.checksum.substring(0, 16) + '</span></td></tr>';
            }
            tbody.innerHTML = html;
        }

        function filterSnapshots() {
            var query = document.getElementById('search-snapshots').value.toLowerCase();
            var filtered = currentSnapshots.filter(function(s) {
                return s.path.toLowerCase().includes(query);
            });
            renderSnapshots(filtered);
        }

        // Detail modal
        async function openDetail(id) {
            var snapshot = currentSnapshots.find(function(s) { return s.id === id; });
            if (!snapshot) return;

            selectedSnapshot = snapshot;

            document.getElementById('modal-path').textContent = snapshot.path;
            document.getElementById('modal-size').textContent = snapshot.size_formatted;
            document.getElementById('modal-date').textContent = snapshot.date.replace('T', ' ').split('.')[0];
            document.getElementById('modal-checksum').textContent = snapshot.checksum.substring(0, 16) + '...';
            if (snapshot.size > 100000) {
                document.getElementById('modal-content').innerHTML = '<div class="content-empty">File too large to preview (' + snapshot.size_formatted + ')\n\nClick "View Content" to try loading anyway, or use CLI:</div><code style="display:block;margin-top:0.5rem;font-size:0.75rem;background:rgba(0,0,0,0.3);padding:0.5rem;border-radius:4px;">freeze view ' + snapshot.path + '</code>';
            } else {
                document.getElementById('modal-content').innerHTML = '<div class="content-empty">Click "View Content" to load preview</div>';
            }

            document.getElementById('detail-modal').classList.add('active');
        }

        function closeModal() {
            document.getElementById('detail-modal').classList.remove('active');
            selectedSnapshot = null;
        }

        function modalAction(action) {
            if (!selectedSnapshot) return;

            if (action === 'restore') {
                if (!confirm('Restore ' + selectedSnapshot.path + '?')) return;
                fetch(API + '/api/snapshots/' + selectedSnapshot.id + '/restore', { method: 'POST' });
                showToast('Restored successfully!', 'success');
                closeModal();
            } else if (action === 'delete') {
                if (!confirm('Delete this snapshot? This cannot be undone.')) return;
                fetch(API + '/api/snapshots/' + selectedSnapshot.id, { method: 'DELETE' });
                showToast('Deleted successfully!', 'success');
                loadSnapshots();
                closeModal();
            } else if (action === 'view') {
                loadContentPreview();
            }
        }

        // Export functions
        function openExportModal() {
            document.getElementById('export-destination').value = '';
            document.getElementById('export-modal').classList.add('active');
        }

        function closeExportModal() {
            document.getElementById('export-modal').classList.remove('active');
        }

        async function confirmExport() {
            if (!selectedSnapshot) return;

            var destination = document.getElementById('export-destination').value;
            closeExportModal();

            try {
                var res = await fetch(API + '/api/snapshots/' + selectedSnapshot.id + '/export', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({destination: destination || null})
                });
                var data = await res.json();
                if (data.ok) {
                    showToast('Exported successfully!', 'success');
                } else {
                    showToast('Error: ' + data.err, 'error');
                }
            } catch (err) {
                showToast('Error: ' + err, 'error');
            }
        }

        // Diff state
        let diffSelected1 = null;
        let diffSelected2 = null;
        let allSnapshots = [];

        // Load diff page
        async function loadDiffPage() {
            await loadDiffSnapshots();
        }

        // Load all snapshots grouped by file
        async function loadDiffSnapshots() {
            var container = document.getElementById('diff-files-list');
            container.innerHTML = '<div style="color: var(--text-muted); text-align: center; padding: 2rem;">Loading...</div>';

            try {
                var res = await fetch(API + '/api/snapshots');
                var snapshots = await res.json();
                allSnapshots = snapshots;

                if (snapshots.length === 0) {
                    container.innerHTML = '<div style="color: var(--text-muted); text-align: center; padding: 2rem;">No snapshots found</div>';
                    return;
                }

                // Group by file path (use only filename for display)
                var byFile = {};
                snapshots.forEach(function(s) {
                    var name = s.path.split('/').pop();
                    if (!byFile[name]) byFile[name] = [];
                    byFile[name].push(s);
                });

                // Sort files alphabetically
                var files = Object.keys(byFile).sort();

                var html = '';
                files.forEach(function(file) {
                    var snaps = byFile[file];
                    // Sort snapshots by date descending (newest first)
                    snaps.sort(function(a, b) { return new Date(b.date) - new Date(a.date); });

                    html += '<div class="diff-file-group">';
                    html += '<div class="diff-file-header">';
                    html += '<span class="diff-file-name">' + escapeHtml(file) + '</span>';
                    html += '<span style="color: var(--text-muted); font-size: 0.75rem;">' + snaps.length + ' snapshot' + (snaps.length > 1 ? 's' : '') + '</span>';
                    html += '</div>';
                    html += '<div class="diff-snapshots">';

                    snaps.forEach(function(s) {
                        var isSelected1 = diffSelected1 && diffSelected1.id === s.id;
                        var isSelected2 = diffSelected2 && diffSelected2.id === s.id;
                        var selectedClass = '';
                        if (isSelected1 && isSelected2) selectedClass = 'both-selected';
                        else if (isSelected1) selectedClass = 'selected-1';
                        else if (isSelected2) selectedClass = 'selected-2';

                        html += '<div class="diff-snapshot-item ' + selectedClass + '" onclick="selectDiffSnapshot(' + s.id + ')" data-id="' + s.id + '">';
                        html += '<div class="diff-checkbox"></div>';
                        html += '<div class="diff-snapshot-info">';
                        html += '<span class="diff-snapshot-date">' + formatDate(s.date) + '</span>';
                        html += '<span class="diff-snapshot-size">' + s.size_formatted + '</span>';
                        html += '</div>';
                        html += '<span class="diff-snapshot-checksum">' + s.checksum.substring(0, 8) + '</span>';
                        html += '</div>';
                    });

                    html += '</div></div>';
                });

                container.innerHTML = html;
                updateDiffSelectionBar();
            } catch (err) {
                container.innerHTML = '<div style="color: var(--danger); text-align: center; padding: 2rem;">Error loading snapshots: ' + err + '</div>';
            }
        }

        // Select a snapshot for comparison
        function selectDiffSnapshot(id) {
            console.log('selectDiffSnapshot called with id:', id);
            var snapshot = allSnapshots.find(function(s) { return s.id === id; });
            if (!snapshot) return;

            // First click - select as first
            if (!diffSelected1) {
                diffSelected1 = snapshot;
            }
            // Second click - select as second (or replace first if already selected)
            else if (!diffSelected2) {
                // Don't allow selecting the same snapshot twice
                if (diffSelected1 && diffSelected1.id === id) {
                    diffSelected1 = null;
                } else {
                    diffSelected2 = snapshot;
                }
            }
            // Third click - start over
            else {
                diffSelected1 = snapshot;
                diffSelected2 = null;
            }

            // Refresh the list to show selection state
            loadDiffSnapshots();
        }

        // Clear diff selection
        function clearDiffSelection() {
            diffSelected1 = null;
            diffSelected2 = null;
            document.getElementById('diff-results').innerHTML = '';
            loadDiffSnapshots();
        }

        // Update the selection bar
        function updateDiffSelectionBar() {
            var bar = document.getElementById('diff-selection-bar');
            var sel1 = document.getElementById('diff-selected-1');
            var sel2 = document.getElementById('diff-selected-2');
            var btn = document.getElementById('diff-compare-btn');

            if (diffSelected1 || diffSelected2) {
                bar.style.display = 'flex';
                sel1.textContent = diffSelected1 ? diffSelected1.path.split('/').pop() + ' (' + formatDate(diffSelected1.date) + ')' : '-';
                sel2.textContent = diffSelected2 ? diffSelected2.path.split('/').pop() + ' (' + formatDate(diffSelected2.date) + ')' : '-';
                btn.disabled = !(diffSelected1 && diffSelected2);
                btn.textContent = diffSelected1 && diffSelected2 ? 'Compare' : 'Select 2 snapshots';
            } else {
                bar.style.display = 'none';
            }
        }

        // Diff function
        async function handleDiff() {
            console.log('handleDiff called', diffSelected1, diffSelected2);
            if (!diffSelected1 || !diffSelected2) {
                showToast('Please select two snapshots to compare', 'error');
                return;
            }

            var container = document.getElementById('diff-results');
            container.innerHTML = '<div class="content-empty">Loading...</div>';

            try {
                var res = await fetch(API + '/api/diff', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({first: diffSelected1.checksum, second: diffSelected2.checksum})
                });
                var data = await res.json();
                if (data.ok && data.data) {
                    var diff = data.data;
                    var lines = diff.split('\n');
                    var html = '<div class="diff-output"><div class="diff-header">--- ' + escapeHtml(diffSelected1.path.split('/').pop()) + ' +++ ' + escapeHtml(diffSelected2.path.split('/').pop()) + '</div><div class="diff-content">';
                    for (var i = 0; i < lines.length; i++) {
                        var line = lines[i];
                        var cls = 'unchanged';
                        if (line.startsWith('+') && !line.startsWith('+++')) cls = 'added';
                        else if (line.startsWith('-') && !line.startsWith('---')) cls = 'removed';
                        html += '<div class="diff-line diff-line-' + cls + '">' + escapeHtml(line) + '</div>';
                    }
                    html += '</div></div>';
                    container.innerHTML = html;
                } else {
                    container.innerHTML = '<div class="content-empty">Error: ' + (data.err || 'Unable to compare') + '</div>';
                }
            } catch (err) {
                container.innerHTML = '<div class="content-empty">Error: ' + err + '</div>';
            }
        }

        // Format date helper
        function formatDate(dateStr) {
            try {
                var d = new Date(dateStr);
                return d.toLocaleString();
            } catch (e) { return dateStr; }
        }

        async function loadContentPreview() {
            if (!selectedSnapshot) return;

            var container = document.getElementById('modal-content');
            container.innerHTML = '<div class="content-empty">Loading...</div>';

            // Check file size first
            if (selectedSnapshot.size > 100000) {
                container.innerHTML = '<div class="content-empty">File too large to preview (' + selectedSnapshot.size_formatted + ')\n\nUse CLI: freeze view ' + selectedSnapshot.path + '</div>';
                return;
            }

            // Try to load content from API
            try {
                var res = await fetch(API + '/api/snapshots/' + selectedSnapshot.id + '/content');
                var data = await res.json();
                if (data) {
                    if (data.startsWith('[')) {
                        container.innerHTML = '<div class="content-empty">' + data + '</div>';
                    } else {
                        container.innerHTML = '<div class="content-viewer" style="max-height:400px;">' + data.replace(/</g, '&lt;').replace(/>/g, '&gt;') + '</div>';
                    }
                } else {
                    container.innerHTML = '<div class="content-empty">Unable to preview this file</div>';
                }
            } catch (err) {
                container.innerHTML = '<div class="content-empty">Error loading content: ' + err + '</div>';
            }
        }

        // Save
        async function handleSave() {
            var path = document.getElementById('save-path').value;
            var msg = document.getElementById('save-message');
            var btn = document.querySelector('#save .btn-primary');
            if (!path) { showToast('Please enter a path', 'error'); return; }

            // Show loading state
            btn.disabled = true;
            btn.textContent = 'Saving...';
            msg.innerHTML = '';

            try {
                var res = await fetch(API + '/api/snapshots', { method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({path: path}) });
                var data = await res.json();
                if (data.ok) {
                    showToast('Snapshot saved successfully!', 'success');
                    document.getElementById('save-path').value = '';
                    loadSnapshots();
                } else {
                    showToast('Error: ' + data.err, 'error');
                }
            } catch (err) {
                showToast('Error: ' + err, 'error');
            } finally {
                btn.disabled = false;
                btn.textContent = 'Save Snapshot';
            }
        }

        // Search
        async function performSearch() {
            var query = document.getElementById('search-input').value;
            var container = document.getElementById('search-results');
            if (!query) { container.innerHTML = ''; return; }

            var snapshots = await fetch(API + '/api/snapshots/search?q=' + encodeURIComponent(query)).then(function(r) { return r.json(); });

            if (snapshots.length === 0) {
                container.innerHTML = '<div class="empty"><div class="empty-icon">&#128269;</div><p>No results found</p></div>';
                return;
            }

            var html = '<div class="table-container"><table><thead><tr><th>Path</th><th>Size</th><th>Date</th><th>Actions</th></tr></thead><tbody>';
            for (var i = 0; i < snapshots.length; i++) {
                var s = snapshots[i];
                html += '<tr onclick="openDetail(' + s.id + ')"><td class="path-cell">' + s.path + '</td><td class="size-cell">' + s.size_formatted + '</td><td class="date-cell">' + s.date.split('T')[0] + '</td><td class="actions-cell"><button class="btn btn-sm" onclick="event.stopPropagation();quickRestore(' + s.id + ')">Restore</button></td></tr>';
            }
            html += '</tbody></table></div>';
            container.innerHTML = html;
        }

        async function quickRestore(id) {
            if (!confirm('Restore this snapshot?')) return;
            await fetch(API + '/api/snapshots/' + id + '/restore', { method: 'POST' });
            showToast('Restored successfully!', 'success');
        }

        // Exclusions
        async function loadExclusions() {
            var exclusions = await fetch(API + '/api/exclusions').then(function(r) { return r.json(); });
            var container = document.getElementById('exclusions-list');

            if (exclusions.length === 0) {
                container.innerHTML = '<div class="empty"><div class="empty-icon">&#128683;</div><p>No exclusions configured</p></div>';
                return;
            }

            var html = '';
            for (var i = 0; i < exclusions.length; i++) {
                var e = exclusions[i];
                html += '<div class="exclusion-tag"><div class="exclusion-info"><span class="exclusion-pattern">' + e.pattern + '</span><span class="exclusion-type">' + e.exclusion_type + '</span></div><button class="btn btn-sm btn-danger" onclick="removeExclusion(\'' + encodeURIComponent(e.pattern) + '\')">&times;</button></div>';
            }
            container.innerHTML = html;
        }

        async function handleAddExclusion() {
            var pattern = document.getElementById('exclusion-pattern').value;
            var type = document.getElementById('exclusion-type').value;
            if (!pattern) { showToast('Please enter a pattern', 'error'); return; }

            await fetch(API + '/api/exclusions', { method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({pattern: pattern, exclusion_type: type}) });
            document.getElementById('exclusion-pattern').value = '';
            loadExclusions();
            showToast('Exclusion added', 'success');
        }

        async function removeExclusion(pattern) {
            await fetch(API + '/api/exclusions/' + decodeURIComponent(pattern), { method: 'DELETE' });
            loadExclusions();
            showToast('Exclusion removed', 'success');
        }

        // Escape HTML
        function escapeHtml(text) {
            if (!text) return '';
            return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#039;');
        }

        // Toast
        function showToast(message, type) {
            var toast = document.getElementById('toast');
            toast.textContent = message;
            toast.className = 'toast ' + type + ' show';
            setTimeout(function() { toast.classList.remove('show'); }, 3000);
        }

        // Close modal on outside click
        document.getElementById('detail-modal').addEventListener('click', function(e) {
            if (e.target === this) closeModal();
        });

        // Close modal on Escape
        document.addEventListener('keydown', function(e) {
            if (e.key === 'Escape') closeModal();
        });

        // Initial load
        loadSnapshots();
    </script>
</body>
</html>
"##;

pub async fn run_server(port: u16, open_browser: bool) -> Result<(), anyhow::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let db = Database::new()?;
    let app_state = AppState(Arc::new(Mutex::new(db)));
    let cors = CorsLayer::new().allow_origin(Any);

    let app = Router::new()
        .route("/", get(|| async { Html(HTML_PAGE) }))
        .route("/index.html", get(|| async { Html(HTML_PAGE) }))
        .route("/api/snapshots", get(api_list_snapshots))
        .route("/api/snapshots/search", get(api_search_snapshots))
        .route("/api/snapshots", post(api_create_snapshot))
        .route("/api/snapshots/:id", get(api_get_snapshot))
        .route("/api/snapshots/:id/content", get(api_get_snapshot_content))
        .route("/api/snapshots/:id/export", post(api_export_snapshot))
        .route("/api/snapshots/:id/restore", post(api_restore_snapshot))
        .route("/api/snapshots/:id", delete(api_delete_snapshot))
        .route("/api/diff", post(api_diff_snapshots))
        .route("/api/exclusions", get(api_list_exclusions))
        .route("/api/exclusions", post(api_add_exclusion))
        .route("/api/exclusions/:pattern", delete(api_remove_exclusion))
        .route("/api/stats", get(api_get_stats))
        .layer(cors)
        .with_state(app_state);

    println!("\n  Freeze Web Interface");
    println!("  Running at: http://{}", addr);
    println!("  Press Ctrl+C to stop.");
    println!();

    if open_browser {
        let url = format!("http://{}", addr);
        let _ = open::that(&url);
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
