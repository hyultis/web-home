use axum::http::StatusCode;

pub(crate) struct DeploymentHealth;

impl DeploymentHealth
{
	pub(crate) const PATH: &'static str = "/health";

	pub(crate) async fn response_get() -> StatusCode
	{
		return StatusCode::NO_CONTENT;
	}

	pub(crate) fn path_is(path: &str) -> bool
	{
		return path == Self::PATH;
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

	use web_home::server::sessionLayer_get;
	use super::DeploymentHealth;

	#[test]
	fn endpoint_contract_isStableAndDoesNotCreateSession()
	{
		assert_eq!(DeploymentHealth::PATH, "/health");
		assert!(DeploymentHealth::path_is("/health"));
		assert!(!DeploymentHealth::path_is("/"));

		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let router = Router::new()
				.route("/application", get(|| async { StatusCode::NO_CONTENT }))
				.layer(sessionLayer_get())
				.route(DeploymentHealth::PATH, get(DeploymentHealth::response_get));
			let response = router.oneshot(
				Request::builder().uri(DeploymentHealth::PATH).body(Body::empty()).unwrap()
			).await.unwrap();

			assert_eq!(response.status(), StatusCode::NO_CONTENT);
			assert!(response.headers().get(SET_COOKIE).is_none());
		});
	}
}
