//! `vilocity.exe`: one executable for both engines. The native engine runs
//! Grand Theft Auto VI, the compat engine other Xbox Series GDK titles. Only
//! one engine runs in a process: both re-launch this executable for their own
//! helper processes, which `main` routes back to the right engine first.

use std::ffi::{OsString, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

#[cfg(windows)]
mod mouseclip;
#[cfg(windows)]
mod sandbox;

const HELP: &str = "\
Vilocity – Xbox Series X|S titles on Windows

Usage:
  vilocity play <game folder | file.exe> [--engine native|compat] [--no-sandbox] [-- game arguments]
      Runs the game with the engine made for it: native for Grand Theft Auto VI,
      compat for everything else. --engine picks one yourself. The native engine
      runs the game in a sandbox (no network, none of your files);
      --no-sandbox runs it as you.
  vilocity warm [--used]
      Builds the graphics driver's shader pipelines ahead of play, in the game's
      sandbox (the driver keeps a sandbox's pipelines apart, where the game
      looks for them). --used: those earlier runs drew with (minutes); without
      it every pipeline the game has ever made (long, and gigabytes of disk).
  vilocity native <command> ...   the native engine's commands (vilocity native --help)
  vilocity compat <command> ...   the compat engine's commands (vilocity compat --help)
  vilocity --version
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Native,
    Compat,
}

/// The compat engine's TLS callback (it must live in the executable the game
/// runs in): in compat host mode it sets up the game's static TLS for each
/// new thread. The native engine's own callback lives in `vilocity-loader`
/// and does nothing until that engine has loaded a title.
unsafe extern "system" fn compat_tls_callback(_h: *mut c_void, reason: u32, _r: *mut c_void) {
    if vilocity_compat::host::HOST_MODE.load(Ordering::Acquire) {
        vilocity_compat::loader::on_tls_event(reason);
    }
}

#[used]
#[unsafe(link_section = ".CRT$XLY")]
static COMPAT_TLS_CALLBACK: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) =
    compat_tls_callback;

fn main() {
    // SAFETY: a plain read of our own static, so the linker keeps it.
    unsafe { std::ptr::read_volatile(&COMPAT_TLS_CALLBACK) };

    // The compat engine's game process: `compat run` started this executable again.
    if let Some(boot) = std::env::var_os("VILOCITY_HOST") {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var("VILOCITY_HOST") };
        std::process::exit(vilocity_compat::host::host_main(&PathBuf::from(boot)));
    }

    let args: Vec<OsString> = std::env::args_os().collect();
    let code = match args.get(1).and_then(|a| a.to_str()) {
        Some("native") => native(args[2..].to_vec()),
        Some("compat") => vilocity_compat::cli::main(strings(&args[2..])),
        Some("play") if args.len() >= 3 => play(&args[2..]),
        Some("warm") => warm(&args[2..]),
        Some("--version" | "-V") => {
            println!("vilocity {}", env!("CARGO_PKG_VERSION"));
            0
        }
        // The native engine's audio recorder (diagnostics builds): started with no arguments.
        None if std::env::var_os("VILOCITY_AUDIO_CAPTURE").is_some() => native(Vec::new()),
        _ => {
            print!("{HELP}");
            let asked = matches!(args.get(1).and_then(|a| a.to_str()), None | Some("--help" | "-h"));
            if asked { 0 } else { 2 }
        }
    };
    std::process::exit(code);
}

fn native(rest: Vec<OsString>) -> i32 {
    let argv = std::iter::once(OsString::from("vilocity native")).chain(rest);
    match vilocity_native::main(argv) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {e:?}");
            1
        }
    }
}

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
}

/// `warm [options]`: the native engine's `warm` in the sandbox `play` runs the
/// game in. What a sandboxed process has the driver compile is kept in that
/// sandbox's own cache, the one the sandboxed game reads; `vilocity native
/// warm` fills the other one (for `play --no-sandbox`).
fn warm(options: &[OsString]) -> i32 {
    let mut run: Vec<OsString> = ["native", "warm"].map(OsString::from).to_vec();
    run.extend(options.iter().cloned());
    #[cfg(windows)]
    {
        let ran = std::env::current_exe().map_err(|e| e.to_string()).and_then(|me| in_sandbox(&me, &run, None));
        ran.unwrap_or_else(|e| {
            eprintln!("vilocity: {e}");
            1
        })
    }
    #[cfg(not(windows))]
    native(run[1..].to_vec())
}

/// `play <target> [--engine E] [-- game args]`.
fn play(args: &[OsString]) -> i32 {
    let target = PathBuf::from(&args[0]);
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let (options, title_args) = (&args[1..split], args.get(split + 1..).unwrap_or(&[]));
    let sandboxed = !options.iter().any(|o| o == "--no-sandbox");
    let options: Vec<&OsString> = options.iter().filter(|o| *o != "--no-sandbox").collect();
    let forced = match options[..] {
        [] => None,
        [flag, e] if flag == "--engine" && e == "native" => Some(Engine::Native),
        [flag, e] if flag == "--engine" && e == "compat" => Some(Engine::Compat),
        _ => {
            eprintln!("vilocity play: unknown options {options:?}\n\n{HELP}");
            return 2;
        }
    };
    // The compat engine's lookup: a folder becomes the exe its MicrosoftGame.config names.
    let exe = match vilocity_compat::launcher::resolve_target(&target) {
        Ok((_, exe, _)) => exe,
        Err(e) => {
            eprintln!("vilocity: {e}");
            return 2;
        }
    };
    match forced.unwrap_or_else(|| engine_for(&exe)) {
        Engine::Compat => vilocity_compat::launcher::run(&target, &strings(title_args)),
        // A process of its own, started as `vilocity native run ...`: when the
        // title relaunches itself, the native engine rebuilds that command line from ours.
        Engine::Native => {
            let me = match std::env::current_exe() {
                Ok(me) => me,
                Err(e) => {
                    eprintln!("vilocity: {e}");
                    return 1;
                }
            };
            println!("vilocity native run \"{}\" --execute", exe.display());
            let mut run: Vec<OsString> = ["native", "run"].map(OsString::from).to_vec();
            run.extend([exe.clone().into_os_string(), "--execute".into()]);
            // `VILOCITY_CONSOLE=series-s`: the console model the title sees.
            if let Some(console) = std::env::var_os("VILOCITY_CONSOLE") {
                run.extend(["--console".into(), console]);
            }
            run.push("--".into());
            run.extend(title_args.iter().cloned());
            #[cfg(windows)]
            if sandboxed {
                return match in_sandbox(&me, &run, exe.parent()) {
                    Ok(code) => code,
                    Err(e) => {
                        eprintln!("vilocity: {e}\n(--no-sandbox runs the game without the sandbox)");
                        1
                    }
                };
            }
            #[cfg(not(windows))]
            let _ = sandboxed;
            let status = std::process::Command::new(me).args(&run).status();
            match status {
                Ok(s) => s.code().unwrap_or(1),
                Err(e) => {
                    eprintln!("vilocity: starting the native engine: {e}");
                    1
                }
            }
        }
    }
}

/// The native engine's `run` in the sandbox: it reads the game's folder (and
/// the folder of `VILOCITY_FSR_DLL`, if set) and keeps its cache and data in the
/// configured folders, the only ones it may write. `game`: `None` for a command
/// that reads no game (`warm`).
#[cfg(windows)]
fn in_sandbox(me: &Path, run: &[OsString], game: Option<&Path>) -> Result<i32, String> {
    let cfg = vilocity_core::Config::load().map_err(|e| format!("{e:#}"))?;
    let tmp = cfg.cache_dir.join("tmp");
    for dir in [&cfg.cache_dir, &cfg.data_dir, &tmp] {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let mut grants = vec![
        sandbox::Grant { path: cfg.cache_dir.clone(), write: true },
        sandbox::Grant { path: cfg.data_dir.clone(), write: true },
    ];
    grants.extend(game.map(|game| sandbox::Grant { path: game.to_path_buf(), write: false }));
    let env = vec![
        ("VILOCITY_CACHE_DIR", cfg.cache_dir.clone().into_os_string()),
        ("VILOCITY_DATA_DIR", cfg.data_dir.clone().into_os_string()),
        ("TEMP", tmp.clone().into_os_string()),
        ("TMP", tmp.into_os_string()),
        // For `warm`: which of the driver's two caches it fills.
        ("VILOCITY_SANDBOXED", "1".into()),
    ];
    // AMD's PC upscaler, if the launcher names one: its folder is read.
    if let Some(dll) = std::env::var_os("VILOCITY_FSR_DLL").map(PathBuf::from)
        && let Some(dir) = dll.parent()
    {
        grants.push(sandbox::Grant { path: dir.to_path_buf(), write: false });
    }
    println!(
        "sandbox: no network; reads {}; writes only {} and {}",
        game.map_or("no game".into(), |game| game.display().to_string()),
        cfg.cache_dir.display(),
        cfg.data_dir.display()
    );
    sandbox::run(me, run, &grants, &env, &cfg.data_dir)
}

/// shortcut: recognises GTA VI by its executable's name only; switch to
/// `vilocity_game::identify` once that lists the builds it supports.
fn engine_for(exe: &Path) -> Engine {
    let name = exe.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
    if name.as_deref() == Some("game_scarlett_master.exe") {
        Engine::Native
    } else {
        Engine::Compat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gta_vi_goes_to_native_everything_else_to_compat() {
        assert_eq!(engine_for(Path::new(r"D:\GTA6\game_scarlett_master.exe")), Engine::Native);
        assert_eq!(engine_for(Path::new(r"D:\GTA6\GAME_SCARLETT_MASTER.EXE")), Engine::Native);
        assert_eq!(engine_for(Path::new(r"D:\Minecraft\Minecraft.Windows.exe")), Engine::Compat);
        assert_eq!(engine_for(Path::new(r"D:\Other\game_scarlett.exe")), Engine::Compat);
    }
}
