use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use Htrace::components::level::Level;
#[cfg(feature = "ssr")]
use Htrace::htracer::HTracer;

#[server]
pub async fn API_Htrace_log(content: String, htype: Type, file: String, line: u32) -> Result<(), ServerFnError>
{
	let input = TracePolicy::input_get(content, file).await?;
	HTracer::trace(&input.content, htype.to_Htype(), &input.file, line, vec![]);
	return Ok(());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Type
{
	DEBUG,
	NORMAL,
	NOTICE,
	NOTICEDERR,
	WARNING,
	DEBUGERR,
	ERROR,
	FATAL,
}

#[cfg(feature = "ssr")]
impl Type
{
	fn to_Htype(&self) -> Level
	{
		return match self
		{
			Type::DEBUG => Level::DEBUG,
			Type::NORMAL => Level::NORMAL,
			Type::NOTICE => Level::NOTICE,
			Type::NOTICEDERR => Level::NOTICEDERR,
			Type::WARNING => Level::WARNING,
			Type::DEBUGERR => Level::DEBUGERR,
			Type::ERROR => Level::ERROR,
			Type::FATAL => Level::FATAL,
		};
	}
}

#[cfg(feature = "ssr")]
pub(crate) struct TraceRuntimePolicy;

#[cfg(feature = "ssr")]
impl TraceRuntimePolicy
{
	pub(crate) fn enabled_get(configured: bool, production: bool) -> bool
	{
		return configured && !production;
	}
}

#[cfg(feature = "ssr")]
struct TraceInput
{
	content: String,
	file: String,
}

#[cfg(feature = "ssr")]
impl TraceInput
{
	fn new(content: String, file: String) -> Result<Self, ServerFnError>
	{
		if (content.len() > TracePolicy::CONTENT_MAXIMUM_BYTES
			|| file.is_empty()
			|| file.len() > TracePolicy::FILE_MAXIMUM_BYTES
			|| file.split(['/', '\\']).any(|segment| segment == "..")
			|| !file.chars().all(|character| {
				return character.is_ascii_alphanumeric() || matches!(character, '/' | '\\' | '.' | '_' | '-');
			}))
		{
			return Err(ServerFnError::new("Trace input rejected"));
		}
		let content = content.replace('\r', "\\r").replace('\n', "\\n");
		if (content.len() > TracePolicy::CONTENT_MAXIMUM_BYTES)
		{
			return Err(ServerFnError::new("Trace input rejected"));
		}
		return Ok(Self { content, file });
	}
}

#[cfg(feature = "ssr")]
struct TraceRateState
{
	acceptedBytes: usize,
	requestCount: usize,
	windowStartedAt: std::time::Instant,
}

#[cfg(feature = "ssr")]
impl TraceRateState
{
	fn new(now: std::time::Instant) -> Self
	{
		return Self { acceptedBytes: 0, requestCount: 0, windowStartedAt: now };
	}

	fn request_accept(&mut self, now: std::time::Instant, contentBytes: usize) -> bool
	{
		if (now.duration_since(self.windowStartedAt) >= TracePolicy::RATE_WINDOW)
		{
			self.windowStartedAt = now;
			self.requestCount = 0;
		}
		let chargedBytes = contentBytes.max(TracePolicy::ENTRY_MINIMUM_CHARGED_BYTES);
		if (self.requestCount >= TracePolicy::RATE_MAXIMUM
			|| chargedBytes > TracePolicy::PROCESS_MAXIMUM_BYTES.saturating_sub(self.acceptedBytes))
		{
			return false;
		}
		self.requestCount += 1;
		self.acceptedBytes += chargedBytes;
		return true;
	}
}

#[cfg(feature = "ssr")]
struct TracePolicy;

#[cfg(feature = "ssr")]
impl TracePolicy
{
	const CONTENT_MAXIMUM_BYTES: usize = 16 * 1024;
	const ENTRY_MINIMUM_CHARGED_BYTES: usize = 512;
	const FILE_MAXIMUM_BYTES: usize = 512;
	const PROCESS_MAXIMUM_BYTES: usize = 64 * 1024 * 1024;
	const RATE_MAXIMUM: usize = 20;
	const RATE_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

	async fn input_get(content: String, file: String) -> Result<TraceInput, ServerFnError>
	{
		use crate::api::login::user_back::{AuthenticatedUser, UserBackHelperError};

		AuthenticatedUser::current().await.map_err(|error| -> ServerFnError
		{
			return match error
			{
				UserBackHelperError::LoginError(_) => ServerFnError::new("Authentication required"),
				_ => ServerFnError::new("Server error"),
			};
		})?;
		let traceFrontLog = crate::api::IS_TRACE_FRONT_LOG.get()
			.map(|enabled| enabled.load(std::sync::atomic::Ordering::Relaxed))
			.unwrap_or(false);
		if (!traceFrontLog)
		{
			return Err(ServerFnError::new("Disabled"));
		}
		let input = TraceInput::new(content, file)?;
		Self::rate_require(input.content.len())?;
		return Ok(input);
	}

	fn rate_require(contentBytes: usize) -> Result<(), ServerFnError>
	{
		use std::sync::{Mutex, OnceLock};

		static TRACE_RATE: OnceLock<Mutex<TraceRateState>> = OnceLock::new();
		let now = std::time::Instant::now();
		let state = TRACE_RATE.get_or_init(|| Mutex::new(TraceRateState::new(now)));
		let mut state = state.lock().map_err(|_| ServerFnError::new("Server error"))?;
		if (!state.request_accept(now, contentBytes))
		{
			return Err(ServerFnError::new("Trace rate limit reached"));
		}
		return Ok(());
	}
}

#[cfg(all(test, feature = "ssr"))]
mod tests
{
	use super::*;

	#[test]
	fn runtimePolicy_neverEnablesClientTraceInProduction()
	{
		assert!(!TraceRuntimePolicy::enabled_get(true, true));
		assert!(!TraceRuntimePolicy::enabled_get(false, false));
		assert!(TraceRuntimePolicy::enabled_get(true, false));
	}

	#[test]
	fn traceInput_isBoundedAndEscapesLineBreaks()
	{
		let input = TraceInput::new("first\nsecond".to_string(), "src/front/test.rs".to_string()).unwrap();
		assert_eq!(input.content, "first\\nsecond");
		assert!(TraceInput::new("ok".to_string(), "../invalid.rs".to_string()).is_err());
		assert!(TraceInput::new("x".repeat(TracePolicy::CONTENT_MAXIMUM_BYTES + 1), "test.rs".to_string()).is_err());
	}

	#[test]
	fn traceRate_rejectsRequestsBeyondWindowLimit()
	{
		let now = std::time::Instant::now();
		let mut state = TraceRateState::new(now);
		for _ in 0..TracePolicy::RATE_MAXIMUM
		{
			assert!(state.request_accept(now, 1));
		}
		assert!(!state.request_accept(now, 1));
		assert!(state.request_accept(now + TracePolicy::RATE_WINDOW, 1));
	}

	#[test]
	fn traceRate_processQuotaDoesNotResetWithRateWindow()
	{
		let now = std::time::Instant::now();
		let mut state = TraceRateState::new(now);
		state.acceptedBytes = TracePolicy::PROCESS_MAXIMUM_BYTES;
		assert!(!state.request_accept(now + TracePolicy::RATE_WINDOW, 1));
	}
}
