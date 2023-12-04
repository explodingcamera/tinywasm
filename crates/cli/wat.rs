use wast::{
    parser::{self, ParseBuffer},
    Wat,
};

pub fn wat2wasm(wat: &str) -> Vec<u8> {
    let buf = ParseBuffer::new(wat).expect("failed to create parse buffer");
    let mut module = parser::parse::<Wat>(&buf).expect("failed to parse wat");
    module.encode().expect("failed to encode wat")
}
