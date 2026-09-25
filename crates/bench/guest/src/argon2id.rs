#![no_main]

#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    let params = argon2::Params::new(1024, 2, 1, None).unwrap();
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut hash = [0u8; 32];
    argon.hash_password_into(b"password", b"some random salt", &mut hash).unwrap();
    i32::from_le_bytes(hash[..4].try_into().unwrap())
}
