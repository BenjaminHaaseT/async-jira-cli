
pub mod prelude {
    pub use super::*;
}

/// A uninhabited enum, for the purpose of shutdown channels.
/// Essentially a dummy type.
pub enum Void {}

/// A function that will parse 4 bytes. Useful when reading from a stream bytes
/// that has repeated patterns of 4 bytes that need to be parsed.
pub fn parse_4_bytes(bytes: &[u8]) -> u32 {
    let mut res = 0;
    for i in 0..4 {
        res ^= (bytes[i] as u32) << (i * 8);
    }
    res
}