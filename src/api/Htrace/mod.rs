use leptos::prelude::ServerFnError;
use leptos::server;

#[cfg(feature = "ssr")]
use Htrace::components::level::Level;
#[cfg(feature = "ssr")]
use Htrace::htracer::HTracer;
use serde::{Deserialize, Serialize};

#[server]
pub async fn API_Htrace_log( content: String, htype: Type, file: String, line: u32) -> Result<(), ServerFnError>
{
	let traceFrontLog = crate::api::IS_TRACE_FRONT_LOG.get().map(|ab| ab.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
	if(!traceFrontLog) {
		return Err(ServerFnError::ServerError("Disabled".to_string()));
	}
	HTracer::trace(&content, htype.to_Htype(), file.as_str(), line, vec![]);
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
	pub fn to_Htype(&self) -> Level
	{
		match self {
			Type::DEBUG => Level::DEBUG,
			Type::NORMAL => Level::NORMAL,
			Type::NOTICE => Level::NOTICE,
			Type::NOTICEDERR => Level::NOTICEDERR,
			Type::WARNING => Level::WARNING,
			Type::DEBUGERR => Level::DEBUGERR,
			Type::ERROR => Level::ERROR,
			Type::FATAL => Level::FATAL,
		}
	}
}