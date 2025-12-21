//! Tests for the WASM loader
//!
//! These tests are written BEFORE the implementation (TDD).
//! They define the expected behavior based on the ABI specification.

use super::*;

// =============================================================================
// WASM Module Builder (for tests)
// =============================================================================

/// Helper to build valid WASM modules for testing
struct WasmBuilder {
    bytes: Vec<u8>,
}

impl WasmBuilder {
    fn new() -> Self {
        Self {
            bytes: vec![
                0x00, 0x61, 0x73, 0x6D, // magic: \0asm
                0x01, 0x00, 0x00, 0x00, // version: 1
            ],
        }
    }

    fn push_leb128(bytes: &mut Vec<u8>, mut value: u32) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                bytes.push(byte);
                break;
            } else {
                bytes.push(byte | 0x80);
            }
        }
    }

    fn section(mut self, id: u8, content: &[u8]) -> Self {
        self.bytes.push(id);
        Self::push_leb128(&mut self.bytes, content.len() as u32);
        self.bytes.extend_from_slice(content);
        self
    }

    /// Add type section with main signature: (i32, i32) -> i32
    fn type_section_main(self) -> Self {
        // 1 type: (i32, i32) -> i32
        let content = vec![
            0x01, // 1 type
            0x60, // func type
            0x02, 0x7F, 0x7F, // 2 params: i32, i32
            0x01, 0x7F, // 1 result: i32
        ];
        self.section(0x01, &content)
    }

    /// Add function section (1 function using type 0)
    fn function_section(self) -> Self {
        let content = vec![0x01, 0x00]; // 1 func, type 0
        self.section(0x03, &content)
    }

    /// Add memory section (1 page)
    fn memory_section(self) -> Self {
        let content = vec![0x01, 0x00, 0x01]; // 1 memory, no max, 1 page min
        self.section(0x05, &content)
    }

    /// Add export section with memory and main
    fn export_section(self) -> Self {
        let mut content = Vec::new();
        content.push(0x02); // 2 exports

        // Export "memory" (memory index 0)
        content.push(0x06); // name length
        content.extend_from_slice(b"memory");
        content.push(0x02); // kind = memory
        content.push(0x00); // index = 0

        // Export "main" (func index 0)
        content.push(0x04); // name length
        content.extend_from_slice(b"main");
        content.push(0x00); // kind = func
        content.push(0x00); // index = 0

        self.section(0x07, &content)
    }

    /// Add code section with a function that returns given value
    fn code_section_return(self, return_value: i32) -> Self {
        let mut func_body = Vec::new();
        func_body.push(0x00); // 0 locals

        // i32.const <value>
        func_body.push(0x41);
        // Write value as signed LEB128
        let mut value = return_value as u32;
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if (value == 0 && byte & 0x40 == 0) || (value == 0x1FFFFFF && byte & 0x40 != 0) {
                func_body.push(byte);
                break;
            } else {
                func_body.push(byte | 0x80);
            }
        }

        func_body.push(0x0B); // end

        let mut content = Vec::new();
        content.push(0x01); // 1 function

        // Function body with size prefix
        let body_size = func_body.len() as u32;
        Self::push_leb128(&mut content, body_size);
        content.extend_from_slice(&func_body);

        self.section(0x0A, &content)
    }

    fn build(self) -> Vec<u8> {
        self.bytes
    }
}

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a minimal valid WASM module that exports memory and main
fn minimal_wasm_module() -> Vec<u8> {
    WasmBuilder::new()
        .type_section_main()
        .function_section()
        .memory_section()
        .export_section()
        .code_section_return(0)
        .build()
}

/// Create a WASM module with non-zero exit
fn exit_code_module(code: i32) -> Vec<u8> {
    WasmBuilder::new()
        .type_section_main()
        .function_section()
        .memory_section()
        .export_section()
        .code_section_return(code)
        .build()
}

// =============================================================================
// ABI Type Tests
// =============================================================================

mod abi_tests {
    use super::super::abi::*;

    #[test]
    fn test_abi_version() {
        assert_eq!(ABI_VERSION, 1);
    }

    #[test]
    fn test_export_names() {
        assert_eq!(exports::MEMORY, "memory");
        assert_eq!(exports::MAIN, "main");
        assert_eq!(exports::HEAP_BASE, "__heap_base");
    }

    #[test]
    fn test_syscall_names() {
        assert_eq!(syscalls::OPEN, "open");
        assert_eq!(syscalls::WRITE, "write");
        assert_eq!(syscalls::READ, "read");
        assert_eq!(syscalls::CLOSE, "close");
        assert_eq!(syscalls::EXIT, "exit");
    }

    #[test]
    fn test_standard_fds() {
        assert_eq!(fd::STDIN, 0);
        assert_eq!(fd::STDOUT, 1);
        assert_eq!(fd::STDERR, 2);
    }
}

// =============================================================================
// Module Validation Tests
// =============================================================================

mod validation_tests {
    use super::*;

    #[test]
    fn test_validate_empty_module() {
        let result = ModuleValidator::validate(&[]);
        assert!(matches!(result, Err(WasmError::InvalidModule { .. })));
    }

    #[test]
    fn test_validate_invalid_magic() {
        let bad_magic = vec![0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        let result = ModuleValidator::validate(&bad_magic);
        assert!(matches!(result, Err(WasmError::InvalidModule { .. })));
    }

    #[test]
    fn test_validate_minimal_module() {
        let module = minimal_wasm_module();
        let result = ModuleValidator::validate(&module);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_missing_memory_export() {
        // A module with main but no memory export should fail
        // (We'll use a truncated module for simplicity)
        let no_memory = vec![
            0x00, 0x61, 0x73, 0x6D, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, // type
            0x03, 0x02, 0x01, 0x00, // func
            0x07, 0x08, 0x01, 0x04, 0x6D, 0x61, 0x69, 0x6E, 0x00, 0x00, // only main export
            0x0A, 0x06, 0x01, 0x04, 0x00, 0x41, 0x00, 0x0B, // code
        ];
        let result = ModuleValidator::validate(&no_memory);
        assert!(matches!(result, Err(WasmError::MissingExport { name: "memory" })));
    }

    #[test]
    fn test_validate_missing_main_export() {
        // A module with memory but no main should fail
        let no_main = vec![
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
            0x05, 0x03, 0x01, 0x00, 0x01, // memory section
            0x07, 0x0A, 0x01, 0x06, 0x6D, 0x65, 0x6D, 0x6F, 0x72, 0x79, 0x02, 0x00,
        ];
        let result = ModuleValidator::validate(&no_main);
        assert!(matches!(result, Err(WasmError::MissingExport { name: "main" })));
    }
}

// =============================================================================
// Loader Tests
// =============================================================================

mod loader_tests {
    use super::*;

    #[test]
    fn test_loader_new() {
        let loader = Loader::new();
        assert!(!loader.has_module());
    }

    #[test]
    fn test_load_minimal_module() {
        let mut loader = Loader::new();
        let module = minimal_wasm_module();
        let result = loader.load(&module);
        assert!(result.is_ok());
        assert!(loader.has_module());
    }

    #[test]
    fn test_load_invalid_module() {
        let mut loader = Loader::new();
        let result = loader.load(&[0, 1, 2, 3]);
        assert!(result.is_err());
        assert!(!loader.has_module());
    }

    #[test]
    fn test_execute_minimal_module() {
        let mut loader = Loader::new();
        let module = minimal_wasm_module();
        loader.load(&module).unwrap();

        let result = loader.execute(&["test"]).unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_execute_with_args() {
        let mut loader = Loader::new();
        let module = minimal_wasm_module();
        loader.load(&module).unwrap();

        let result = loader.execute(&["cat", "file.txt"]).unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_execute_non_zero_exit() {
        let mut loader = Loader::new();
        let module = exit_code_module(42);
        loader.load(&module).unwrap();

        let result = loader.execute(&["test"]).unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    fn test_execute_without_load() {
        let loader = Loader::new();
        let result = loader.execute(&["test"]);
        assert!(result.is_err());
    }
}

// =============================================================================
// Runtime Tests (syscall handling)
// =============================================================================

mod runtime_tests {
    use super::*;

    #[test]
    fn test_runtime_new() {
        let runtime = Runtime::new();
        assert_eq!(runtime.exit_code(), None);
    }

    #[test]
    fn test_runtime_stdout_capture() {
        let mut runtime = Runtime::new();
        runtime.write_stdout(b"hello");
        assert_eq!(runtime.stdout(), b"hello");
    }

    #[test]
    fn test_runtime_stderr_capture() {
        let mut runtime = Runtime::new();
        runtime.write_stderr(b"error");
        assert_eq!(runtime.stderr(), b"error");
    }

    #[test]
    fn test_runtime_stdin() {
        let mut runtime = Runtime::with_stdin(b"input data".to_vec());
        let mut buf = [0u8; 5];
        let n = runtime.read_stdin(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(&buf, b"input");
    }

    #[test]
    fn test_runtime_stdin_eof() {
        let mut runtime = Runtime::with_stdin(b"hi".to_vec());
        let mut buf = [0u8; 10];
        let n = runtime.read_stdin(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], b"hi");

        // Second read should return 0 (EOF)
        let n = runtime.read_stdin(&mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_runtime_exit() {
        let mut runtime = Runtime::new();
        assert!(!runtime.has_exited());
        runtime.exit(42);
        assert!(runtime.has_exited());
        assert_eq!(runtime.exit_code(), Some(42));
    }

    #[test]
    fn test_runtime_env_vars() {
        let mut runtime = Runtime::new();
        runtime.set_env("HOME", "/home/user");
        assert_eq!(runtime.get_env("HOME"), Some("/home/user".to_string()));
        assert_eq!(runtime.get_env("NONEXISTENT"), None);
    }

    #[test]
    fn test_runtime_cwd() {
        let runtime = Runtime::with_cwd("/home/user");
        assert_eq!(runtime.cwd(), "/home/user");
    }
}

// =============================================================================
// File Descriptor Tests
// =============================================================================

mod fd_tests {
    use super::*;

    #[test]
    fn test_fd_table_new() {
        let table = FdTable::new();
        // Standard fds should be pre-allocated
        assert!(table.is_valid(fd::STDIN));
        assert!(table.is_valid(fd::STDOUT));
        assert!(table.is_valid(fd::STDERR));
    }

    #[test]
    fn test_fd_table_allocate() {
        let mut table = FdTable::new();
        let fd = table.allocate("/tmp/file.txt", OpenFlags::READ);
        assert!(fd.is_ok());
        let fd = fd.unwrap();
        assert!(fd >= 3); // After standard fds
        assert!(table.is_valid(fd));
    }

    #[test]
    fn test_fd_table_close() {
        let mut table = FdTable::new();
        let fd = table.allocate("/tmp/file.txt", OpenFlags::READ).unwrap();
        assert!(table.is_valid(fd));
        let result = table.close(fd);
        assert!(result.is_ok());
        assert!(!table.is_valid(fd));
    }

    #[test]
    fn test_fd_table_cannot_close_std() {
        let mut table = FdTable::new();
        // Should not be able to close stdin/stdout/stderr
        assert!(table.close(fd::STDIN).is_err());
        assert!(table.close(fd::STDOUT).is_err());
        assert!(table.close(fd::STDERR).is_err());
    }

    #[test]
    fn test_fd_table_max_fds() {
        let mut table = FdTable::new();
        // Should be able to allocate up to MAX_FDS
        for _ in 0..FdTable::MAX_FDS - 3 {
            // -3 for std fds
            let result = table.allocate("/tmp/file", OpenFlags::READ);
            assert!(result.is_ok());
        }
        // Next allocation should fail
        let result = table.allocate("/tmp/file", OpenFlags::READ);
        assert!(matches!(result, Err(WasmError::TooManyOpenFiles { .. })));
    }

    #[test]
    fn test_fd_table_get_path() {
        let mut table = FdTable::new();
        let fd = table.allocate("/tmp/file.txt", OpenFlags::READ).unwrap();
        assert_eq!(table.get_path(fd), Some("/tmp/file.txt".to_string()));
        assert_eq!(table.get_path(999), None);
    }
}

// =============================================================================
// Memory Access Tests
// =============================================================================

mod memory_tests {
    use super::*;

    #[test]
    fn test_memory_bounds_check() {
        let mem = WasmMemory::new(1); // 1 page = 64KB
        assert!(mem.check_bounds(0, 100));
        assert!(mem.check_bounds(65535, 1));
        assert!(!mem.check_bounds(65536, 1)); // Out of bounds
        assert!(!mem.check_bounds(65500, 100)); // Would overflow
    }

    #[test]
    fn test_memory_read_write() {
        let mut mem = WasmMemory::new(1);
        mem.write(0, b"hello").unwrap();
        let mut buf = [0u8; 5];
        mem.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_memory_read_string() {
        let mut mem = WasmMemory::new(1);
        mem.write(0, b"hello\0world").unwrap();
        let s = mem.read_cstring(0, 20).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_memory_read_out_of_bounds() {
        let mem = WasmMemory::new(1);
        let mut buf = [0u8; 100];
        let result = mem.read(65500, &mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_write_out_of_bounds() {
        let mut mem = WasmMemory::new(1);
        let data = [0u8; 100];
        let result = mem.write(65500, &data);
        assert!(result.is_err());
    }
}

// =============================================================================
// Argument Passing Tests
// =============================================================================

mod args_tests {
    use super::*;

    #[test]
    fn test_args_layout_single() {
        let layout = ArgLayout::new(&["test"]);
        assert_eq!(layout.string_offsets.len(), 1);
        assert_eq!(layout.strings_size, 5); // "test\0"
        assert_eq!(layout.argv_size, 8); // 2 pointers (1 arg + null)
    }

    #[test]
    fn test_args_layout_multiple() {
        let layout = ArgLayout::new(&["cat", "-n", "file.txt"]);
        assert_eq!(layout.string_offsets.len(), 3);
        // "cat\0" = 4, "-n\0" = 3, "file.txt\0" = 9
        assert_eq!(layout.strings_size, 16);
        // 4 pointers (3 args + null) = 16
        assert_eq!(layout.argv_size, 16);
    }

    #[test]
    fn test_args_write_and_read() {
        let args = &["echo", "hello", "world"];
        let layout = ArgLayout::new(args);
        let base_addr = 1024u32;

        let mut buf = vec![0u8; layout.total_size()];
        let argv_ptr = layout.write_to(args, base_addr, &mut buf);

        // Verify strings
        assert_eq!(&buf[0..4], b"echo");
        assert_eq!(buf[4], 0);
        assert_eq!(&buf[5..10], b"hello");
        assert_eq!(buf[10], 0);
        assert_eq!(&buf[11..16], b"world");
        assert_eq!(buf[16], 0);

        // Verify argv pointer
        assert_eq!(argv_ptr, base_addr + layout.strings_size as u32);
    }
}

// =============================================================================
// Integration Tests (simulated, since we can't run real WASM in tests)
// =============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_lifecycle() {
        // Create a loader
        let mut loader = Loader::new();

        // Load a minimal module
        let module = minimal_wasm_module();
        assert!(loader.load(&module).is_ok());

        // Execute with arguments
        let result = loader.execute(&["test", "arg1", "arg2"]).unwrap();

        // Check result
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_multiple_executions() {
        let mut loader = Loader::new();
        loader.load(&minimal_wasm_module()).unwrap();

        // Execute multiple times (should work - fresh state each time)
        let r1 = loader.execute(&["test1"]).unwrap();
        let r2 = loader.execute(&["test2"]).unwrap();

        assert_eq!(r1.exit_code, 0);
        assert_eq!(r2.exit_code, 0);
    }
}
