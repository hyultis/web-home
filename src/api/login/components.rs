use std::fmt::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoginStatus
{
	USER_IS_CONNECTED(bool),
	USER_CONNECTED,
	USER_DISCONNECTED,
	USER_NOT_FOUND,
	USER_INVALID_PWD,
	USER_ALREADY_EXISTS,
	LOCKED(i64), // duration in seconds
	SERVER_ERROR,
}

impl Display for LoginStatus
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return write!(f, "{:?}", self);
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SaltReturn
{
	SALT(String),
	ERROR(String)
}