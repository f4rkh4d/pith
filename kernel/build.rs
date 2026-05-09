// kernel build.rs. compiles user/hello, runs llvm-objcopy on the elf to
// produce a flat binary, exposes the binary path to main.rs as the env
// var USER_HELLO_BIN. the rest of the kernel `include_bytes!`s it.
//
// requires the `llvm-tools-preview` rustup component because we use
// rust-objcopy (which ships inside rustc). the build aborts loudly if
// the component isn't installed.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir  = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // build every user crate listed below; expose the flat .bin path
    // back to the kernel as USER_<NAME>_BIN.
    for name in ["hello", "echo"] {
        let bin = build_user_bin(&manifest, &out_dir, name);
        println!(
            "cargo:rustc-env=USER_{}_BIN={}",
            name.to_uppercase(),
            bin.display()
        );
    }
}

fn build_user_bin(manifest: &PathBuf, out_dir: &PathBuf, name: &str) -> PathBuf {
    println!("cargo:rerun-if-changed=../user/{}/src/main.rs", name);
    println!("cargo:rerun-if-changed=../user/{}/Cargo.toml", name);
    println!("cargo:rerun-if-changed=../user/{}/user.ld", name);

    let user_dir   = manifest.join("..").join("user").join(name);
    let target_dir = out_dir.join(format!("user-{}", name));

    // RUSTFLAGS env var REPLACES cargo config rustflags entirely, so we
    // pin the user crate's link flags here and isolate from the parent
    // kernel build's rustflags.
    let user_rustflags = [
        "-C", "link-arg=-Tuser.ld",
        "-C", "link-arg=-no-pie",
        "-C", "code-model=medium",
        "-C", "relocation-model=static",
    ].join("\x1f");

    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap());
    cmd.args([
        "build",
        "--release",
        "--target",
        "riscv64gc-unknown-none-elf",
        "--target-dir",
    ])
    .arg(&target_dir)
    .current_dir(&user_dir)
    .env("CARGO_ENCODED_RUSTFLAGS", user_rustflags)
    .env_remove("RUSTFLAGS")
    .env_remove("CARGO_BUILD_RUSTFLAGS")
    .env_remove("CARGO_TARGET_DIR")
    .env_remove("CARGO_BUILD_TARGET");
    for (k, _) in std::env::vars() {
        if k.starts_with("CARGO_TARGET_") || k.starts_with("CARGO_PROFILE_") {
            cmd.env_remove(&k);
        }
    }
    let status = cmd.status().unwrap_or_else(|e| panic!("cargo for user/{}: {}", name, e));
    assert!(status.success(), "user/{} build failed", name);

    let elf = target_dir
        .join("riscv64gc-unknown-none-elf")
        .join("release")
        .join(name);
    assert!(elf.exists(), "user/{} elf missing at {}", name, elf.display());

    let bin = target_dir.join(format!("{}.bin", name));
    let objcopy = locate_objcopy();
    let status = Command::new(&objcopy)
        .args(["-O", "binary"])
        .arg(&elf)
        .arg(&bin)
        .status()
        .unwrap_or_else(|e| panic!("objcopy ({}): {}", objcopy.display(), e));
    assert!(status.success(), "objcopy on user/{} failed", name);

    bin
}

fn locate_objcopy() -> PathBuf {
    // prefer the llvm-objcopy that ships with rustup's llvm-tools-preview.
    // we ask rustc for its sysroot, then scan rustlib/*/bin/ which is
    // host-arch-agnostic. fall back to a $PATH binary if missing.
    if let Ok(out) = Command::new("rustc").args(["--print", "sysroot"]).output() {
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let rustlib = PathBuf::from(&root).join("lib").join("rustlib");
        if let Ok(entries) = std::fs::read_dir(&rustlib) {
            for entry in entries.flatten() {
                let p = entry.path().join("bin").join("llvm-objcopy");
                if p.exists() {
                    return p;
                }
            }
        }
    }
    // homebrew fallback (macos).
    for hb in ["/opt/homebrew/opt/llvm/bin/llvm-objcopy", "/usr/local/opt/llvm/bin/llvm-objcopy"] {
        if PathBuf::from(hb).exists() {
            return PathBuf::from(hb);
        }
    }
    PathBuf::from("llvm-objcopy")
}
