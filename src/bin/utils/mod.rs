
pub mod prelude {
    pub use super::*;
}

pub fn parse_4_bytes(bytes: &[u8]) -> u32 {
    let mut res = 0;
    for i in 0..4 {
        res ^= (bytes[i] as u32) << (i * 8);
    }
    res
}