use std::fs;

use axum::extract::{Request,State};
use axum::http::header::{CACHE_CONTROL, HeaderName, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use Htrace::components::level::Level;
use Htrace::HTrace;
use leptos::prelude::LeptosOptions;

#[derive(Clone)]
pub(super) struct BrowserAssetDelivery
{
	wasmHash: Option<HeaderValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserAssetCachePolicy
{
	Preserve,
	Revalidate,
	Immutable,
}

impl BrowserAssetDelivery
{
	const IMMUTABLE: &'static str = "public, max-age=31536000, immutable";
	const REVALIDATE: &'static str = "no-cache";
	const WASM_HASH_HEADER: &'static str = "x-webhome-wasm-hash";

	pub(super) fn new(options: &LeptosOptions) -> Self
	{
		if (!options.hash_files)
		{
			HTrace!((Level::WARNING) "Browser asset hash header disabled because file hashing is disabled");
			return Self {wasmHash: None};
		}
		let hashPath = std::env::current_exe()
			.map(|path| path.parent().map(|parent| parent.to_path_buf()).unwrap_or_default())
			.unwrap_or_default()
			.join(options.hash_file.as_ref());
		let wasmHash = fs::read_to_string(hashPath)
			.map_err(|error| format!("hash file read failed: {:?}",error.kind()))
			.and_then(|content| Self::wasmHash_parse(&content));
		return match wasmHash
		{
			Ok(wasmHash) => Self {wasmHash: Some(wasmHash)},
			Err(reason) =>
			{
				HTrace!((Level::WARNING) "Browser asset hash header unavailable: {}",reason);
				Self {wasmHash: None}
			},
		};
	}

	pub(super) async fn headers_apply(
		State(delivery): State<Self>,
		request: Request,
		next: Next,
	) -> Response
	{
		let method = request.method().clone();
		let path = request.uri().path().to_string();
		let isApi = Self::apiPath_is(&path);
		let mut response = next.run(request).await;

		if (isApi)
		{
			if let Some(wasmHash) = delivery.wasmHash
			{
				response.headers_mut().insert(
					HeaderName::from_static(Self::WASM_HASH_HEADER),
					wasmHash,
				);
			}
		}
		if (response.headers().contains_key(CACHE_CONTROL))
		{
			return response;
		}

		let value = match Self::cachePolicy_get(&method,&path,response.status())
		{
			BrowserAssetCachePolicy::Preserve => return response,
			BrowserAssetCachePolicy::Revalidate => HeaderValue::from_static(Self::REVALIDATE),
			BrowserAssetCachePolicy::Immutable => HeaderValue::from_static(Self::IMMUTABLE),
		};
		response.headers_mut().insert(CACHE_CONTROL,value);
		return response;
	}

	fn wasmHash_parse(content: &str) -> Result<HeaderValue,String>
	{
		let hash = content.lines()
			.filter_map(|line| line.trim().split_once(':'))
			.find_map(|(asset,hash)| (asset.trim() == "wasm").then_some(hash.trim()))
			.ok_or_else(|| "wasm hash is missing".to_string())?;
		if (hash.is_empty()
			|| hash.len() > 128
			|| !hash.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte,b'-' | b'_')))
		{
			return Err("wasm hash is invalid".to_string());
		}
		return hash.parse::<HeaderValue>().map_err(|_| "wasm hash cannot be used as an HTTP header".to_string());
	}

	fn cachePolicy_get(method: &Method, path: &str, status: StatusCode) -> BrowserAssetCachePolicy
	{
		if (method != Method::GET && method != Method::HEAD)
		{
			return BrowserAssetCachePolicy::Preserve;
		}
		if (Self::operationalPath_is(path))
		{
			return BrowserAssetCachePolicy::Preserve;
		}
		if (path.starts_with("/pkg/") && (status.is_success() || status == StatusCode::NOT_MODIFIED))
		{
			return BrowserAssetCachePolicy::Immutable;
		}
		return BrowserAssetCachePolicy::Revalidate;
	}

	fn operationalPath_is(path: &str) -> bool
	{
		return Self::apiPath_is(path) || path == "/health" || path == "/csp-report";
	}

	fn apiPath_is(path: &str) -> bool
	{
		return path == "/api" || path.starts_with("/api/");
	}

	#[cfg(test)]
	fn test_get() -> Self
	{
		return Self {wasmHash: Some(HeaderValue::from_static("test-wasm-hash"))};
	}
}

#[cfg(test)]
mod tests
{
	use super::BrowserAssetDelivery;
	use axum::body::Body;
	use axum::http::header::CACHE_CONTROL;
	use axum::http::{Request, StatusCode};
	use axum::middleware;
	use axum::response::IntoResponse;
	use axum::routing::{get, post};
	use axum::Router;
	use tower::ServiceExt;

	fn router_get() -> Router
	{
		return Router::new()
			.route("/home",get(|| async {"home"}))
			.route("/favicon.png",get(|| async {"asset"}))
			.route("/pkg/webhome.hash.js",get(|| async {"hashed"}))
			.route("/pkg/revalidated.hash.js",get(|| async {StatusCode::NOT_MODIFIED}))
			.route("/pkg/missing.js",get(|| async {StatusCode::NOT_FOUND}))
			.route("/api/action",post(|| async {StatusCode::NO_CONTENT}))
			.route("/explicit",get(|| async {
				return ([(CACHE_CONTROL,"private, no-store")],"private").into_response();
			}))
			.fallback(|| async {StatusCode::NOT_FOUND})
			.layer(middleware::from_fn_with_state(
				BrowserAssetDelivery::test_get(),
				BrowserAssetDelivery::headers_apply,
			));
	}

	async fn request_get(path: &str, method: &str) -> axum::response::Response
	{
		return router_get().oneshot(
			Request::builder().method(method).uri(path).body(Body::empty()).unwrap()
		).await.unwrap();
	}

	#[tokio::test]
	async fn hashedPackageAssetsAreImmutable()
	{
		for (path,method) in [
			("/pkg/webhome.hash.js","GET"),
			("/pkg/webhome.hash.js","HEAD"),
			("/pkg/revalidated.hash.js","GET"),
		]
		{
			let response = request_get(path,method).await;
			assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(),BrowserAssetDelivery::IMMUTABLE,"{method} {path}");
		}
	}

	#[tokio::test]
	async fn documentsAndStableAssetsAlwaysRevalidate()
	{
		for path in ["/home","/favicon.png","/pkg/missing.js"]
		{
			let response = request_get(path,"GET").await;
			assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(),BrowserAssetDelivery::REVALIDATE,"path {path}");
		}
	}

	#[tokio::test]
	async fn everyApiResponseCarriesTheCurrentWasmHashIncludingNotFound()
	{
		for (path,method,status) in [
			("/api/action","POST",StatusCode::NO_CONTENT),
			("/api/removed-route","POST",StatusCode::NOT_FOUND),
		]
		{
			let response = request_get(path,method).await;
			assert_eq!(response.status(),status);
			assert_eq!(response.headers().get(BrowserAssetDelivery::WASM_HASH_HEADER).unwrap(),"test-wasm-hash");
			assert!(response.headers().get(CACHE_CONTROL).is_none());
		}
	}

	#[tokio::test]
	async fn nonApiAndExistingApplicationPoliciesRemainUntouched()
	{
		let explicitResponse = request_get("/explicit","GET").await;

		assert!(explicitResponse.headers().get(BrowserAssetDelivery::WASM_HASH_HEADER).is_none());
		assert_eq!(explicitResponse.headers().get(CACHE_CONTROL).unwrap(),"private, no-store");
	}

	#[test]
	fn hashManifestAcceptsOnlyTheCargoLeptosWasmHash()
	{
		let parsed = BrowserAssetDelivery::wasmHash_parse(
			"js: one\nwasm: valid_WASM-hash\ncss: two\n"
		).unwrap();

		assert_eq!(parsed,"valid_WASM-hash");
		assert!(BrowserAssetDelivery::wasmHash_parse("js: one\ncss: two\n").is_err());
		assert!(BrowserAssetDelivery::wasmHash_parse("wasm: invalid/hash\n").is_err());
	}
}
