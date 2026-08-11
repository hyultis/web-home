use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

pub const PROXY_CACHE_DIR: &str = crate::api::proxys::proxy_cache::CACHE_DIR;

pub fn runtimeConfig_set(traceFrontLog: bool, allowRegistration: bool)
{
	crate::api::runtimeConfig_set(traceFrontLog,allowRegistration);
}

pub fn traceFrontLog_enabled(configured: bool, production: bool) -> bool
{
	return crate::api::Htrace::TraceRuntimePolicy::enabled_get(configured,production);
}

pub fn sessionLayer_get() -> SessionManagerLayer<MemoryStore>
{
	return crate::api::login::session::SessionCookie::layer_get();
}

pub async fn sessionErrorActivity_renew(
	session: Session,
	request: Request,
	next: Next,
) -> Response
{
	return crate::api::login::session::SessionCookie::serverErrorActivity_renew(session,request,next).await;
}
