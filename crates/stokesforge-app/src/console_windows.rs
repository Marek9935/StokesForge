//! Windows, release : l'exécutable est du sous-système « fenêtré », donc un
//! double-clic n'ouvre plus de console noire (#332). Revers : depuis un terminal,
//! `stderr` et `stdout` n'arriveraient plus nulle part. [`attach_parent`] rend
//! les messages (erreur de démarrage, sortie d'un script) au terminal qui a
//! lancé l'application ; sans terminal parent, il ne fait rien.

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

/// Rattache les sorties standard au terminal parent, si l'application en a un.
///
/// Une sortie déjà redirigée (fichier, tube) est laissée telle quelle : seules
/// les sorties absentes sont rouvertes sur `CONOUT$`.
pub fn attach_parent() {
    // SAFETY : appels Win32 sans pointeur retenu ; `conout` vit pendant l'appel
    // de `CreateFileW`, terminé par son zéro final.
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
