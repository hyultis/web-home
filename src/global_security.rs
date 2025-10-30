use base64ct::{Base64, Encoding};
use getrandom::Error;
use sha3::{Digest, Sha3_256};

/// generate a salt into a string
pub fn generate_salt() -> Result<String,Error>
{
	return Ok(Base64::encode_string(&generate_salt_raw()?));
}

/// generate a salt into a array of u8
pub fn generate_salt_raw() -> Result<[u8;16],Error>
{
	let mut bytes = [0u8; 16]; // 128 bits de sel
	getrandom::fill(&mut bytes)?;
	return Ok(bytes)
}

/// Generates a hashed representation of the provided string using the SHA3 hashing algorithm.
pub fn hash(str: String) -> String
{
	let mut hasher = Sha3_256::new();
	hasher.update(str);
	let result = hasher.finalize();
	return Base64::encode_string(&result)
}