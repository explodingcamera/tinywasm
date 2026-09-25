#![no_main]

const INPUT: &str = include_str!("input.json");

#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    let document: serde_json::Value = serde_json::from_str(INPUT).unwrap();
    document["items"].as_array().unwrap().iter().map(|item| item["value"].as_i64().unwrap() as i32).sum()
}
