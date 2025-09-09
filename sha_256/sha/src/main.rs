use risc0_zkvm::{
    guest::{env, sha::Impl},
    sha::Sha256,
};

fn main() {
    let mut input_bytes = [0u8; 32];
    env::read_slice(&mut input_bytes);

    // Find the actual string length by looking for the first null byte
    let actual_length = input_bytes.iter().position(|&x| x == 0).unwrap_or(32);
    let input = std::str::from_utf8(&input_bytes[..actual_length]).unwrap();

    println!("Input: {}", input);

    let input_digest =
        Impl::hash_bytes(&[input.as_bytes()].concat());
    env::commit_slice(&input_digest.as_bytes());

    let hash_hex = hex::encode(input_digest.as_bytes());

    println!("SHA-256 Hash: {}", hash_hex);
    env::commit_slice(&hash_hex.as_bytes());
}