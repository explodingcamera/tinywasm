#![no_main]

#[no_mangle]
pub extern "C" fn argon2id(m_cost: i32, t_cost: i32, p_cost: i32) -> i32 {
    let password = b"password";
    let salt = b"some random salt";

    let params = argon2::Params::new(m_cost as u32, t_cost as u32, p_cost as u32, None).unwrap();
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut hash = [0u8; 32];
    argon.hash_password_into(password, salt, &mut hash).unwrap();
    hash[0] as i32
}
