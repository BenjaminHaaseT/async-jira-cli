
pub mod prelude {
    pub use super::*;
}

/// Provides an interface for any type that can be converted to a `Vec<u8>`. Useful for converting
/// a type into a representation that can be transmitted over Tcp.
pub trait AsBytes {
    /// Required method. Takes `self` by reference and returns the a vector of bytes
    /// that represents `self`.
    fn as_bytes(&self) -> Vec<u8>;
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