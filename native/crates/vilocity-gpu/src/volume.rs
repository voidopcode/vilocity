//! (Vilocity) The game's sound volume (the in-game menu's, `live::Knob::Volume`): Windows'
//! volume of this process's sound, as the mixer shows it, so the title's own audio (its
//! XAudio2, passed through to Windows) is left as it is.

use windows::Win32::Media::Audio::{IAudioSessionManager, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};

/// Sets it to `share` (0 to 1), on a thread of its own (COM, and never in a frame's way);
/// again a little later, in case the title's sound had not started yet.
pub(crate) fn set(share: f32) {
    let share = share.clamp(0.0, 1.0);
    let _ = std::thread::Builder::new().name("vilocity-volume".into()).spawn(move || {
        for wait in [0, 20] {
            std::thread::sleep(std::time::Duration::from_secs(wait));
            // SAFETY: COM on this thread, for these calls only.
            let set = unsafe {
                let init = CoInitializeEx(None, COINIT_MULTITHREADED);
                let set = apply(share);
                if init.is_ok() {
                    CoUninitialize();
                }
                set
            };
            match set {
                Ok(()) if wait == 0 => tracing::info!(share, "sound volume set"),
                Ok(()) => {}
                Err(e) => tracing::warn!(error = %e, "the sound volume could not be set"),
            }
        }
    });
}

/// SAFETY: COM is initialised on the calling thread.
unsafe fn apply(share: f32) -> windows::core::Result<()> {
    // SAFETY: plain COM calls; the default output's session of this process.
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let sessions: IAudioSessionManager = device.Activate(CLSCTX_ALL, None)?;
        let volume: ISimpleAudioVolume = sessions.GetSimpleAudioVolume(None, 0)?;
        volume.SetMasterVolume(share, std::ptr::null())
    }
}
