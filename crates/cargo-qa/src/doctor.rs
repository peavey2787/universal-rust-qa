use std::{path::Path, process::Command};
pub fn run(_: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Universal Rust QA toolchain doctor\n");
    for (label, program, args) in [
        ("cargo", "cargo", vec!["--version"]),
        ("rustc", "rustc", vec!["--version"]),
        ("rustfmt", "rustfmt", vec!["--version"]),
        ("clippy", "cargo", vec!["clippy", "--version"]),
        ("cargo-llvm-cov", "cargo", vec!["llvm-cov", "--version"]),
        ("cargo-mutants", "cargo", vec!["mutants", "--version"]),
        ("cargo-fuzz", "cargo", vec!["fuzz", "--version"]),
        ("cargo-hack", "cargo", vec!["hack", "--version"]),
        ("cargo-deny", "cargo", vec!["deny", "--version"]),
        ("cargo-machete", "cargo-machete", vec!["--version"]),
        ("semver-checks", "cargo", vec!["semver-checks", "--version"]),
        ("cargo-bloat", "cargo", vec!["bloat", "--version"]),
        ("cargo-llvm-lines", "cargo", vec!["llvm-lines", "--version"]),
        ("cargo-asm", "cargo", vec!["asm", "--version"]),
        ("cargo-insta", "cargo", vec!["insta", "--version"]),
    ] {
        let status = Command::new(program).args(&args).output().is_ok_and(|o| o.status.success());
        println!("{label:<20} {}", if status { "available" } else { "missing / unavailable" });
    }
    for (label, program) in [("readelf", "readelf"), ("dumpbin", "dumpbin"), ("otool", "otool")] {
        let status =
            Command::new(program).arg("--version").output().is_ok_and(|o| o.status.success());
        println!("{label:<20} {}", if status { "available" } else { "N/A / unavailable" });
    }
    println!(
        "\nNightly-only MIR/sanitizer backends are validated when executed and report UNKNOWN/N/A rather than silently passing unsupported targets."
    );
    Ok(())
}
