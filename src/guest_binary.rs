/// Embedded guest binary
/// This binary is compiled during the build process and embedded directly into the host
pub const GUEST_BINARY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/guest/target/release/libagent_guest.so"
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_binary_is_not_empty() {
        assert!(!GUEST_BINARY.is_empty(), "Guest binary should be embedded");
    }
}
