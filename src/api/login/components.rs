use std::fmt::Display;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

use crate::api::modules::components::ModuleID;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoginStatusErrors
{
	SIGN_DISABLED,
	SALT_INVALID,
	USER_DISCONNECTED,
	USER_NOT_FOUND,
	USER_INVALID_PWD,
	USER_ALREADY_EXISTS,
	LOCKED(i64), // Unix timestamp in seconds when another attempt becomes possible
	SERVER_ERROR,
}

impl Display for LoginStatusErrors
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return write!(f, "{:?}", self);
	}
}

impl FromServerFnError for LoginStatusErrors {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(_value: ServerFnErrorErr) -> Self {
		LoginStatusErrors::SERVER_ERROR
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordRotationContent
{
	pub id: ModuleID,
	pub content: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordRotationSnapshot
{
	pub rotationId: String,
	pub credentialSalt: String,
	pub credentialVersion: u64,
	pub revision: String,
	#[serde(default)]
	pub preferences: Option<String>,
	pub contents: Vec<PasswordRotationContent>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordRotationFinalize
{
	pub rotationId: String,
	pub credentialVersion: u64,
	pub revision: String,
	pub oldCredential: String,
	pub newCredential: String,
	#[serde(default)]
	pub preferences: Option<String>,
	pub contents: Vec<PasswordRotationContent>,
}

impl std::fmt::Debug for PasswordRotationFinalize
{
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("PasswordRotationFinalize")
			.field("rotationId",&self.rotationId)
			.field("credentialVersion",&self.credentialVersion)
			.field("revision",&"[REDACTED]")
			.field("oldCredential",&"[REDACTED]")
			.field("newCredential",&"[REDACTED]")
			.field("preferences",&self.preferences.as_ref().map(|_| "[REDACTED]"))
			.field("contentCount",&self.contents.len())
			.finish();
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, strum_macros::Display)]
#[strum(prefix = "FRONTOPTIONS_PREFERENCES_")]
pub enum AccountPreferencesError
{
	AUTH_REQUIRED,
	CONTENT_INVALID,
	CRYPTO_FAILED,
	STORAGE_FAILED,
	SERVER_ERROR,
}

impl FromServerFnError for AccountPreferencesError
{
	type Encoder = JsonEncoding;

	fn from_server_fn_error(_value: ServerFnErrorErr) -> Self
	{
		return Self::SERVER_ERROR;
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, strum_macros::Display)]
#[strum(prefix = "FRONTOPTIONS_PASSWORD_")]
pub enum PasswordRotationError
{
	AUTH_REQUIRED,
	REAUTH_REQUIRED,
	CURRENT_INVALID,
	NEW_TOO_SHORT,
	CONFIRMATION_MISMATCH,
	UNCHANGED,
	CONTENT_INVALID,
	CONFLICT,
	STORAGE_FAILED,
	SERVER_ERROR,
}

impl FromServerFnError for PasswordRotationError
{
	type Encoder = JsonEncoding;

	fn from_server_fn_error(_value: ServerFnErrorErr) -> Self
	{
		return Self::SERVER_ERROR;
	}
}

#[cfg(test)]
mod tests
{
	use super::{PasswordRotationContent, PasswordRotationFinalize};
	use crate::api::modules::components::ModuleID;

	#[test]
	fn passwordRotationRequest_debugNeverExposesCredentialsOrCiphertexts()
	{
		let request = PasswordRotationFinalize {
			rotationId: "rotation-id".to_string(),
			credentialVersion: 2,
			revision: "sensitive-revision".to_string(),
			oldCredential: "sensitive-old-credential".to_string(),
			newCredential: "sensitive-new-credential".to_string(),
			preferences: Some("sensitive-preferences-ciphertext".to_string()),
			contents: vec![PasswordRotationContent {
				id: ModuleID {id: "module".to_string()},
				content: "sensitive-ciphertext".to_string(),
			}],
		};
		let output = format!("{:?}",request);

		assert!(output.contains("rotation-id"));
		assert!(output.contains("contentCount: 1"));
		assert!(!output.contains("sensitive-revision"));
		assert!(!output.contains("sensitive-old-credential"));
		assert!(!output.contains("sensitive-new-credential"));
		assert!(!output.contains("sensitive-preferences-ciphertext"));
		assert!(!output.contains("sensitive-ciphertext"));
	}
}
