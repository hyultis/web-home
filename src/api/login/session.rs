use time::Duration;
use tower_sessions::{
	cookie::{Cookie, SameSite},
	Expiry, MemoryStore, Session, SessionManagerLayer,
};

pub(crate) struct SessionCookie;

impl SessionCookie
{
	const COOKIE_NAME: &'static str = "id";
	const COOKIE_PATH: &'static str = "/";
	const INACTIVITY_DURATION: Duration = Duration::days(1);

	pub(crate) fn layer_get() -> SessionManagerLayer<MemoryStore>
	{
		return Self::layerWithStore_get(MemoryStore::default());
	}

	fn layerWithStore_get(store: MemoryStore) -> SessionManagerLayer<MemoryStore>
	{
		return SessionManagerLayer::new(store)
			.with_name(Self::COOKIE_NAME)
			.with_http_only(true)
			.with_same_site(SameSite::Strict)
			.with_secure(true)
			.with_path(Self::COOKIE_PATH)
			.with_expiry(Expiry::OnInactivity(Self::INACTIVITY_DURATION))
			.with_always_save(true);
	}

	pub(crate) async fn serverErrorActivity_renew(
		session: Session,
		request: axum::extract::Request,
		next: axum::middleware::Next,
	) -> axum::response::Response
	{
		use axum::http::header::SET_COOKIE;
		use axum::http::HeaderValue;
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		let sessionIdWasPresent = session.id().is_some();
		let mut response = next.run(request).await;
		if (!sessionIdWasPresent || !response.status().is_server_error())
		{
			return response;
		}
		let authenticated = match super::user_back::AuthenticatedUser::session_isAuthenticated(&session).await
		{
			Ok(authenticated) => authenticated,
			Err(_) =>
			{
				HTrace!((Level::ERROR) "session authentication check failed during 5xx renewal");
				return response;
			},
		};
		if (!authenticated)
		{
			return response;
		}
		if let Err(error) = session.save().await
		{
			HTrace!((Level::ERROR) "session save failed during 5xx renewal: {}", error);
			return response;
		}
		let Some(sessionId) = session.id()
		else
		{
			HTrace!((Level::ERROR) "session id missing after 5xx renewal");
			return response;
		};
		let cookie = Cookie::build((Self::COOKIE_NAME, sessionId.to_string()))
			.http_only(true)
			.same_site(SameSite::Strict)
			.secure(true)
			.path(Self::COOKIE_PATH)
			.max_age(Self::INACTIVITY_DURATION)
			.build();
		let header = match HeaderValue::from_str(&cookie.to_string())
		{
			Ok(header) => header,
			Err(error) =>
			{
				HTrace!((Level::ERROR) "session cookie serialization failed during 5xx renewal: {}", error);
				return response;
			},
		};
		response.headers_mut().append(SET_COOKIE, header);
		return response;
	}
}

#[cfg(test)]
mod tests
{
	use axum::body::Body;
	use axum::http::header::{COOKIE, SET_COOKIE};
	use axum::http::{Request, StatusCode};
	use axum::routing::get;
	use axum::{middleware, Router};
	use tower::ServiceExt;
	use tower_sessions::{session::Id, MemoryStore, Session, SessionStore};

	use super::SessionCookie;

	struct SessionCookieHeaderTest;

	impl SessionCookieHeaderTest
	{
		async fn session_create(session: Session) -> StatusCode
		{
			session.insert("test.authenticated", true).await.unwrap();
			return StatusCode::NO_CONTENT;
		}

		async fn session_read(session: Session) -> StatusCode
		{
			return match session.get::<bool>("test.authenticated").await
			{
				Ok(Some(true)) => StatusCode::NO_CONTENT,
				_ => StatusCode::UNAUTHORIZED,
			};
		}

		async fn authenticatedSession_create(session: Session) -> StatusCode
		{
			session.insert("auth.user", serde_json::json!({ "identity": "test" })).await.unwrap();
			return StatusCode::NO_CONTENT;
		}

		async fn authenticatedSession_serverError(session: Session) -> StatusCode
		{
			let _ = session.get_value("auth.user").await.unwrap();
			return StatusCode::INTERNAL_SERVER_ERROR;
		}

		async fn anonymous_serverError() -> StatusCode
		{
			return StatusCode::INTERNAL_SERVER_ERROR;
		}

		async fn authenticatedSession_flushAndServerError(session: Session) -> StatusCode
		{
			session.flush().await.unwrap();
			return StatusCode::INTERNAL_SERVER_ERROR;
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

	#[test]
	fn sessionCookie_activityRenewsServerAndBrowserExpiry()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let store = MemoryStore::default();
			let router = Router::new()
				.route("/session", get(SessionCookieHeaderTest::session_create))
				.route("/session/read", get(SessionCookieHeaderTest::session_read))
				.layer(SessionCookie::layerWithStore_get(store.clone()));
			let firstResponse = router.clone().oneshot(
				Request::builder().uri("/session").body(Body::empty()).unwrap()
			).await.unwrap();
			assert_eq!(firstResponse.status(), StatusCode::NO_CONTENT);

			let firstCookieHeader = firstResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			let firstCookie = firstCookieHeader.split(';').next().unwrap().to_string();
			let sessionId = firstCookie.strip_prefix("id=").unwrap().parse().unwrap();
			let firstRecord = store.load(&sessionId).await.unwrap().unwrap();
			tokio::time::sleep(std::time::Duration::from_millis(5)).await;

			let secondResponse = router.oneshot(
				Request::builder()
					.uri("/session/read")
					.header(COOKIE, &firstCookie)
					.body(Body::empty())
					.unwrap()
			).await.unwrap();
			assert_eq!(secondResponse.status(), StatusCode::NO_CONTENT);

			let secondCookieHeader = secondResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			let secondCookie = secondCookieHeader.split(';').next().unwrap();
			let secondRecord = store.load(&sessionId).await.unwrap().unwrap();
			assert_eq!(secondCookie, firstCookie);
			assert!(secondCookieHeader.contains("Max-Age=86400"));
			assert!(secondRecord.expiry_date > firstRecord.expiry_date);
		});
	}

	#[test]
	fn sessionCookie_malformedAuthenticatedStateDoesNotRenewServerError()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let store = MemoryStore::default();
			let router = Router::new()
				.route("/session", get(SessionCookieHeaderTest::authenticatedSession_create))
				.route("/session/error", get(SessionCookieHeaderTest::authenticatedSession_serverError))
				.layer(middleware::from_fn(SessionCookie::serverErrorActivity_renew))
				.layer(SessionCookie::layerWithStore_get(store.clone()));
			let firstResponse = router.clone().oneshot(
				Request::builder().uri("/session").body(Body::empty()).unwrap()
			).await.unwrap();
			let firstCookieHeader = firstResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			let firstCookie = firstCookieHeader.split(';').next().unwrap().to_string();
			let sessionId = firstCookie.strip_prefix("id=").unwrap().parse().unwrap();
			let firstRecord = store.load(&sessionId).await.unwrap().unwrap();

			let secondResponse = router.oneshot(
				Request::builder()
					.uri("/session/error")
					.header(COOKIE, &firstCookie)
					.body(Body::empty())
					.unwrap()
			).await.unwrap();
			assert_eq!(secondResponse.status(), StatusCode::INTERNAL_SERVER_ERROR);

			let secondRecord = store.load(&sessionId).await.unwrap().unwrap();
			assert!(secondResponse.headers().get(SET_COOKIE).is_none());
			assert_eq!(secondRecord.expiry_date,firstRecord.expiry_date);
		});
	}

	#[test]
	fn sessionCookie_serverErrorDoesNotRenewAnonymousOrFlushedSession()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let store = MemoryStore::default();
			let router = Router::new()
				.route("/session", get(SessionCookieHeaderTest::authenticatedSession_create))
				.route("/session/error", get(SessionCookieHeaderTest::anonymous_serverError))
				.route("/session/flush-error", get(SessionCookieHeaderTest::authenticatedSession_flushAndServerError))
				.layer(middleware::from_fn(SessionCookie::serverErrorActivity_renew))
				.layer(SessionCookie::layerWithStore_get(store.clone()));

			let anonymousResponse = router.clone().oneshot(
				Request::builder().uri("/session/error").body(Body::empty()).unwrap()
			).await.unwrap();
			assert_eq!(anonymousResponse.status(), StatusCode::INTERNAL_SERVER_ERROR);
			assert!(anonymousResponse.headers().get(SET_COOKIE).is_none());

			let staleCookie = format!("id={}", Id::default());
			let staleResponse = router.clone().oneshot(
				Request::builder()
					.uri("/session/error")
					.header(COOKIE, staleCookie)
					.body(Body::empty())
					.unwrap()
			).await.unwrap();
			let staleCookieHeader = staleResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			assert!(staleCookieHeader.contains("Max-Age=0"));
			assert!(!staleCookieHeader.contains("Max-Age=86400"));

			let createdResponse = router.clone().oneshot(
				Request::builder().uri("/session").body(Body::empty()).unwrap()
			).await.unwrap();
			let createdCookieHeader = createdResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			let createdCookie = createdCookieHeader.split(';').next().unwrap().to_string();
			let sessionId = createdCookie.strip_prefix("id=").unwrap().parse().unwrap();
			let flushedResponse = router.oneshot(
				Request::builder()
					.uri("/session/flush-error")
					.header(COOKIE, &createdCookie)
					.body(Body::empty())
					.unwrap()
			).await.unwrap();
			assert_eq!(flushedResponse.status(), StatusCode::INTERNAL_SERVER_ERROR);
			let flushedCookieHeader = flushedResponse.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			assert!(flushedCookieHeader.contains("Max-Age=0"));
			assert!(!flushedCookieHeader.contains("Max-Age=86400"));
			assert!(store.load(&sessionId).await.unwrap().is_none());
		});
	}
}
