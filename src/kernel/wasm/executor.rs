//! WASM Command Executor
//!
//! Executes WASM command modules using the browser's WebAssembly API.
//! This is the core execution engine that bridges WASM modules to the kernel.

#[cfg(target_arch = "wasm32")]
use super::abi::{ArgLayout, OpenFlags, SyscallError};
#[cfg(target_arch = "wasm32")]
use super::error::WasmError;
use super::error::{CommandResult, WasmResult};
use super::runtime::Runtime;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Object, Reflect, Uint8Array, WebAssembly};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Shared state for syscall handlers
pub type SharedRuntime = Rc<RefCell<RuntimeState>>;

/// Runtime state accessible by syscall handlers
pub struct RuntimeState {
    /// The runtime environment
    pub runtime: Runtime,
    /// WASM memory reference (set after instantiation)
    pub memory: Option<WasmMemoryRef>,
    /// Whether the command has terminated
    pub terminated: bool,
}

impl RuntimeState {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            memory: None,
            terminated: false,
        }
    }
}

/// Reference to WASM linear memory for reading/writing
#[cfg(target_arch = "wasm32")]
pub struct WasmMemoryRef {
    memory: WebAssembly::Memory,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct WasmMemoryRef {
    data: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
impl WasmMemoryRef {
    pub fn new(memory: WebAssembly::Memory) -> Self {
        Self { memory }
    }

    /// Read bytes from WASM memory
    pub fn read(&self, offset: u32, len: u32) -> Vec<u8> {
        let buffer = self.memory.buffer();
        let array = Uint8Array::new(&buffer);
        let mut result = vec![0u8; len as usize];
        for (i, byte) in result.iter_mut().enumerate() {
            *byte = array.get_index(offset + i as u32);
        }
        result
    }

    /// Write bytes to WASM memory
    pub fn write(&self, offset: u32, data: &[u8]) {
        let buffer = self.memory.buffer();
        let array = Uint8Array::new(&buffer);
        for (i, &byte) in data.iter().enumerate() {
            array.set_index(offset + i as u32, byte);
        }
    }

    /// Read a null-terminated string from WASM memory
    pub fn read_string(&self, ptr: u32, max_len: u32) -> String {
        let buffer = self.memory.buffer();
        let array = Uint8Array::new(&buffer);
        let mut bytes = Vec::new();
        for i in 0..max_len {
            let byte = array.get_index(ptr + i);
            if byte == 0 {
                break;
            }
            bytes.push(byte);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Read a string with explicit length
    pub fn read_string_len(&self, ptr: u32, len: u32) -> String {
        let bytes = self.read(ptr, len);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Get memory size in bytes
    pub fn size(&self) -> u32 {
        let buffer = self.memory.buffer();
        let array_buffer: js_sys::ArrayBuffer = buffer.unchecked_into();
        array_buffer.byte_length() as u32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WasmMemoryRef {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
        }
    }

    pub fn read(&self, offset: u32, len: u32) -> Vec<u8> {
        let start = offset as usize;
        let end = start + len as usize;
        if end <= self.data.len() {
            self.data[start..end].to_vec()
        } else {
            vec![]
        }
    }

    pub fn write(&mut self, offset: u32, data: &[u8]) {
        let start = offset as usize;
        for (i, &byte) in data.iter().enumerate() {
            if start + i < self.data.len() {
                self.data[start + i] = byte;
            }
        }
    }

    pub fn read_string(&self, ptr: u32, max_len: u32) -> String {
        let start = ptr as usize;
        let mut bytes = Vec::new();
        for i in 0..max_len as usize {
            if start + i >= self.data.len() {
                break;
            }
            let byte = self.data[start + i];
            if byte == 0 {
                break;
            }
            bytes.push(byte);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn read_string_len(&self, ptr: u32, len: u32) -> String {
        let bytes = self.read(ptr, len);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn size(&self) -> u32 {
        self.data.len() as u32
    }
}

/// WASM Command Executor
///
/// Executes WASM modules by:
/// 1. Compiling the WASM bytecode
/// 2. Creating import objects with syscall implementations
/// 3. Instantiating the module
/// 4. Calling the main() function
/// 5. Capturing output and exit code
pub struct WasmExecutor {
    /// Environment variables for commands
    env: HashMap<String, String>,
    /// Current working directory
    cwd: String,
}

impl WasmExecutor {
    /// Create a new executor
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            cwd: "/".to_string(),
        }
    }

    /// Set environment variables
    pub fn set_env(&mut self, env: HashMap<String, String>) {
        self.env = env;
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, cwd: &str) {
        self.cwd = cwd.to_string();
    }

    /// Execute a WASM module with given arguments and stdin
    #[cfg(target_arch = "wasm32")]
    pub async fn execute(
        &self,
        module_bytes: &[u8],
        args: &[&str],
        stdin: &[u8],
    ) -> WasmResult<CommandResult> {
        // Create runtime with stdin and environment
        let mut runtime = Runtime::new();
        runtime.stdin = stdin.to_vec();
        runtime.set_cwd(&self.cwd);
        for (k, v) in &self.env {
            runtime.set_env(k, v);
        }

        // Create shared state
        let state = Rc::new(RefCell::new(RuntimeState::new(runtime)));

        // Compile the WASM module
        let module = self.compile_module(module_bytes).await?;

        // Create import object with syscalls
        let imports = self.create_imports(Rc::clone(&state))?;

        // Instantiate the module
        let instance = self.instantiate_module(&module, &imports).await?;

        // Get the memory export and store it in state
        let exports = instance.exports();
        let memory = Reflect::get(&exports, &JsValue::from_str("memory"))
            .map_err(|_| WasmError::MissingExport { name: "memory" })?;
        let memory: WebAssembly::Memory =
            memory.dyn_into().map_err(|_| WasmError::WrongExportType {
                name: "memory",
                expected: "Memory",
                got: "unknown".to_string(),
            })?;

        state.borrow_mut().memory = Some(WasmMemoryRef::new(memory.clone()));

        // Set up arguments in WASM memory
        let (argc, argv) = self.setup_args(&state, args)?;

        // Get and call main function
        let main_fn = Reflect::get(&exports, &JsValue::from_str("main"))
            .map_err(|_| WasmError::MissingExport { name: "main" })?;
        let main_fn: Function = main_fn.dyn_into().map_err(|_| WasmError::WrongExportType {
            name: "main",
            expected: "Function",
            got: "unknown".to_string(),
        })?;

        // Call main(argc, argv)
        let result = main_fn.call2(&JsValue::NULL, &JsValue::from(argc), &JsValue::from(argv));

        let exit_code = match result {
            Ok(val) => val.as_f64().unwrap_or(0.0) as i32,
            Err(e) => {
                // Check if it's an exit() call
                let state_ref = state.borrow();
                if state_ref.terminated {
                    state_ref.runtime.exit_code().unwrap_or(1)
                } else {
                    // Actual trap/error
                    let msg = e.as_string().unwrap_or_else(|| "unknown error".to_string());
                    return Err(WasmError::Aborted { reason: msg });
                }
            }
        };

        // Extract results
        let state_ref = state.borrow();
        let final_code = state_ref.runtime.exit_code().unwrap_or(exit_code);

        Ok(CommandResult {
            exit_code: final_code,
            stdout: state_ref.runtime.stdout().to_vec(),
            stderr: state_ref.runtime.stderr().to_vec(),
        })
    }

    /// Execute a WASM module (non-WASM target stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute(
        &self,
        _module_bytes: &[u8],
        _args: &[&str],
        _stdin: &[u8],
    ) -> WasmResult<CommandResult> {
        // For native builds (testing), return a stub result
        Ok(CommandResult::success())
    }

    /// Compile WASM bytecode into a module
    #[cfg(target_arch = "wasm32")]
    async fn compile_module(&self, bytes: &[u8]) -> WasmResult<WebAssembly::Module> {
        let array = Uint8Array::new_with_length(bytes.len() as u32);
        array.copy_from(bytes);

        let promise = WebAssembly::compile(&array.buffer());
        let result = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| WasmError::InstantiationFailed {
                reason: e
                    .as_string()
                    .unwrap_or_else(|| "compilation failed".to_string()),
            })?;

        result
            .dyn_into::<WebAssembly::Module>()
            .map_err(|_| WasmError::InstantiationFailed {
                reason: "failed to cast to Module".to_string(),
            })
    }

    /// Create import object with syscall implementations
    #[cfg(target_arch = "wasm32")]
    fn create_imports(&self, state: SharedRuntime) -> WasmResult<Object> {
        let imports = Object::new();
        let env = Object::new();

        // Create syscall closures
        self.add_syscall_write(&env, Rc::clone(&state))?;
        self.add_syscall_read(&env, Rc::clone(&state))?;
        self.add_syscall_open(&env, Rc::clone(&state))?;
        self.add_syscall_close(&env, Rc::clone(&state))?;
        self.add_syscall_exit(&env, Rc::clone(&state))?;
        self.add_syscall_getenv(&env, Rc::clone(&state))?;
        self.add_syscall_getcwd(&env, Rc::clone(&state))?;
        self.add_syscall_stat(&env, Rc::clone(&state))?;
        self.add_syscall_mkdir(&env, Rc::clone(&state))?;
        self.add_syscall_readdir(&env, Rc::clone(&state))?;
        self.add_syscall_rmdir(&env, Rc::clone(&state))?;
        self.add_syscall_unlink(&env, Rc::clone(&state))?;
        self.add_syscall_rename(&env, Rc::clone(&state))?;

        Reflect::set(&imports, &JsValue::from_str("env"), &env).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set env imports".to_string(),
            }
        })?;

        Ok(imports)
    }

    /// Add write syscall: write(fd, buf_ptr, len) -> bytes_written
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_write(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(move |fd: i32, buf_ptr: i32, len: i32| -> i32 {
            let state_ref = state.borrow();
            if let Some(ref memory) = state_ref.memory {
                let data = memory.read(buf_ptr as u32, len as u32);
                drop(state_ref);
                state.borrow_mut().runtime.sys_write(fd, &data)
            } else {
                SyscallError::Generic.code()
            }
        }) as Box<dyn Fn(i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("write"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set write import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add read syscall: read(fd, buf_ptr, len) -> bytes_read
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_read(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(move |fd: i32, buf_ptr: i32, len: i32| -> i32 {
            let mut buf = vec![0u8; len as usize];
            let result = state.borrow_mut().runtime.sys_read(fd, &mut buf);
            if result > 0 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    memory.write(buf_ptr as u32, &buf[..result as usize]);
                }
            }
            result
        }) as Box<dyn Fn(i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("read"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set read import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add open syscall: open(path_ptr, path_len, flags) -> fd
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_open(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(
            move |path_ptr: i32, path_len: i32, flags: i32| -> i32 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                    drop(state_ref);
                    state.borrow_mut().runtime.sys_open(&path, OpenFlags(flags))
                } else {
                    SyscallError::Generic.code()
                }
            },
        ) as Box<dyn Fn(i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("open"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set open import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add close syscall: close(fd) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_close(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(move |fd: i32| -> i32 {
            state.borrow_mut().runtime.sys_close(fd)
        }) as Box<dyn Fn(i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("close"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set close import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add exit syscall: exit(code) -> !
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_exit(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(move |code: i32| {
            let mut state_mut = state.borrow_mut();
            state_mut.runtime.sys_exit(code);
            state_mut.terminated = true;
            // Throw to unwind the WASM execution
            // The caller will check terminated flag
        }) as Box<dyn Fn(i32)>);

        Reflect::set(env, &JsValue::from_str("exit"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set exit import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add getenv syscall: getenv(name_ptr, name_len, buf_ptr, buf_len) -> len or 0
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_getenv(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(
            move |name_ptr: i32, name_len: i32, buf_ptr: i32, buf_len: i32| -> i32 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    let name = memory.read_string_len(name_ptr as u32, name_len as u32);
                    if let Some(value) = state_ref.runtime.sys_getenv(&name) {
                        let value_bytes = value.as_bytes();
                        let write_len = std::cmp::min(value_bytes.len(), buf_len as usize);
                        memory.write(buf_ptr as u32, &value_bytes[..write_len]);
                        write_len as i32
                    } else {
                        0
                    }
                } else {
                    SyscallError::Generic.code()
                }
            },
        ) as Box<dyn Fn(i32, i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("getenv"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set getenv import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add getcwd syscall: getcwd(buf_ptr, buf_len) -> len or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_getcwd(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(move |buf_ptr: i32, buf_len: i32| -> i32 {
            let state_ref = state.borrow();
            if let Some(ref memory) = state_ref.memory {
                let cwd = state_ref.runtime.sys_getcwd();
                let cwd_bytes = cwd.as_bytes();
                let write_len = std::cmp::min(cwd_bytes.len(), buf_len as usize);
                memory.write(buf_ptr as u32, &cwd_bytes[..write_len]);
                write_len as i32
            } else {
                SyscallError::Generic.code()
            }
        }) as Box<dyn Fn(i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("getcwd"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set getcwd import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add stat syscall: stat(path_ptr, path_len, stat_buf) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_stat(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        let closure = Closure::wrap(Box::new(
            move |path_ptr: i32, path_len: i32, stat_buf: i32| -> i32 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                    match state_ref.runtime.sys_stat(&path) {
                        Ok(stat) => {
                            memory.write(stat_buf as u32, &stat.to_bytes());
                            0
                        }
                        Err(e) => e.code(),
                    }
                } else {
                    SyscallError::Generic.code()
                }
            },
        ) as Box<dyn Fn(i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("stat"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set stat import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add mkdir syscall: mkdir(path_ptr, path_len) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_mkdir(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        use crate::kernel::syscall as ksyscall;

        let closure = Closure::wrap(Box::new(move |path_ptr: i32, path_len: i32| -> i32 {
            let state_ref = state.borrow();
            if let Some(ref memory) = state_ref.memory {
                let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                match ksyscall::mkdir(&path) {
                    Ok(()) => 0,
                    Err(_) => SyscallError::Generic.code(),
                }
            } else {
                SyscallError::Generic.code()
            }
        }) as Box<dyn Fn(i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("mkdir"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set mkdir import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add readdir syscall: readdir(path_ptr, path_len, buf_ptr, buf_len) -> bytes or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_readdir(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        use crate::kernel::syscall as ksyscall;

        let closure = Closure::wrap(Box::new(
            move |path_ptr: i32, path_len: i32, buf_ptr: i32, buf_len: i32| -> i32 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                    match ksyscall::readdir(&path) {
                        Ok(entries) => {
                            // Format as null-terminated strings
                            let mut output = Vec::new();
                            for entry in entries {
                                if output.len() + entry.len() + 1 > buf_len as usize {
                                    break;
                                }
                                output.extend_from_slice(entry.as_bytes());
                                output.push(0);
                            }
                            let write_len = std::cmp::min(output.len(), buf_len as usize);
                            memory.write(buf_ptr as u32, &output[..write_len]);
                            write_len as i32
                        }
                        Err(_) => SyscallError::NotFound.code(),
                    }
                } else {
                    SyscallError::Generic.code()
                }
            },
        ) as Box<dyn Fn(i32, i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("readdir"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set readdir import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add rmdir syscall: rmdir(path_ptr, path_len) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_rmdir(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        use crate::kernel::syscall as ksyscall;

        let closure = Closure::wrap(Box::new(move |path_ptr: i32, path_len: i32| -> i32 {
            let state_ref = state.borrow();
            if let Some(ref memory) = state_ref.memory {
                let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                match ksyscall::rmdir(&path) {
                    Ok(()) => 0,
                    Err(_) => SyscallError::Generic.code(),
                }
            } else {
                SyscallError::Generic.code()
            }
        }) as Box<dyn Fn(i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("rmdir"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set rmdir import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add unlink syscall: unlink(path_ptr, path_len) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_unlink(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        use crate::kernel::syscall as ksyscall;

        let closure = Closure::wrap(Box::new(move |path_ptr: i32, path_len: i32| -> i32 {
            let state_ref = state.borrow();
            if let Some(ref memory) = state_ref.memory {
                let path = memory.read_string_len(path_ptr as u32, path_len as u32);
                match ksyscall::unlink(&path) {
                    Ok(()) => 0,
                    Err(_) => SyscallError::Generic.code(),
                }
            } else {
                SyscallError::Generic.code()
            }
        }) as Box<dyn Fn(i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("unlink"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set unlink import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Add rename syscall: rename(from_ptr, from_len, to_ptr, to_len) -> 0 or error
    #[cfg(target_arch = "wasm32")]
    fn add_syscall_rename(&self, env: &Object, state: SharedRuntime) -> WasmResult<()> {
        use crate::kernel::syscall as ksyscall;

        let closure = Closure::wrap(Box::new(
            move |from_ptr: i32, from_len: i32, to_ptr: i32, to_len: i32| -> i32 {
                let state_ref = state.borrow();
                if let Some(ref memory) = state_ref.memory {
                    let from = memory.read_string_len(from_ptr as u32, from_len as u32);
                    let to = memory.read_string_len(to_ptr as u32, to_len as u32);
                    match ksyscall::rename(&from, &to) {
                        Ok(()) => 0,
                        Err(_) => SyscallError::Generic.code(),
                    }
                } else {
                    SyscallError::Generic.code()
                }
            },
        ) as Box<dyn Fn(i32, i32, i32, i32) -> i32>);

        Reflect::set(env, &JsValue::from_str("rename"), closure.as_ref()).map_err(|_| {
            WasmError::InstantiationFailed {
                reason: "failed to set rename import".to_string(),
            }
        })?;
        closure.forget();
        Ok(())
    }

    /// Instantiate a compiled module with imports
    #[cfg(target_arch = "wasm32")]
    async fn instantiate_module(
        &self,
        module: &WebAssembly::Module,
        imports: &Object,
    ) -> WasmResult<WebAssembly::Instance> {
        let promise = WebAssembly::instantiate_module(module, imports);
        let result = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| WasmError::InstantiationFailed {
                reason: e
                    .as_string()
                    .unwrap_or_else(|| "instantiation failed".to_string()),
            })?;

        result
            .dyn_into::<WebAssembly::Instance>()
            .map_err(|_| WasmError::InstantiationFailed {
                reason: "failed to cast to Instance".to_string(),
            })
    }

    /// Set up command arguments in WASM memory
    #[cfg(target_arch = "wasm32")]
    fn setup_args(&self, state: &SharedRuntime, args: &[&str]) -> WasmResult<(i32, i32)> {
        let state_ref = state.borrow();
        let memory = state_ref
            .memory
            .as_ref()
            .ok_or(WasmError::InstantiationFailed {
                reason: "memory not available".to_string(),
            })?;

        let layout = ArgLayout::new(args);
        let total_size = layout.total_size();

        // Allocate at the end of the first page (after potential data)
        // In a real implementation, we'd use __heap_base
        let base_addr = (memory.size() - total_size as u32 - 256).max(1024);

        let mut buf = vec![0u8; total_size];
        let argv_ptr = layout.write_to(args, base_addr, &mut buf);

        // Write to WASM memory
        memory.write(base_addr, &buf);

        Ok((args.len() as i32, argv_ptr as i32))
    }
}

impl Default for WasmExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_new() {
        let exec = WasmExecutor::new();
        assert_eq!(exec.cwd, "/");
    }

    #[test]
    fn test_executor_set_env() {
        let mut exec = WasmExecutor::new();
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/user".to_string());
        exec.set_env(env);
        assert_eq!(exec.env.get("HOME"), Some(&"/home/user".to_string()));
    }

    #[test]
    fn test_executor_set_cwd() {
        let mut exec = WasmExecutor::new();
        exec.set_cwd("/tmp");
        assert_eq!(exec.cwd, "/tmp");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_wasm_memory_ref() {
        let mut mem = WasmMemoryRef::new(65536);
        mem.write(0, b"hello\0world");
        let s = mem.read_string(0, 20);
        assert_eq!(s, "hello");
    }
}
