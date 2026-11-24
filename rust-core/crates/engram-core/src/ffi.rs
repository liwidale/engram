#![allow(clippy::missing_safety_doc)]

use crate::client_config::{self, McpClient};
use crate::context::{build_context, ContextRequest};
use crate::diff::diff;
use crate::error::{EngramError, Result};
use crate::graph::{Graph, LayoutOptions};
use crate::health::{audit, repo_files, resolve_repo_root};
use crate::history::History;
use crate::interop;
use crate::ops::Op;
use crate::render;
use crate::schema::EngramDocument;
use crate::store::{Snapshot, Store};
use crate::tokens::count_tokens;
use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub struct EngramHandle {
    store: Store,
    snapshot: Mutex<Snapshot>,
    history: Mutex<History>,
    files: Mutex<Option<(Instant, Vec<String>)>>,
}

impl EngramHandle {
    fn history(&self) -> MutexGuard<'_, History> {
        self.history.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn repo_root(&self) -> PathBuf {
        resolve_repo_root(&self.snapshot().document, &self.store.base_dir(), None).0
    }

    fn snapshot(&self) -> MutexGuard<'_, Snapshot> {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn to_c(s: String) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

fn ok<T: Serialize>(data: T) -> String {
    json!({ "ok": true, "data": data }).to_string()
}

fn err(e: &EngramError) -> String {
    json!({ "ok": false, "error": { "code": e.code(), "message": e.to_string() } }).to_string()
}

fn envelope<T: Serialize>(f: impl FnOnce() -> Result<T>) -> *mut c_char {
    let out = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => ok(v),
        Ok(Err(e)) => err(&e),
        Err(_) => {
            json!({ "ok": false, "error": { "code": "panic", "message": "internal error in engram-core" } }).to_string()
        }
    };
    to_c(out)
}

unsafe fn str_arg<'a>(p: *const c_char, name: &str) -> Result<&'a str> {
    if p.is_null() {
        return Err(EngramError::Invalid(format!("{name} is null")));
    }
    CStr::from_ptr(p)
        .to_str()
        .map_err(|_| EngramError::Invalid(format!("{name} is not valid UTF-8")))
}

unsafe fn opt_str_arg<'a>(p: *const c_char) -> Option<&'a str> {
    (!p.is_null())
        .then(|| CStr::from_ptr(p).to_str().ok())
        .flatten()
        .filter(|s| !s.is_empty())
}

unsafe fn handle_ref<'a>(h: *const EngramHandle) -> Result<&'a EngramHandle> {
    h.as_ref()
        .ok_or_else(|| EngramError::Invalid("engram handle is null".into()))
}

unsafe fn open_with(out_error: *mut *mut c_char, f: impl FnOnce() -> Result<(Store, Snapshot)>) -> *mut EngramHandle {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok((store, snapshot))) => Box::into_raw(Box::new(EngramHandle {
            store,
            snapshot: Mutex::new(snapshot),
            history: Mutex::default(),
            files: Mutex::default(),
        })),
        Ok(Err(e)) => {
            if !out_error.is_null() {
                *out_error = to_c(err(&e));
            }
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn engram_open(path: *const c_char, out_error: *mut *mut c_char) -> *mut EngramHandle {
    open_with(out_error, || {
        let store = Store::new(str_arg(path, "path")?);
        let snap = store.load()?;
        Ok((store, snap))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_create(
    path: *const c_char,
    project_name: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut EngramHandle {
    open_with(out_error, || {
        let store = Store::new(str_arg(path, "path")?);
        store.create(str_arg(project_name, "project_name")?)?;
        let snap = store.load()?;
        Ok((store, snap))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_close(handle: *mut EngramHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

#[no_mangle]
pub unsafe extern "C" fn engram_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[no_mangle]
pub extern "C" fn engram_version() -> *mut c_char {
    to_c(env!("CARGO_PKG_VERSION").to_owned())
}

#[no_mangle]
pub unsafe extern "C" fn engram_count_tokens(text: *const c_char) -> u64 {
    match str_arg(text, "text") {
        Ok(t) => catch_unwind(|| count_tokens(t) as u64).unwrap_or(0),
        Err(_) => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn engram_document(handle: *const EngramHandle) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let snap = h.snapshot();
        Ok(json!({ "path": h.store.path(), "revision": snap.document.revision, "document": snap.document }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_reload(handle: *const EngramHandle) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let current = h.snapshot().fingerprint;
        if h.store.disk_fingerprint() == Some(current) {
            return Ok(json!({ "changed": false, "revision": h.snapshot().document.revision }));
        }
        let fresh = h.store.load()?;
        let revision = fresh.document.revision;
        let changes = diff(&h.snapshot().document, &fresh.document);
        *h.snapshot() = fresh;
        Ok(json!({ "changed": true, "revision": revision, "diff": changes }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_apply(handle: *const EngramHandle, op_json: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let value: Value = serde_json::from_str(str_arg(op_json, "op_json")?)?;
        let label = Op::label(value.get("op").and_then(Value::as_str).unwrap_or_default());
        let op: Op = serde_json::from_value(value).map_err(|e| EngramError::Invalid(format!("bad operation: {e}")))?;
        let (result, before, snap) = op.execute_tracked(&h.store)?;
        let revision = snap.document.revision;
        let changes = diff(&before, &snap.document);
        h.history().record(before, &snap, label);
        *h.snapshot() = snap;
        Ok(json!({ "result": result, "revision": revision, "diff": changes, "history": h.history().state() }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_build_context(handle: *const EngramHandle, request_json: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let req: ContextRequest = serde_json::from_str(str_arg(request_json, "request_json")?)
            .map_err(|e| EngramError::Invalid(format!("bad context request: {e}")))?;
        let doc = h.snapshot().document.clone();
        Ok(build_context(&doc, &req))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_audit(handle: *const EngramHandle, repo_root: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let doc = h.snapshot().document.clone();
        Ok(audit(&doc, &h.store.base_dir(), opt_str_arg(repo_root)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_layout(handle: *const EngramHandle) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let snap = h.snapshot();
        Ok(Graph::new(&snap.document.architecture).layout(LayoutOptions::default()))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_render(handle: *const EngramHandle, what: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let doc = h.snapshot().document.clone();
        Ok(match str_arg(what, "what")? {
            "memory" => render::memory_markdown(&doc),
            "decisions" => render::decisions_markdown(&doc),
            "handoff" => render::handoff_markdown(&doc),
            "handoff_xml" => render::handoff_xml(&doc),
            "architecture" => serde_json::to_string_pretty(&render::architecture_json(&doc))?,
            "prompts" => serde_json::to_string_pretty(&render::prompts_json(&doc))?,
            "mermaid" => interop::architecture_mermaid(&doc),
            "agents_pointer" => {
                let file = h
                    .store
                    .path()
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                interop::agents_pointer(&doc, &file, &client_config::server_name_for(&doc))
            }
            other => return Err(EngramError::Invalid(format!("unknown render target '{other}'"))),
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_render_prompt(
    handle: *const EngramHandle,
    name: *const c_char,
    vars_json: *const c_char,
) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let name = str_arg(name, "name")?;
        let vars = match opt_str_arg(vars_json) {
            Some(s) => match serde_json::from_str::<Value>(s)? {
                Value::Object(m) => m,
                _ => return Err(EngramError::Invalid("vars_json must be an object".into())),
            },
            None => Default::default(),
        };
        let snap = h.snapshot();
        let p = snap
            .document
            .prompt(name)
            .ok_or_else(|| EngramError::NotFound(format!("prompt '{name}'")))?;
        let r = p.render(&vars);
        Ok(json!({ "text": r.text, "missing": r.missing, "variables": p.variables() }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_mcp_config(handle: *const EngramHandle, server_binary: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let binary = str_arg(server_binary, "server_binary")?;
        let name = client_config::server_name_for(&h.snapshot().document);
        Ok(client_config::mcp_config(&name, h.store.path(), binary))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_replace(
    handle: *const EngramHandle,
    doc_json: *const c_char,
    expected_revision: i64,
) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let doc: EngramDocument = serde_json::from_str(str_arg(doc_json, "doc_json")?)
            .map_err(|e| EngramError::Invalid(format!("bad document: {e}")))?;
        let before = h.snapshot().document.clone();
        let expected = u64::try_from(expected_revision).ok();
        let snap = h.store.replace(doc, expected)?;
        let revision = snap.document.revision;
        h.history().record(before, &snap, "Replace document");
        *h.snapshot() = snap;
        Ok(json!({ "revision": revision, "history": h.history().state() }))
    })
}

unsafe fn history_step(handle: *const EngramHandle, undo: bool) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let current = h.snapshot().document.clone();
        let label = {
            let state = h.history().state();
            if undo {
                state.undo_label
            } else {
                state.redo_label
            }
        };
        let snap = {
            let mut history = h.history();
            if undo {
                history.undo(&h.store, &current)
            } else {
                history.redo(&h.store, &current)
            }
        }?;
        let revision = snap.document.revision;
        let changes = diff(&current, &snap.document);
        *h.snapshot() = snap;
        Ok(json!({ "revision": revision, "label": label, "diff": changes, "history": h.history().state() }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_undo(handle: *const EngramHandle) -> *mut c_char {
    history_step(handle, true)
}

#[no_mangle]
pub unsafe extern "C" fn engram_redo(handle: *const EngramHandle) -> *mut c_char {
    history_step(handle, false)
}

#[no_mangle]
pub unsafe extern "C" fn engram_history(handle: *const EngramHandle) -> *mut c_char {
    envelope(|| Ok(handle_ref(handle)?.history().state()))
}

#[no_mangle]
pub unsafe extern "C" fn engram_client_configs(
    handle: *const EngramHandle,
    server_binary: *const c_char,
) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let binary = str_arg(server_binary, "server_binary")?;
        let doc = h.snapshot().document.clone();
        Ok(client_config::client_configs(
            &doc,
            binary,
            h.store.path(),
            &h.repo_root(),
        ))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_write_client_config(
    handle: *const EngramHandle,
    client: *const c_char,
    server_binary: *const c_char,
) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let client: McpClient = serde_json::from_value(json!(str_arg(client, "client")?))
            .map_err(|_| EngramError::Invalid("unknown MCP client".into()))?;
        let binary = str_arg(server_binary, "server_binary")?;
        let name = client_config::server_name_for(&h.snapshot().document);
        let config = client_config::client_config(client, &name, binary, h.store.path(), &h.repo_root());
        client_config::write_client_config(&config)
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_import(handle: *const EngramHandle, path: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let doc = h.snapshot().document.clone();
        interop::import_file(std::path::Path::new(str_arg(path, "path")?), &doc)
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_bootstrap(handle: *const EngramHandle) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let doc = h.snapshot().document.clone();
        Ok(interop::bootstrap(&h.repo_root(), &doc))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_repo_files(
    handle: *const EngramHandle,
    query: *const c_char,
    limit: u32,
) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let query = opt_str_arg(query).unwrap_or_default().to_lowercase();
        let mut cache = h.files.lock().unwrap_or_else(|e| e.into_inner());
        if cache
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(10))
        {
            let ignore = h.snapshot().document.project.settings.health_ignore_dirs.clone();
            *cache = Some((Instant::now(), repo_files(&h.repo_root(), &ignore)));
        }
        let files = cache.as_ref().map(|(_, f)| f.as_slice()).unwrap_or_default();
        Ok(rank_files(files, &query, limit as usize))
    })
}

pub fn rank_files(files: &[String], query: &str, limit: usize) -> Vec<String> {
    let query = query.trim().to_lowercase();
    let mut hits: Vec<(usize, &String)> = files
        .iter()
        .filter_map(|f| {
            let lower = f.to_lowercase();
            let name = lower.rsplit('/').next().unwrap_or(&lower);
            let rank = if query.is_empty() {
                2
            } else if name.starts_with(&query) {
                0
            } else if lower.starts_with(&query) || name.contains(&query) {
                1
            } else if lower.contains(&query) {
                2
            } else {
                return None;
            };
            Some((rank, f))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())).then(a.1.cmp(b.1)));
    hits.into_iter().take(limit.max(1)).map(|(_, f)| f.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn take(p: *mut c_char) -> Value {
        assert!(!p.is_null());
        let v = serde_json::from_str(CStr::from_ptr(p).to_str().unwrap()).unwrap();
        engram_string_free(p);
        v
    }

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn full_ffi_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("My App.engram");
        let path_c = c(path.to_str().unwrap());
        unsafe {
            let mut e: *mut c_char = std::ptr::null_mut();
            assert!(engram_open(path_c.as_ptr(), &mut e).is_null());
            assert_eq!(take(e)["error"]["code"], "io");

            let h = engram_create(path_c.as_ptr(), c("My App").as_ptr(), std::ptr::null_mut());
            assert!(!h.is_null());
            let doc = take(engram_document(h));
            assert_eq!(doc["data"]["revision"], 1);

            let r = take(engram_apply(
                h,
                c(r#"{"op":"upsert_memory","key":"k","category":"rule","content":"Always validate input"}"#).as_ptr(),
            ));
            assert_eq!(r["data"]["revision"], 2);
            let r = take(engram_apply(h, c(r#"{"op":"delete_memory","key":"nope"}"#).as_ptr()));
            assert_eq!(r["error"]["code"], "not_found");
            let r = take(engram_apply(h, c("{\"op\":\"bogus\"}").as_ptr()));
            assert_eq!(r["error"]["code"], "invalid");

            Op::UpsertPrompt(serde_json::from_value(json!({"name":"p","body":"Hi {{who}}"})).unwrap())
                .execute(&Store::new(&path))
                .unwrap();
            assert_eq!(take(engram_reload(h))["data"]["changed"], true);
            assert_eq!(take(engram_reload(h))["data"]["changed"], false);

            let p = take(engram_render_prompt(
                h,
                c("p").as_ptr(),
                c(r#"{"who":"there"}"#).as_ptr(),
            ));
            assert_eq!(p["data"]["text"], "Hi there");
            let ctx = take(engram_build_context(
                h,
                c(r#"{"task":"validate input","max_tokens":1000}"#).as_ptr(),
            ));
            assert!(ctx["data"]["xml"].as_str().unwrap().contains("Always validate input"));
            assert!(take(engram_audit(h, std::ptr::null()))["data"]["score"]
                .as_u64()
                .is_some());
            assert!(take(engram_layout(h))["data"]["nodes"].is_array());
            assert!(take(engram_render(h, c("memory").as_ptr()))["data"]
                .as_str()
                .unwrap()
                .contains("Always validate input"));

            let cfg = take(engram_mcp_config(
                h,
                c("/Applications/Engram.app/Contents/MacOS/engram-mcp").as_ptr(),
            ));
            assert_eq!(cfg["data"]["server_name"], "engram-my-app");
            assert!(cfg["data"]["claude_code_command"]
                .as_str()
                .unwrap()
                .contains("--file '/"));
            let cfg_json: Value = serde_json::from_str(cfg["data"]["json"].as_str().unwrap()).unwrap();
            assert_eq!(cfg_json["mcpServers"]["engram-my-app"]["args"][0], "--file");

            assert!(engram_count_tokens(c("hello world").as_ptr()) >= 2);
            assert_eq!(engram_count_tokens(std::ptr::null()), 0);
            engram_close(h);
        }
    }

    #[test]
    fn history_diff_and_interop_over_ffi() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.engram");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nedition = \"2021\"\n[dependencies]\ntokio = \"1\"\n",
        )
        .unwrap();
        let path_c = c(path.to_str().unwrap());
        unsafe {
            let h = engram_create(path_c.as_ptr(), c("P").as_ptr(), std::ptr::null_mut());
            let r = take(engram_apply(
                h,
                c(r#"{"op":"upsert_memory","key":"k","category":"rule","content":"x"}"#).as_ptr(),
            ));
            assert_eq!(r["data"]["diff"]["changes"][0]["kind"], "added");
            assert_eq!(r["data"]["history"]["undo_label"], "Edit memory");

            let u = take(engram_undo(h));
            assert_eq!(u["data"]["label"], "Edit memory");
            assert_eq!(u["data"]["diff"]["changes"][0]["kind"], "deleted");
            assert_eq!(take(engram_history(h))["data"]["can_redo"], true);
            take(engram_redo(h));
            assert!(take(engram_document(h))["data"]["document"]["memory"][0]["key"] == "k");

            Op::UpsertPrompt(serde_json::from_value(json!({"name":"p","body":"b"})).unwrap())
                .execute(&Store::new(&path))
                .unwrap();
            let reload = take(engram_reload(h));
            assert_eq!(reload["data"]["diff"]["changes"][0]["entity"], "prompt");
            let blocked = take(engram_undo(h));
            assert_eq!(blocked["error"]["code"], "conflict");

            let doc = take(engram_document(h))["data"]["document"].clone();
            let rev = doc["revision"].as_i64().unwrap();
            let mut edited = doc.clone();
            edited["memory"] = json!([]);
            let stale = take(engram_replace(h, c(&edited.to_string()).as_ptr(), rev - 1));
            assert_eq!(stale["error"]["code"], "conflict");
            let ok = take(engram_replace(h, c(&edited.to_string()).as_ptr(), rev));
            assert_eq!(ok["data"]["revision"], rev + 1);

            let configs = take(engram_client_configs(h, c("/bin/engram-mcp").as_ptr()));
            assert_eq!(configs["data"].as_array().unwrap().len(), 6);
            let written = take(engram_write_client_config(
                h,
                c("cursor").as_ptr(),
                c("/bin/engram-mcp").as_ptr(),
            ));
            assert!(written["data"]["path"].as_str().unwrap().ends_with(".cursor/mcp.json"));
            assert_eq!(
                take(engram_write_client_config(h, c("nope").as_ptr(), c("x").as_ptr()))["error"]["code"],
                "invalid"
            );

            let boot = take(engram_bootstrap(h));
            assert!(boot["data"]["memory"][0]["content"].as_str().unwrap().contains("Tokio"));
            assert!(take(engram_render(h, c("mermaid").as_ptr()))["data"]
                .as_str()
                .unwrap()
                .starts_with("flowchart TD"));
            assert!(take(engram_render(h, c("agents_pointer").as_ptr()))["data"]
                .as_str()
                .unwrap()
                .contains("`p.engram`"));
            let files = take(engram_repo_files(h, c("cargo").as_ptr(), 5));
            assert_eq!(files["data"], json!(["Cargo.toml"]));
            engram_close(h);
        }
    }

    #[test]
    fn ranks_repo_files() {
        let files: Vec<String> = ["src/main.rs", "src/app/main_view.rs", "docs/main.md", "README.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            rank_files(&files, "main", 10),
            vec!["src/main.rs", "docs/main.md", "src/app/main_view.rs"]
        );
        assert_eq!(
            rank_files(&files, "src/", 10),
            vec!["src/main.rs", "src/app/main_view.rs"]
        );
        assert_eq!(rank_files(&files, "", 2).len(), 2);
    }
}
