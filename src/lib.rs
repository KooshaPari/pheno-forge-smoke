//! Pure-Rust facade exposed to the `pheno-forge-smoke` binary.
//!
//! Thin Rust wrapper around `pheno-cdylib-bridge`'s C-ABI surface, loaded
//! at runtime via `libloading`. The cdylib is discovered through:
//!   1. `PHENO_BRIDGE_PATH` env var
//!   2. Standard system paths (`/usr/local/lib`, `/opt/homebrew/lib`)
//!   3. Same dir as the smoke binary
//!
//! All calls ultimately route through the loaded cdylib, so the smoke
//! binary never has a hard link dependency on `libpheno_bridge`.

use anyhow::{anyhow, Result};
use libloading::Library;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Supermemory,
    Letta,
    Cognee,
    Mem0,
    Composite,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::Supermemory => "sm",
            Provider::Letta => "letta",
            Provider::Cognee => "cognee",
            Provider::Mem0 => "mem0",
            Provider::Composite => "composite",
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Episodic,
    Identity,
    ProjectKnowledge,
    Fallback,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Episodic => "episodic",
            Scope::Identity => "identity",
            Scope::ProjectKnowledge => "project_knowledge",
            Scope::Fallback => "fallback",
        }
    }
}

/// A value being stored. Mirrors `thegent_memory::v2::MemoryValue`.
#[derive(Debug, Clone)]
pub enum MemoryValue {
    Text(String),
    Binary(Vec<u8>),
    Json(String),
}

/// Opaque handle to a memory port opened via the bridge.
#[derive(Clone, Copy)]
pub struct MemoryHandle(pub *mut c_void);

// SAFETY: see C-ABI contract for `*mut c_void` handles.
unsafe impl Send for MemoryHandle {}
unsafe impl Sync for MemoryHandle {}

/// Loaded bridge — owns the `Library` and typed function pointers.
pub struct Bridge {
    _lib: Library,
    f_version: unsafe extern "C" fn() -> *const c_char,
    f_last_error: unsafe extern "C" fn() -> *const c_char,
    f_string_free: unsafe extern "C" fn(*mut c_char),
    f_memory_new: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    f_memory_store: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *const c_char) -> c_int,
    f_memory_recall: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *mut *mut c_char) -> c_int,
    f_memory_forget: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    f_memory_free: unsafe extern "C" fn(*mut c_void),
}

// SAFETY: the C-ABI treats handles as opaque pointers; the Library is Send + Sync.
unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

impl Bridge {
    /// Load `libpheno_bridge` from the given path.
    pub fn load(path: &str) -> Result<Self> {
        // SAFETY: the library is a valid cdylib; symbols are looked up once at load.
        unsafe {
            let lib = Library::new(path)
                .map_err(|e| anyhow!("failed to load {}: {}", path, e))?;

            macro_rules! sym {
                ($name:literal, $ty:ty) => {{
                    let s = std::ffi::CString::new($name).unwrap();
                    *lib.get::<$ty>(s.as_bytes())
                        .map_err(|e| anyhow!("symbol {}: {}", $name, e))?
                }};
            }

            Ok(Self {
                f_version: sym!("pheno_bridge_version", unsafe extern "C" fn() -> *const c_char),
                f_last_error: sym!("pheno_last_error", unsafe extern "C" fn() -> *const c_char),
                f_string_free: sym!("pheno_string_free", unsafe extern "C" fn(*mut c_char)),
                f_memory_new: sym!("pheno_memory_new", unsafe extern "C" fn(*const c_char) -> *mut c_void),
                f_memory_store: sym!("pheno_memory_store", unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *const c_char) -> c_int),
                f_memory_recall: sym!("pheno_memory_recall", unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *mut *mut c_char) -> c_int),
                f_memory_forget: sym!("pheno_memory_forget", unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int),
                f_memory_free: sym!("pheno_memory_free", unsafe extern "C" fn(*mut c_void)),
                _lib: lib,
            })
        }
    }

    pub fn version(&self) -> Result<String> {
        // SAFETY: version returns a static `*const c_char`.
        unsafe {
            let ptr = (self.f_version)();
            if ptr.is_null() {
                Err(anyhow!("pheno_bridge_version returned null"))
            } else {
                Ok(CStr::from_ptr(ptr).to_string_lossy().into_owned())
            }
        }
    }

    pub fn last_error(&self) -> String {
        // SAFETY: last_error returns a static `*const c_char`.
        unsafe {
            let ptr = (self.f_last_error)();
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    pub fn new_handle(&self, provider: Provider) -> Result<MemoryHandle> {
        let label = CString::new(provider.label())
            .map_err(|e| anyhow!("label contained null byte: {}", e))?;
        // SAFETY: label is a valid C string; the bridge handles null returns.
        unsafe {
            let raw = (self.f_memory_new)(label.as_ptr());
            if raw.is_null() {
                Err(anyhow!(
                    "pheno_memory_new failed for provider {}: {}",
                    provider.label(),
                    self.last_error()
                ))
            } else {
                Ok(MemoryHandle(raw))
            }
        }
    }

    pub fn free_handle(&self, handle: MemoryHandle) {
        // SAFETY: handle is non-null (verified at new).
        unsafe { (self.f_memory_free)(handle.0) };
    }

    pub fn store(&self, handle: MemoryHandle, scope: Scope, key: &str, value: &MemoryValue) -> Result<()> {
        let scope_c = CString::new(scope.label()).map_err(|e| anyhow!("scope label: {}", e))?;
        let key_c = CString::new(key).map_err(|e| anyhow!("key contained null: {}", e))?;
        let (text, _kind) = match value {
            MemoryValue::Text(s) => (s.clone(), "text"),
            MemoryValue::Binary(b) => (String::from_utf8_lossy(b).into_owned(), "binary"),
            MemoryValue::Json(j) => (j.clone(), "json"),
        };
        let value_c = CString::new(text).map_err(|e| anyhow!("value contained null: {}", e))?;

        // SAFETY: all c_* are valid C strings; handle is non-null.
        let rc = unsafe {
            (self.f_memory_store)(handle.0, scope_c.as_ptr(), key_c.as_ptr(), value_c.as_ptr())
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(anyhow!(
                "store failed: rc={} scope={} key={} err={}",
                rc,
                scope.label(),
                key,
                self.last_error()
            ))
        }
    }

    pub fn recall(&self, handle: MemoryHandle, scope: Scope, query: &str) -> Result<String> {
        let scope_c = CString::new(scope.label()).map_err(|e| anyhow!("scope label: {}", e))?;
        let query_c = CString::new(query).map_err(|e| anyhow!("query contained null: {}", e))?;

        let mut out: *mut c_char = std::ptr::null_mut();
        // SAFETY: c_* are valid C strings; out is a valid output pointer.
        let rc = unsafe {
            (self.f_memory_recall)(handle.0, scope_c.as_ptr(), query_c.as_ptr(), &mut out as *mut *mut c_char)
        };
        if rc == 0 && !out.is_null() {
            // SAFETY: out is a heap-allocated c_string returned by the bridge.
            let s = unsafe { CStr::from_ptr(out).to_string_lossy().into_owned() };
            unsafe { (self.f_string_free)(out) };
            Ok(s)
        } else {
            Err(anyhow!(
                "recall failed: rc={} scope={} query={} err={}",
                rc,
                scope.label(),
                query,
                self.last_error()
            ))
        }
    }

    pub fn forget(&self, handle: MemoryHandle, scope: Scope, key: &str) -> Result<()> {
        let scope_c = CString::new(scope.label()).map_err(|e| anyhow!("scope label: {}", e))?;
        let key_c = CString::new(key).map_err(|e| anyhow!("key contained null: {}", e))?;

        // SAFETY: c_* are valid C strings; handle is non-null.
        let rc = unsafe {
            (self.f_memory_forget)(handle.0, scope_c.as_ptr(), key_c.as_ptr())
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(anyhow!(
                "forget failed: rc={} scope={} key={} err={}",
                rc,
                scope.label(),
                key,
                self.last_error()
            ))
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // The Library is dropped automatically, which dlcloses the handle.
    }
}

/// Resolve the bridge library path from an explicit path or default search.
pub fn default_bridge_path() -> std::path::PathBuf {
    // 1. PHENO_BRIDGE_PATH env var (explicit override)
    if let Ok(p) = std::env::var("PHENO_BRIDGE_PATH") {
        return std::path::PathBuf::from(p);
    }
    // 2. macOS: Homebrew prefix
    #[cfg(target_os = "macos")]
    {
        let brew = std::path::PathBuf::from("/usr/local/lib/libpheno_bridge.dylib");
        if brew.exists() {
            return brew;
        }
        let brew_arm = std::path::PathBuf::from("/opt/homebrew/lib/libpheno_bridge.dylib");
        if brew_arm.exists() {
            return brew_arm;
        }
    }
    // 3. Linux: standard lib dir
    #[cfg(target_os = "linux")]
    {
        let lib = std::path::PathBuf::from("/usr/local/lib/libpheno_bridge.so");
        if lib.exists() {
            return lib;
        }
    }
    // 4. Fallback — same dir as the smoke binary
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().unwrap_or(std::path::Path::new("."));
    dir.join(if cfg!(target_os = "macos") {
        "libpheno_bridge.dylib"
    } else {
        "libpheno_bridge.so"
    })
}