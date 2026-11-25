//! `vilocity-native.exe`: the command line in `lib.rs`.

fn main() -> anyhow::Result<()> {
    vilocity_native::main(std::env::args_os())
}
