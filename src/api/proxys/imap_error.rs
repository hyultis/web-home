#[cfg(feature = "ssr")]
use imap::Error;
use leptoaster::ToastLevel;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};
use crate::api::IsToastable;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize,strum_macros::Display)]
#[strum(prefix = "IMAP_ERROR_")]
pub enum ImapError {
	IMAP_SERVER_CONNECTION,
	IMAP_SERVER_CONNECTION_TLS,
	MAIL_NOT_FOUND,
	INVALID_DATE,
	AUTH_REQUIRED,
	DESTINATION_FORBIDDEN,
	RESOURCE_LIMIT,
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
			ImapError::AUTH_REQUIRED => Some(ToastLevel::Error),
			ImapError::DESTINATION_FORBIDDEN => Some(ToastLevel::Error),
			ImapError::RESOURCE_LIMIT => Some(ToastLevel::Error),
			ImapError::SERVER_ERROR => Some(ToastLevel::Error),
			ImapError::MAIL_NOT_FOUND => Some(ToastLevel::Error),
		}
	}

	fn authenticationRequired_get(&self) -> bool
	{
		return self == &Self::AUTH_REQUIRED;
	}
}

#[cfg(feature = "ssr")]
impl From<tokio::task::JoinError> for ImapError
{
	fn from(value: tokio::task::JoinError) -> Self
	{
		use Htrace::HTrace;

		HTrace!(
			"[IMAP proxy] blocking task failed (cancelled: {}, panic: {})",
			value.is_cancelled(),
			value.is_panic()
		);
		return Self::SERVER_ERROR;
	}
}

#[cfg(feature = "ssr")]
impl From<crate::api::proxys::outbound_policy::OutboundPolicyError> for ImapError
{
	fn from(value: crate::api::proxys::outbound_policy::OutboundPolicyError) -> Self
	{
		use crate::api::proxys::outbound_policy::OutboundPolicyError;

		return match value
		{
			OutboundPolicyError::AuthenticationRequired => Self::AUTH_REQUIRED,
			OutboundPolicyError::DestinationForbidden => Self::DESTINATION_FORBIDDEN,
			OutboundPolicyError::ConfigurationInvalid |
			OutboundPolicyError::Internal |
			OutboundPolicyError::ResolutionFailed |
			OutboundPolicyError::ResourceLimitReached => Self::SERVER_ERROR,
		};
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
