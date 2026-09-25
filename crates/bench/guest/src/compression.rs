#![no_main]

const INPUT: &[u8] = include_bytes!("input.txt");
const COMPRESSED: &[u8] = include_bytes!("input.deflate");

#[unsafe(no_mangle)]
pub extern "C" fn compress() -> i32 {
    let compressed = miniz_oxide::deflate::compress_to_vec(INPUT, 6);
    compressed.iter().fold(0u32, |sum, byte| sum.wrapping_add(*byte as u32)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn decompress() -> i32 {
    let decoded = miniz_oxide::inflate::decompress_to_vec(COMPRESSED).unwrap();
    decoded.iter().fold(0u32, |sum, byte| sum.wrapping_add(*byte as u32)) as i32
}
