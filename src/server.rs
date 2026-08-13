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

pub async fn passwordRotationBodyLimit_apply(
	session: Session,
	mut request: Request,
	next: Next,
) -> Response
{
	use axum::extract::DefaultBodyLimit;
	use leptos::server_fn::ServerFn;

	let isPasswordRotation = request.uri().path()
		== <crate::api::login::ApiUserPasswordrotationFinalize as ServerFn>::PATH;
	if (isPasswordRotation
		&& crate::api::login::user_back::AuthenticatedUser::session_passwordRotationBody_isAllowed(&session).await)
	{
		DefaultBodyLimit::max(crate::api::login::user_back::PASSWORD_ROTATION_REQUEST_MAXIMUM_BYTES)
			.apply(&mut request);
	}
	return next.run(request).await;
}
