mod parser;
mod constant;
use std::io::Write;

use chrono::{DateTime, Local};

use base64::{engine::general_purpose, Engine as _};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::digest;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

const TRANSIT_KEY_LENGTH: usize = 32;
const TRANSIT_TIME_BUCKET: u64 = 60;


/// Initializes the logger based on the provided debug flag and cargo information.
///
/// # Arguments
///
/// * `debug` - A flag indicating whether to enable debug mode for detailed logging.
/// * `crate_name` - Name of the crate loaded during compile time.
fn init_logger(debug: bool, utc: bool, crate_name: &String) {
    if debug {
        std::env::set_var("RUST_LOG", format!("{}=debug", crate_name));
        std::env::set_var("RUST_BACKTRACE", "1");
    } else {
        // Set Actix logging to warning mode since it becomes too noisy when streaming large files
        std::env::set_var("RUST_LOG", format!("{}=info", crate_name));
        std::env::set_var("RUST_BACKTRACE", "0");
    }
    if utc {
        env_logger::init();
    } else {
        env_logger::Builder::from_default_env()
            .format(|buf, record| {
                let local_time: DateTime<Local> = Local::now();
                writeln!(
                    buf,
                    "[{} {} {}] - {}",
                    local_time.format("%Y-%m-%dT%H:%M:%SZ"),
                    record.level(),
                    record.target(),
                    record.args()
                )
            })
            .init();
    }
}

/// Decrypts a transit-encrypted payload.
///
/// # Arguments
/// * `ciphertext` - A base64-encoded encrypted string.
///
/// # Returns
/// * A `Result<Value, String>` containing the decrypted JSON payload or an error message.
pub fn transit_decrypt(apikey: &String, ciphertext: &String) -> Result<Value, String> {
    // Compute the current epoch bucket
    let epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => return Err("System time is before the UNIX epoch".into())
    };
    let epoch = epoch / TRANSIT_TIME_BUCKET;

    // Derive the AES key using SHA-256
    let hash_input = format!("{}.{}", epoch, apikey);
    let hash_output = digest::digest(&digest::SHA256, hash_input.as_bytes());
    let aes_key = &hash_output.as_ref()[..TRANSIT_KEY_LENGTH];

    // Decode the base64-encoded ciphertext
    let ciphertext_bytes = match general_purpose::STANDARD.decode(ciphertext) {
        Ok(bytes) => bytes,
        Err(_) => return Err("Failed to decode ciphertext".into())
    };

    // Ensure the ciphertext is long enough
    if ciphertext_bytes.len() < 12 {
        return Err("Ciphertext is too short".into());
    }

    // Extract the nonce (first 12 bytes) and the actual encrypted data
    let (nonce_bytes, encrypted_data) = ciphertext_bytes.split_at(12);

    // Initialize AES-GCM decryption
    let unbound_key = match UnboundKey::new(&aead::AES_256_GCM, aes_key) {
        Ok(key) => key,
        Err(_) => return Err("Failed to create AES key".into()),
    };
    let key = LessSafeKey::new(unbound_key);

    let nonce = match Nonce::try_assume_unique_for_key(nonce_bytes) {
        Ok(n) => n,
        Err(_) => return Err("Failed to create nonce".into()),
    };

    // Decrypt the data
    let mut binding = encrypted_data.to_vec();
    let decrypted_data = match key.open_in_place(nonce, Aad::empty(), &mut binding) {
        Ok(data) => data,
        Err(_) => return Err("Failed to decrypt data".into()),
    };

    // Parse the decrypted data as JSON
    let decrypted_json: Value = match serde_json::from_slice(decrypted_data) {
        Ok(json) => json,
        Err(_) => return Err("Failed to parse decrypted data as JSON".into()),
    };

    Ok(decrypted_json)
}

pub fn decrypt_vault_secret() -> Result<Value, String> {
    let metadata = constant::build_info();
    let config = parser::arguments(&metadata);
    // Instantiate custom logger
    init_logger(
        config.debug,
        config.utc,
        &metadata.crate_name
    );
    transit_decrypt(&config.apikey, &config.cipher)
}