//! Links `vilocity.exe` without high-entropy ASLR: the title's memory then lies
//! under 2^40, which a retail title's BVH builder (`XboxBvhBuilder`)
//! requires of its GPU addresses. With Rust's default (high-entropy VA on),
//! Windows puts a process's bottom-up allocations around 1 to 2.5 TiB.

fn main() {
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg-bin=vilocity=/HIGHENTROPYVA:NO"),
        Ok("gnu") => println!("cargo:rustc-link-arg-bin=vilocity=-Wl,--disable-high-entropy-va"),
        _ => {}
    }
}
