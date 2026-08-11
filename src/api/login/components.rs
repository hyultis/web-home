use std::fmt::Display;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

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
