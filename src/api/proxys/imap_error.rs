#[cfg(feature = "ssr")]
use imap::Error;
use leptoaster::ToastLevel;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};
use crate::api::IsToastable;

#[derive(Debug, Clone, Deserialize, Serialize,strum_macros::Display)]
#[strum(prefix = "IMAP_ERROR_")]
pub enum ImapError {
	IMAP_SERVER_CONNECTION,
	IMAP_SERVER_CONNECTION_TLS,
	MAIL_NOT_FOUND,
	INVALID_DATE,
	SERVER_ERROR,
}

impl FromServerFnError for ImapError {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		ImapError::SERVER_ERROR
	}
}

impl IsToastable for ImapError {
	fn level(&self) -> Option<ToastLevel> {
		match self {
			ImapError::IMAP_SERVER_CONNECTION => Some(ToastLevel::Error),
			ImapError::IMAP_SERVER_CONNECTION_TLS => Some(ToastLevel::Error),
			ImapError::INVALID_DATE => Some(ToastLevel::Error),
			ImapError::SERVER_ERROR => Some(ToastLevel::Error),
			ImapError::MAIL_NOT_FOUND => Some(ToastLevel::Error),
		}
	}
}


#[cfg(feature = "ssr")]
impl From<Error> for ImapError {
	fn from(value: Error) -> Self {
		match value {
			Error::Io(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::TlsHandshake(_) => ImapError::IMAP_SERVER_CONNECTION_TLS,
			Error::Tls(_) => ImapError::IMAP_SERVER_CONNECTION_TLS,
			Error::Bad(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::No(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::Bye(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::ConnectionLost => ImapError::IMAP_SERVER_CONNECTION,
			Error::Parse(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::Validate(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::Append => ImapError::IMAP_SERVER_CONNECTION,
			Error::Unexpected(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::MissingStatusResponse => ImapError::IMAP_SERVER_CONNECTION,
			Error::TagMismatch(_) => ImapError::IMAP_SERVER_CONNECTION,
			Error::StartTlsNotAvailable => ImapError::IMAP_SERVER_CONNECTION_TLS,
			Error::TlsNotConfigured => ImapError::IMAP_SERVER_CONNECTION_TLS,
			_ => ImapError::SERVER_ERROR
		}
	}
}