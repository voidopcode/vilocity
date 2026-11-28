//! (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
//!
//! The title's FidelityFX upscaler path for the supported retail image (GTA
//! VI 1.0.2.1), on every console model and at the scale DLSS is asked for:
//! with DLSS available (`VILOCITY_DLSS=1` and Streamline loaded,
//! `vilocity_hle::streamline`), the image's renderer selection and the
//! FidelityFX path's render size are patched before it is mapped, so that
//! the title upscales through `ffxDispatch`, which DLSS takes over. Without
//! DLSS nothing here runs and the image is the stock one.
//!
//! From foxhound's `apps/victory/src/native_1440p.rs`, its `prepare_upscaler`
//! alone (there from DexNom's fork: DexNom/VIctory,
//! `apps/victory/src/native_1440p.rs`, `prepare_fsr_4k`, GPL-3.0-or-later).
//! The rest of that file, a 2560x1440 display mode patched into the image,
//! is not here: this tree tells the title its display's size
//! (`VILOCITY_DISPLAY_MODE`). Every field is checked against the stock bytes
//! first: another build is refused, not patched.

use anyhow::{Context, Result, ensure};
use vilocity_hle::streamline::Mode;
use vilocity_loader::PreparedImage;

const RETAIL_BUILD: &str = "b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93";
/// The scale of the title's own temporal pass (stock: 2/3).
const TEMPORAL_SCALE: u32 = 0x028f_1e7e;

/// The title's renderer selection: `mov eax, 6; cmovne eax, ecx` (the
/// title's own temporal pass, or on Series S with the upscaler there the
/// FidelityFX path, 7).
const RENDERER_SELECTION: u32 = 0x007e_9421;
const STOCK_RENDERER_SELECTION: [u8; 8] = [0xb8, 6, 0, 0, 0, 0x0f, 0x45, 0xc1];
/// The FidelityFX path's render size: the console check's result (Series S)
/// is noted as the FidelityFX quality mode and picks the scaled branch
/// (`movzx eax, al; mov [r14+0x90], eax; test al, al; jne`), whose scale
/// multiplies the output size.
const UPSCALER_SIZING: u32 = 0x028e_f69a;
const STOCK_UPSCALER_SIZING: [u8; 14] = [0x0f, 0xb6, 0xc0, 0x41, 0x89, 0x86, 0x90, 0, 0, 0, 0x84, 0xc0, 0x75, 0x0a];
/// The native branch's load of 1.0, and the scaled branch's of the scale.
const NATIVE_LOAD: u32 = 0x028e_f6a8;
const STOCK_NATIVE_LOAD: [u8; 10] = [0xc5, 0xfa, 0x10, 0x05, 0x28, 0x78, 0x6d, 0x03, 0xeb, 0x08];
const SCALED_LOAD: u32 = 0x028e_f6b2;
const STOCK_SCALED_LOAD: [u8; 8] = [0xc5, 0xfa, 0x10, 0x05, 0xa2, 0xe2, 0x86, 0x03];
/// The scale the scaled branch loads (shared with other code: not written).
const UPSCALER_SCALE: u32 = 0x0615_d95c;

/// Makes the title render through its FidelityFX upscaler path (renderer 7,
/// as Series S does) with the scene at `mode`'s scale of the output, so that
/// DLSS can take its upscale dispatches over (`vilocity_hle::modules::fidelityfx::dlss`).
/// On Series X the stock title uses its own temporal pass instead and never
/// loads its upscaler. The FidelityFX quality mode the title notes is
/// `mode`'s; the temporal scene scale (the title's own pass) is set to the
/// same scale, as the stock Series X image pairs them (2/3 each). Every
/// patched instruction is checked against the stock bytes first; nothing is
/// written if one differs.
pub(crate) fn prepare_upscaler(image: &mut PreparedImage, mode: Mode) -> Result<()> {
    ensure!(
        image.original_hash.to_hex().as_str() == RETAIL_BUILD,
        "the upscaler path is only known for the retail build {RETAIL_BUILD}"
    );
    ensure!(*field::<8>(image, RENDERER_SELECTION)? == STOCK_RENDERER_SELECTION, "unexpected renderer selection");
    ensure!(*field::<14>(image, UPSCALER_SIZING)? == STOCK_UPSCALER_SIZING, "unexpected upscaler render size selection");
    ensure!(*field::<10>(image, NATIVE_LOAD)? == STOCK_NATIVE_LOAD, "unexpected upscaler native scale load");
    ensure!(*field::<8>(image, SCALED_LOAD)? == STOCK_SCALED_LOAD, "unexpected upscaler scaled load");
    ensure!(*field::<4>(image, UPSCALER_SCALE)? == (2.0f32 / 3.0).to_le_bytes(), "unexpected upscaler scale");
    ensure!(
        *field::<4>(image, TEMPORAL_SCALE)? == (2.0f32 / 3.0).to_le_bytes(),
        "unexpected temporal scene scale (stock 2/3)"
    );
    // Renderer 7 every time it is asked (the console check has run).
    *field::<8>(image, RENDERER_SELECTION)? = [0xb8, 7, 0, 0, 0, 0x90, 0x90, 0x90];
    // `mov eax, mode; mov [r14+0x90], eax; jmp scaled`.
    // (Vilocity) DLAA (the whole size) is noted as the quality mode: noted as native AA (0),
    // the title stopped at a breakpoint 12 s in (2026-11-28).
    let noted = if mode == Mode::Dlaa { Mode::Quality.ffx_mode() } else { mode.ffx_mode() };
    *field::<14>(image, UPSCALER_SIZING)? = [0xb8, noted, 0, 0, 0, 0x41, 0x89, 0x86, 0x90, 0, 0, 0, 0xeb, 0x0a];
    if mode != Mode::Quality {
        // The jump leaves the native branch unreached: its load's operand
        // becomes a literal of ours, which the scaled branch loads.
        let literal = NATIVE_LOAD + 4;
        *field::<4>(image, literal)? = mode.scale().to_le_bytes();
        let displacement = literal as i32 - (SCALED_LOAD as i32 + 8);
        *field::<4>(image, SCALED_LOAD + 4)? = displacement.to_le_bytes();
    }
    *field::<4>(image, TEMPORAL_SCALE)? = mode.scale().to_le_bytes();
    Ok(())
}

fn field<const N: usize>(image: &mut PreparedImage, rva: u32) -> Result<&mut [u8; N]> {
    let offset = image.info.rva_to_file_offset(rva).context("upscaler path field missing")? as usize;
    image
        .bytes
        .get_mut(offset..offset + N)
        .context("incomplete upscaler path field")?
        .try_into()
        .context("invalid upscaler path field length")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the retail executable named by `VILOCITY_UPSCALER_PATH_TEST_IMAGE`
    /// (not part of the repository): the upscaler path at each scale; the
    /// scaled branch then loads the mode's scale and the noted mode is its
    /// FidelityFX mode. Without it, nothing is checked.
    #[test]
    fn patches_the_upscaler_path() {
        let Some(path) = std::env::var_os("VILOCITY_UPSCALER_PATH_TEST_IMAGE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        for mode in [Mode::Dlaa, Mode::Quality, Mode::Balanced, Mode::Performance, Mode::UltraPerformance] {
            let mut image = vilocity_loader::prepare(&bytes).unwrap();
            prepare_upscaler(&mut image, mode).unwrap();
            assert_eq!(*field::<8>(&mut image, RENDERER_SELECTION).unwrap(), [0xb8, 7, 0, 0, 0, 0x90, 0x90, 0x90]);
            let sizing = *field::<14>(&mut image, UPSCALER_SIZING).unwrap();
            assert_eq!(sizing[1], if mode == Mode::Dlaa { Mode::Quality.ffx_mode() } else { mode.ffx_mode() });
            assert_eq!(sizing[12..], [0xeb, 0x0a]);
            // The jump lands on the scaled load.
            assert_eq!(UPSCALER_SIZING + 14 + 0x0a, SCALED_LOAD);
            let disp = i32::from_le_bytes(*field::<4>(&mut image, SCALED_LOAD + 4).unwrap());
            let loaded = (SCALED_LOAD + 8).wrapping_add_signed(disp);
            assert_eq!(f32::from_le_bytes(*field::<4>(&mut image, loaded).unwrap()), mode.scale());
            assert_eq!(f32::from_le_bytes(*field::<4>(&mut image, TEMPORAL_SCALE).unwrap()), mode.scale());
            // The shared constant stays as it was.
            assert_eq!(*field::<4>(&mut image, UPSCALER_SCALE).unwrap(), (2.0f32 / 3.0).to_le_bytes());
            // Patched once, refused.
            assert!(prepare_upscaler(&mut image, mode).is_err());
        }
    }

    /// An image that is not the retail build (this test program) is refused
    /// with nothing written.
    #[test]
    fn another_build_is_refused_untouched() {
        let bytes = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let mut image = vilocity_loader::prepare(&bytes).unwrap();
        let before = image.bytes.clone();
        assert!(prepare_upscaler(&mut image, Mode::Quality).is_err());
        assert!(image.bytes == before);
    }
}
