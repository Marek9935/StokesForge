//! Windows, release: the executable uses the "windowed" subsystem, so a
//! double-click no longer opens a black console (#332). Downside: from a
//! terminal, `stderr` and `stdout` would go nowhere. [`attach_parent`] gives
//! the messages (startup error, script output) back to the terminal that
//! launched the application; without a parent terminal, it does nothing.

use std::ffi::c_void;

type Handle = *mut c_void;

const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
// STD_OUTPUT_HANDLE = (DWORD)-11, STD_ERROR_HANDLE = (DWORD)-12.
const STD_OUTPUT_HANDLE: u32 = 0u32.wrapping_sub(11);
const STD_ERROR_HANDLE: u32 = 0u32.wrapping_sub(12);
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
const OPEN_EXISTING: u32 = 3;
const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn GetStdHandle(std_handle: u32) -> Handle;
    fn SetStdHandle(std_handle: u32, handle: Handle) -> i32;
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: Handle,
    ) -> Handle;
}

/// Attaches the standard outputs to the parent terminal, if the application
/// has one.
///
/// An output that is already redirected (file, pipe) is left as is: only the
/// missing outputs are reopened on `CONOUT$`.
pub fn attach_parent() {
    // SAFETY: Win32 calls that keep no pointer; `conout` lives during the
    // `CreateFileW` call, ended by its final zero.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        for std_handle in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let current = GetStdHandle(std_handle);
            if !current.is_null() && current != INVALID_HANDLE_VALUE {
                continue;
            }
            let console = CreateFileW(
                conout.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if console != INVALID_HANDLE_VALUE {
                SetStdHandle(std_handle, console);
            }
        }
    }
}
