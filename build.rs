use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=guest/src");
    println!("cargo:rerun-if-changed=guest/Cargo.toml");

    // Build the guest binary
    let status = Command::new("cargo")
        .args(&["build", "--release"])
        .current_dir("guest")
        .status()
        .expect("Failed to build guest");

    if !status.success() {
        panic!("Guest build failed");
    }

    // Tell cargo where to find the guest binary for include_bytes!
    println!("cargo:rustc-env=GUEST_BINARY_PATH=guest/target/release/libagent_guest.so");
}
