//! Git Web Worker bridge for Rust/WASM
//!
//! Provides async interface to the isomorphic-git Web Worker.
//! Follows the same pattern as voice_recorder.rs for JS interop.

use js_sys::Reflect;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// File entry from git tree listing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String, // "tree" or "blob"
    pub path: String,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        self.entry_type == "tree"
    }
}

/// Git Worker manager - follows voice_recorder.rs pattern
/// Worker is initialized globally via window.gitWorkerManager
pub struct GitWorkerManager;

impl GitWorkerManager {
    /// Initialize the git worker (call once on app startup)
    /// Waits for the worker to send 'ready' message before returning.
    pub async fn init() -> Result<(), String> {
        let script = r#"
            (async function() {
                if (window.gitWorkerManager && window.gitWorkerManager.ready) {
                    return { success: true };
                }

                return new Promise((resolveInit, rejectInit) => {
                    const timeoutId = setTimeout(() => {
                        rejectInit(new Error('Git worker initialization timed out after 30s'));
                    }, 30000);

                    window.gitWorkerManager = {
                        worker: new Worker('/git-worker.js', { type: 'module' }),
                        nextId: 0,
                        pending: new Map(),
                        ready: false,

                        call(method, params) {
                            const id = ++this.nextId;
                            return new Promise((resolve, reject) => {
                                this.pending.set(id, { resolve, reject });
                                this.worker.postMessage({ id, method, params });
                            });
                        }
                    };

                    window.gitWorkerManager.worker.onmessage = (e) => {
                        const { id, type, result, error } = e.data;
                        if (type === 'ready') {
                            console.log('[GitWorker] Worker ready');
                            window.gitWorkerManager.ready = true;
                            clearTimeout(timeoutId);
                            resolveInit({ success: true });
                            return;
                        }
                        if (type === 'progress') {
                            console.log('[GitWorker] Progress:', e.data);
                            return;
                        }
                        const handler = window.gitWorkerManager.pending.get(id);
                        if (handler) {
                            window.gitWorkerManager.pending.delete(id);
                            if (type === 'error') {
                                handler.reject(new Error(error));
                            } else {
                                handler.resolve(result);
                            }
                        }
                    };

                    window.gitWorkerManager.worker.onerror = (e) => {
                        console.error('[GitWorker] Worker error:', e);
                        clearTimeout(timeoutId);
                        rejectInit(new Error('Git worker failed to load: ' + e.message));
                    };
                });
            })()
        "#;

        let promise =
            js_sys::eval(script).map_err(|e| format!("Failed to init git worker: {:?}", e))?;
        let promise = js_sys::Promise::from(promise);
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| format!("Git worker init failed: {:?}", e))?;

        log::info!("Git worker initialized");
        Ok(())
    }

    /// Check if git worker is initialized and ready
    pub fn is_initialized() -> bool {
        let script = "window.gitWorkerManager && window.gitWorkerManager.ready === true";
        js_sys::eval(script)
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Clone a repository (shallow by default)
    pub async fn clone_repo(url: &str, dir: &str, depth: u32) -> Result<(), String> {
        // Escape strings for JS
        let url_escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
        let dir_escaped = dir.replace('\\', "\\\\").replace('\'', "\\'");

        let script = format!(
            "window.gitWorkerManager.call('clone', {{ url: '{}', dir: '{}', depth: {} }})",
            url_escaped, dir_escaped, depth
        );

        Self::call_worker(&script).await?;
        Ok(())
    }

    /// List files in a directory at a given ref
    pub async fn list_files(
        dir: &str,
        path: &str,
        git_ref: &str,
    ) -> Result<Vec<FileEntry>, String> {
        let dir_escaped = dir.replace('\\', "\\\\").replace('\'', "\\'");
        let path_escaped = path.replace('\\', "\\\\").replace('\'', "\\'");
        let ref_escaped = git_ref.replace('\\', "\\\\").replace('\'', "\\'");

        let script = format!(
            "window.gitWorkerManager.call('listFiles', {{ dir: '{}', path: '{}', ref: '{}' }})",
            dir_escaped, path_escaped, ref_escaped
        );

        let result = Self::call_worker(&script).await?;
        serde_wasm_bindgen::from_value(result).map_err(|e| format!("Parse error: {:?}", e))
    }

    /// Read file content at a given ref
    pub async fn read_file(dir: &str, filepath: &str, git_ref: &str) -> Result<String, String> {
        let dir_escaped = dir.replace('\\', "\\\\").replace('\'', "\\'");
        let filepath_escaped = filepath.replace('\\', "\\\\").replace('\'', "\\'");
        let ref_escaped = git_ref.replace('\\', "\\\\").replace('\'', "\\'");

        let script = format!(
            "window.gitWorkerManager.call('readFile', {{ dir: '{}', filepath: '{}', ref: '{}' }})",
            dir_escaped, filepath_escaped, ref_escaped
        );

        let result = Self::call_worker(&script).await?;
        result
            .as_string()
            .ok_or_else(|| "Expected string result".to_string())
    }

    /// List branches
    pub async fn get_branches(dir: &str) -> Result<Vec<String>, String> {
        let dir_escaped = dir.replace('\\', "\\\\").replace('\'', "\\'");

        let script = format!(
            "window.gitWorkerManager.call('branches', {{ dir: '{}' }})",
            dir_escaped
        );

        let result = Self::call_worker(&script).await?;
        serde_wasm_bindgen::from_value(result).map_err(|e| format!("Parse error: {:?}", e))
    }

    /// Check if repo exists in cache
    pub async fn repo_exists(dir: &str) -> bool {
        let dir_escaped = dir.replace('\\', "\\\\").replace('\'', "\\'");

        let script = format!(
            "window.gitWorkerManager.call('status', {{ dir: '{}' }})",
            dir_escaped
        );

        if let Ok(result) = Self::call_worker(&script).await {
            Reflect::get(&result, &JsValue::from_str("exists"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Internal: call worker and await result
    async fn call_worker(script: &str) -> Result<JsValue, String> {
        let promise =
            js_sys::eval(script).map_err(|e| format!("Eval failed: {:?}", e))?;
        let promise = js_sys::Promise::from(promise);
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| {
                // Try to extract error message from JsValue
                if let Some(err) = e.as_string() {
                    err
                } else if let Some(obj) = e.dyn_ref::<js_sys::Object>() {
                    if let Ok(msg) = Reflect::get(obj, &JsValue::from_str("message")) {
                        msg.as_string().unwrap_or_else(|| format!("{:?}", e))
                    } else {
                        format!("{:?}", e)
                    }
                } else {
                    format!("Worker call failed: {:?}", e)
                }
            })
    }
}
