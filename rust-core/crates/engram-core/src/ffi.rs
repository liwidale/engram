#![allow(clippy::missing_safety_doc)]

use crate::context::{build_context, ContextRequest};
use crate::error::{EngramError, Result};
use crate::graph::{Graph, LayoutOptions};
use crate::health::audit;
use crate::ops::Op;
use crate::render;
use crate::store::{Snapshot, Store};
use crate::tokens::count_tokens;
use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::Mutex;

pub struct EngramHandle {
    store: Store,
    snapshot: Mutex<Snapshot>,
}

impl EngramHandle {
    fn snapshot(&self) -> std::sync::MutexGuard<'_, Snapshot> {
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
        *h.snapshot() = fresh;
        Ok(json!({ "changed": true, "revision": revision }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn engram_apply(handle: *const EngramHandle, op_json: *const c_char) -> *mut c_char {
    envelope(|| {
        let h = handle_ref(handle)?;
        let op: Op = serde_json::from_str(str_arg(op_json, "op_json")?)
            .map_err(|e| EngramError::Invalid(format!("bad operation: {e}")))?;
        let (result, snap) = op.execute(&h.store)?;
        let revision = snap.document.revision;
        *h.snapshot() = snap;
        Ok(json!({ "result": result, "revision": revision }))
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
        let name = h.snapshot().document.project.name.clone();
        Ok(mcp_config(&name, h.store.path(), binary))
    })
}

pub fn mcp_config(project_name: &str, file: &Path, binary: &str) -> Value {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let file = file.to_string_lossy();
    let slug: String = project_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let server_name = if slug.is_empty() {
        "engram".to_owned()
    } else {
        format!("engram-{slug}")
    };
    let config = json!({
        "mcpServers": {
            server_name.clone(): { "type": "stdio", "command": binary, "args": ["--file", file] }
        }
    });
    let quote = |s: &str| {
        if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@".contains(c)) {
            s.to_owned()
        } else {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
    };
    json!({
        "server_name": server_name,
        "json": serde_json::to_string_pretty(&config).unwrap_or_default(),
        "claude_code_command": format!("claude mcp add {} -- {} --file {}", server_name, quote(binary), quote(&file)),
    })
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
}
