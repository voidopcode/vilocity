//! vilocity-compat.exe – command line front-end and (in host mode) the process the game runs in.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

/// Our own TLS callback: ntdll calls it for every thread start/exit in the process.
/// In host mode it initialises the game's static TLS for the new thread and forwards
/// the notification to the game's TLS callbacks / DllMains.
unsafe extern "system" fn vilocity_tls_callback(_h: *mut c_void, reason: u32, _r: *mut c_void) {
    if vilocity_compat::host::HOST_MODE.load(Ordering::Acquire) {
        vilocity_compat::loader::on_tls_event(reason);
    }
}

#[used]
#[link_section = ".CRT$XLY"]
static VILOCITY_TLS_CALLBACK: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) = vilocity_tls_callback;

fn main() {
    // keep the TLS callback from being discarded by the linker
    unsafe { std::ptr::read_volatile(&VILOCITY_TLS_CALLBACK) };

    if let Some(boot) = std::env::var_os("VILOCITY_HOST") {
        std::env::remove_var("VILOCITY_HOST");
        std::process::exit(vilocity_compat::host::host_main(&PathBuf::from(boot)));
    }

    std::process::exit(vilocity_compat::cli::main(std::env::args().skip(1).collect()));
}
