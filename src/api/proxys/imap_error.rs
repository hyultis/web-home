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

#[cfg(feature = "ssr")]
impl ImapError
{
	pub(super) fn trace(self, operation: &'static str, stage: &'static str, mailboxIndex: Option<usize>) -> Self
	{
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		if let Some(mailboxIndex) = mailboxIndex
		{
			HTrace!(
				(Level::ERROR)
				"[IMAP proxy] operation={} stage={} mailbox_index={} error={}",
				operation,
				stage,
				mailboxIndex,
				self
			);
		}
		else
		{
			HTrace!(
				(Level::ERROR)
				"[IMAP proxy] operation={} stage={} error={}",
				operation,
				stage,
				self
			);
		}
		return self;
	}

	pub(super) fn fromImapAt(
		value: Error,
		operation: &'static str,
		stage: &'static str,
		mailboxIndex: Option<usize>,
	) -> Self
	{
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		let source = Self::imapSource_get(&value);
		let result = Self::from(value);
		if let Some(mailboxIndex) = mailboxIndex
		{
			HTrace!(
				(Level::ERROR)
				"[IMAP proxy] operation={} stage={} mailbox_index={} source={} error={}",
				operation,
				stage,
				mailboxIndex,
				source,
				result
			);
		}
		else
		{
			HTrace!(
				(Level::ERROR)
				"[IMAP proxy] operation={} stage={} source={} error={}",
				operation,
				stage,
				source,
				result
			);
		}
		return result;
	}

	pub(super) fn imapSource_get(value: &Error) -> &'static str
	{
		return match value
		{
			Error::Io(_) => "io",
			Error::TlsHandshake(_) => "tls_handshake",
			Error::Tls(_) => "tls",
			Error::Bad(_) => "bad_response",
			Error::No(_) => "no_response",
			Error::Bye(_) => "bye_response",
			Error::ConnectionLost => "connection_lost",
			Error::Parse(_) => "parse",
			Error::Validate(_) => "validate",
			Error::Append => "append",
			Error::Unexpected(_) => "unexpected_response",
			Error::MissingStatusResponse => "missing_status_response",
			Error::TagMismatch(_) => "tag_mismatch",
			Error::StartTlsNotAvailable => "starttls_unavailable",
			Error::TlsNotConfigured => "tls_not_configured",
			_ => "unknown",
		};
	}
}

impl FromServerFnError for ImapError {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		#[cfg(feature = "ssr")]
		{
			use Htrace::components::level::Level;
			use Htrace::HTrace;

			let source = match &value
			{
				ServerFnErrorErr::Registration(_) => "registration",
				ServerFnErrorErr::UnsupportedRequestMethod(_) => "unsupported_method",
				ServerFnErrorErr::Request(_) => "request",
				ServerFnErrorErr::ServerError(_) => "server",
				ServerFnErrorErr::MiddlewareError(_) => "middleware",
				ServerFnErrorErr::Deserialization(_) => "deserialization",
				ServerFnErrorErr::Serialization(_) => "serialization",
				ServerFnErrorErr::Args(_) => "args",
				ServerFnErrorErr::MissingArg(_) => "missing_arg",
				ServerFnErrorErr::Response(_) => "response",
			};
			HTrace!(
				(Level::ERROR)
				"[IMAP proxy] operation=server_fn stage=codec source={} error={}",
				source,
				ImapError::SERVER_ERROR
			);
		}
		#[cfg(not(feature = "ssr"))]
		let _ = value;
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
