use time::Duration;
use tower_sessions::{cookie::SameSite, Expiry, MemoryStore, SessionManagerLayer};

pub(crate) struct SessionCookie;

impl SessionCookie
{
	pub(crate) fn layer_get() -> SessionManagerLayer<MemoryStore>
	{
		return SessionManagerLayer::new(MemoryStore::default())
			.with_http_only(true)
			.with_same_site(SameSite::Strict)
			.with_secure(true)
			.with_path("/")
			.with_expiry(Expiry::OnInactivity(Duration::days(1)))
			.with_always_save(true);
	}
}

#[cfg(test)]
mod tests
{
	use axum::body::Body;
	use axum::http::header::SET_COOKIE;
	use axum::http::{Request, StatusCode};
	use axum::routing::get;
	use axum::Router;
	use tower::ServiceExt;
	use tower_sessions::Session;

	use super::SessionCookie;

	struct SessionCookieHeaderTest;

	impl SessionCookieHeaderTest
	{
		async fn session_create(session: Session) -> StatusCode
		{
			session.insert("test.authenticated", true).await.unwrap();
			return StatusCode::NO_CONTENT;
		}
	}

	#[test]
	fn sessionCookie_httpHeaderHasProductionSecurityAttributes()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let router = Router::new()
				.route("/session", get(SessionCookieHeaderTest::session_create))
				.layer(SessionCookie::layer_get());
			let response = router.oneshot(
				Request::builder().uri("/session").body(Body::empty()).unwrap()
			).await.unwrap();

			assert_eq!(response.status(), StatusCode::NO_CONTENT);
			let cookie = response.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			assert!(cookie.starts_with("id="));
			assert!(cookie.contains("HttpOnly"));
			assert!(cookie.contains("SameSite=Strict"));
			assert!(cookie.contains("Secure"));
			assert!(cookie.contains("Path=/"));
			assert!(cookie.contains("Max-Age=86400"));
			assert!(!cookie.contains("Domain="));
			assert!(!cookie.contains("userSalt"));
			assert!(!cookie.contains("generatedId"));
		});
	}
}
