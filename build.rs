//! Hands the release target triple to `boxpigma update`, which must name the asset it
//! downloads (`boxpigma-<target>.zip` / `.tar.gz`) exactly as the release does.
//!
//! `TARGET` is what cargo itself resolved — unlike `std::env::consts`, it stays correct for
//! cross builds (Linux aarch64 goes through `cross`) and for musl, neither of which is
//! distinguishable from the host at run time.
fn main() {
    println!(
        "cargo:rustc-env=BOXPIGMA_TARGET={}",
        std::env::var("TARGET").expect("cargo always sets TARGET for build scripts")
    );
}
