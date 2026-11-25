fn main() {
    println!("cargo:rustc-link-arg-cdylib=/DELAYLOAD:vilocity-probe-delayed.dll");
    println!("cargo:rustc-link-lib=delayimp");
}
