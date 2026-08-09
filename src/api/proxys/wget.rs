use leptoaster::ToastLevel;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server;
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

use crate::api::IsToastable;

#[derive(Serialize,Deserialize,PartialEq,Debug,Clone,strum_macros::Display)]
#[strum(prefix = "WGET_ERROR_")]
pub enum proxys_return
{
	NOT_MODIFIED,
	BLANK_URL,
	AUTH_REQUIRED,
	DESTINATION_FORBIDDEN,
	RESPONSE_TOO_LARGE,
	SERVER_ERROR,
}

impl FromServerFnError for proxys_return
{
	type Encoder = JsonEncoding;

	fn from_server_fn_error(_value: ServerFnErrorErr) -> Self
	{
		return proxys_return::SERVER_ERROR;
	}
}

impl IsToastable for proxys_return
{
	fn level(&self) -> Option<ToastLevel>
	{
		return match self
		{
			proxys_return::NOT_MODIFIED => None,
			proxys_return::BLANK_URL => Some(ToastLevel::Info),
			proxys_return::AUTH_REQUIRED => Some(ToastLevel::Error),
			proxys_return::DESTINATION_FORBIDDEN => Some(ToastLevel::Error),
			proxys_return::RESPONSE_TOO_LARGE => Some(ToastLevel::Error),
			proxys_return::SERVER_ERROR => Some(ToastLevel::Error),
		};
	}

	fn authenticationRequired_get(&self) -> bool
	{
		return self == &Self::AUTH_REQUIRED;
	}
}

#[cfg(feature = "ssr")]
impl From<crate::api::proxys::outbound_policy::OutboundPolicyError> for proxys_return
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
impl From<std::io::Error> for proxys_return
{
	fn from(value: std::io::Error) -> Self
	{
		use Htrace::HTrace;

		HTrace!("[proxy cache] IO error: {}", value);
		return proxys_return::SERVER_ERROR;
	}
}

#[cfg(feature = "ssr")]
impl From<reqwest::Error> for proxys_return
{
	fn from(value: reqwest::Error) -> Self
	{
		use Htrace::HTrace;

		HTrace!(
			"[RSS proxy] HTTP request failed (timeout: {}, connect: {}, status: {:?})",
			value.is_timeout(),
			value.is_connect(),
			value.status()
		);
		return proxys_return::SERVER_ERROR;
	}
}

/// Fetches an RSS document through the authenticated and validated server proxy.
#[server]
pub async fn API_proxys_wget(url: String, lastUpdate: Option<u64>) -> Result<(u64,String), proxys_return>
{
	use crate::api::proxys::outbound_policy::OutboundPolicy;
	use crate::api::proxys::proxy_cache::ProxyCache;
	use crate::global_security::hash;

	OutboundPolicy::authentication_require().await.map_err(proxys_return::from)?;
	let _permit = OutboundPolicy::httpPermit_get().map_err(proxys_return::from)?;

	if (url.is_empty())
	{
		return Err(proxys_return::BLANK_URL);
	}
	let destination = OutboundPolicy::httpDestination_get(&url).await.map_err(proxys_return::from)?;
	let proxy = inner::RssProxy::new(
		ProxyCache::get("wget")?,
		hash(url),
		destination,
	);
	return match proxy.content_get(lastUpdate).await?
	{
		inner::RssProxyResult::NotModified => Err(proxys_return::NOT_MODIFIED),
		inner::RssProxyResult::Content { content, version } => Ok((version, content)),
	};
}

#[cfg(feature = "ssr")]
mod inner
{
	use std::time::{Duration, SystemTime};

	use Htrace::HTrace;
	use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION};
	use serde::{Deserialize, Serialize};

	use crate::api::proxys::outbound_policy::ValidatedHttpDestination;
	use crate::api::proxys::proxy_cache::ProxyCache;
	use crate::api::proxys::wget::proxys_return;

	struct RssLimits;

	impl RssLimits
	{
		const BODY_MAXIMUM_BYTES: usize = 4 * 1024 * 1024;
		const CACHE_ENTRY_MAXIMUM_BYTES: usize = Self::BODY_MAXIMUM_BYTES * 6 + 64 * 1024;
		const CACHE_MAXIMUM_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
		const CACHE_MAXIMUM_BYTES: u64 = 64 * 1024 * 1024;
		const CACHE_MAXIMUM_ENTRIES: usize = 256;
		const CACHE_TTL_MILLISECONDS: u64 = 5 * 60 * 1_000;
		const VALIDATOR_MAXIMUM_BYTES: usize = 8 * 1024;
	}

	#[derive(Default, Deserialize, Serialize)]
	struct RssCacheMetadata
	{
		#[serde(default)]
		contentVersion: u64,
		#[serde(default)]
		etag: Option<String>,
		#[serde(default)]
		lastModified: Option<String>,
		#[serde(default)]
		validatedAt: u64,
	}

	impl RssCacheMetadata
	{
		fn fresh_is(&self, now: u64) -> bool
		{
			return now >= self.validatedAt
				&& now.saturating_sub(self.validatedAt) <= RssLimits::CACHE_TTL_MILLISECONDS;
		}

		fn revalidated_get(mut self, now: u64, etag: Option<String>, lastModified: Option<String>) -> Self
		{
			self.validatedAt = now;
			if (etag.is_some())
			{
				self.etag = etag;
			}
			if (lastModified.is_some())
			{
				self.lastModified = lastModified;
			}
			return self;
		}
	}

	#[derive(Deserialize, Serialize)]
	struct RssCacheRecord
	{
		content: String,
		metadata: RssCacheMetadata,
	}

	impl RssCacheRecord
	{
		fn clientResult_get(&self, clientVersion: Option<u64>) -> RssProxyResult
		{
			if (clientVersion == Some(self.metadata.contentVersion))
			{
				return RssProxyResult::NotModified;
			}
			return RssProxyResult::Content {
				content: self.content.clone(),
				version: self.metadata.contentVersion,
			};
		}

		fn revalidated_get(mut self, now: u64, etag: Option<String>, lastModified: Option<String>) -> Self
		{
			self.metadata = self.metadata.revalidated_get(now, etag, lastModified);
			return self;
		}
	}

	pub(super) enum RssProxyResult
	{
		NotModified,
		Content
		{
			content: String,
			version: u64,
		},
	}

	enum RssFetchResult
	{
		NotModified
		{
			etag: Option<String>,
			lastModified: Option<String>,
		},
		Updated
		{
			content: String,
			etag: Option<String>,
			lastModified: Option<String>,
		},
	}

	pub(super) struct RssProxy
	{
		cache: ProxyCache,
		cacheKey: String,
		destination: ValidatedHttpDestination,
	}

	impl RssProxy
	{
		pub(super) fn new(cache: ProxyCache, cacheKey: String, destination: ValidatedHttpDestination) -> Self
		{
			return Self { cache, cacheKey, destination };
		}

		pub(super) async fn content_get(self, clientVersion: Option<u64>) -> Result<RssProxyResult, proxys_return>
		{
			let cachedRecord = self.cacheRecord_get()?;
			let now = Self::now_get()?;
			if let Some(record) = &cachedRecord && record.metadata.fresh_is(now)
			{
				return Ok(record.clientResult_get(clientVersion));
			}

			let fetchResult = self.fetch_get(cachedRecord.as_ref().map(|record| &record.metadata)).await?;
			let now = Self::now_get()?;
			let record = match fetchResult
			{
				RssFetchResult::NotModified { etag, lastModified } =>
				{
					let Some(record) = cachedRecord
					else
					{
						return Err(proxys_return::SERVER_ERROR);
					};
					record.revalidated_get(now, etag, lastModified)
				},
				RssFetchResult::Updated { content, etag, lastModified } =>
				{
					let previousVersion = cachedRecord.as_ref()
						.map(|record| record.metadata.contentVersion)
						.unwrap_or(0);
					RssCacheRecord {
						content,
						metadata: RssCacheMetadata {
							contentVersion: now.max(previousVersion.saturating_add(1)),
							etag,
							lastModified,
							validatedAt: now,
						},
					}
				},
			};
			self.cacheRecord_save(&record);
			return Ok(record.clientResult_get(clientVersion));
		}

		fn cacheRecord_get(&self) -> Result<Option<RssCacheRecord>, proxys_return>
		{
			let raw = match self.cache.load(&self.cacheKey, RssLimits::CACHE_ENTRY_MAXIMUM_BYTES)
			{
				Ok(Some(raw)) => raw,
				Ok(None) => return Ok(None),
				Err(error) if error.kind() == std::io::ErrorKind::InvalidData =>
				{
					HTrace!("[RSS proxy] Removing an oversized cache entry");
					self.cache.remove(&self.cacheKey)?;
					return Ok(None);
				},
				Err(error) => return Err(error.into()),
			};
			return match serde_json::from_slice(&raw)
			{
				Ok(record) => Ok(Some(record)),
				Err(error) =>
				{
					HTrace!("[RSS proxy] Ignoring an incompatible cache entry: {}", error);
					self.cache.remove(&self.cacheKey)?;
					Ok(None)
				},
			};
		}

		fn cacheRecord_save(&self, record: &RssCacheRecord)
		{
			let result = serde_json::to_vec(record)
				.map_err(|error| error.to_string())
				.and_then(|raw|
				{
					if (raw.len() > RssLimits::CACHE_ENTRY_MAXIMUM_BYTES)
					{
						return Err("serialized RSS cache entry exceeds its limit".to_string());
					}
					return self.cache.save(&self.cacheKey, &raw).map_err(|error| error.to_string());
				});
			if let Err(error) = result
			{
				HTrace!("[RSS proxy] Cache save failed: {}", error);
				return;
			}
			if let Err(error) = self.cache.cleanup(
				RssLimits::CACHE_MAXIMUM_BYTES,
				RssLimits::CACHE_MAXIMUM_ENTRIES,
				RssLimits::CACHE_MAXIMUM_AGE,
			)
			{
				HTrace!("[RSS proxy] Cache cleanup failed: {}", error);
			}
		}

		async fn fetch_get(&self, metadata: Option<&RssCacheMetadata>) -> Result<RssFetchResult, proxys_return>
		{
			let mut destination = self.destination.clone();
			let mut redirectCount = 0;
			let mut response = loop
			{
				let client = destination.client_get().map_err(proxys_return::from)?;
				let mut request = client.get(destination.url_get().clone());
				if let Some(etag) = metadata.and_then(|metadata| metadata.etag.as_deref())
				{
					request = request.header(IF_NONE_MATCH, etag);
				}
				if let Some(lastModified) = metadata.and_then(|metadata| metadata.lastModified.as_deref())
				{
					request = request.header(IF_MODIFIED_SINCE, lastModified);
				}

				let response = request.send().await?;
				if (!response.status().is_redirection())
				{
					break response;
				}
				if (redirectCount >= ValidatedHttpDestination::redirectMaximum_get())
				{
					return Err(proxys_return::DESTINATION_FORBIDDEN);
				}
				let location = response.headers().get(LOCATION)
					.and_then(|value| value.to_str().ok())
					.ok_or(proxys_return::DESTINATION_FORBIDDEN)?
					.to_string();
				destination = destination.redirected_get(&location).await.map_err(proxys_return::from)?;
				redirectCount += 1;
			};

			let etag = Self::validator_get(response.headers().get(ETAG));
			let lastModified = Self::validator_get(response.headers().get(LAST_MODIFIED));
			match response.status().as_u16()
			{
				304 => return Ok(RssFetchResult::NotModified { etag, lastModified }),
				200 => {},
				_ => return Err(proxys_return::SERVER_ERROR),
			}
			if (response.content_length().is_some_and(|length| length > RssLimits::BODY_MAXIMUM_BYTES as u64))
			{
				return Err(proxys_return::RESPONSE_TOO_LARGE);
			}
			let mut body = BoundedResponseBody::new(
				response.content_length().and_then(|length| usize::try_from(length).ok()),
			);
			while let Some(chunk) = response.chunk().await?
			{
				body.chunk_add(&chunk)?;
			}
			let content = String::from_utf8(body.content_get()).map_err(|_| proxys_return::SERVER_ERROR)?;
			return Ok(RssFetchResult::Updated { content, etag, lastModified });
		}

		fn validator_get(value: Option<&reqwest::header::HeaderValue>) -> Option<String>
		{
			let value = value?.to_str().ok()?;
			if (value.is_empty() || value.len() > RssLimits::VALIDATOR_MAXIMUM_BYTES)
			{
				return None;
			}
			return Some(value.to_string());
		}

		fn now_get() -> Result<u64, proxys_return>
		{
			let duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
				.map_err(|_| proxys_return::SERVER_ERROR)?;
			return Ok(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
		}
	}

	struct BoundedResponseBody
	{
		content: Vec<u8>,
	}

	impl BoundedResponseBody
	{
		fn new(expectedLength: Option<usize>) -> Self
		{
			let capacity = expectedLength.unwrap_or(0).min(RssLimits::BODY_MAXIMUM_BYTES);
			return Self { content: Vec::with_capacity(capacity) };
		}

		fn chunk_add(&mut self, chunk: &[u8]) -> Result<(), proxys_return>
		{
			if (chunk.len() > RssLimits::BODY_MAXIMUM_BYTES.saturating_sub(self.content.len()))
			{
				return Err(proxys_return::RESPONSE_TOO_LARGE);
			}
			self.content.extend_from_slice(chunk);
			return Ok(());
		}

		fn content_get(self) -> Vec<u8>
		{
			return self.content;
		}
	}

	#[cfg(test)]
	mod tests
	{
		use std::net::SocketAddr;
		use std::path::PathBuf;
		use std::sync::Arc;
		use std::sync::atomic::{AtomicUsize, Ordering};

		use axum::body::Body;
		use axum::extract::State;
		use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
		use axum::routing::get;
		use axum::Router;
		use tokio::task::JoinHandle;
		use url::Url;

		use super::*;

		#[derive(Clone)]
		struct RssTestServerState
		{
			address: Arc<std::sync::OnceLock<SocketAddr>>,
			requests: Arc<AtomicUsize>,
		}

		impl RssTestServerState
		{
			async fn feed_get(State(state): State<Self>, headers: HeaderMap) -> Response<Body>
			{
				state.requests.fetch_add(1, Ordering::Relaxed);
				if (headers.get(IF_NONE_MATCH).and_then(|value| value.to_str().ok()) == Some("\"v1\""))
				{
					let mut response = Response::new(Body::empty());
					*response.status_mut() = StatusCode::NOT_MODIFIED;
					response.headers_mut().insert(ETAG, HeaderValue::from_static("\"v1\""));
					return response;
				}
				let mut response = Response::new(Body::from("<rss version=\"2.0\"><channel><title>test</title></channel></rss>"));
				response.headers_mut().insert(ETAG, HeaderValue::from_static("\"v1\""));
				response.headers_mut().insert(LAST_MODIFIED, HeaderValue::from_static("Sat, 08 Aug 2026 12:00:00 GMT"));
				return response;
			}

			async fn large_get(State(state): State<Self>) -> Response<Body>
			{
				state.requests.fetch_add(1, Ordering::Relaxed);
				return Response::new(Body::from(vec![b'a'; RssLimits::BODY_MAXIMUM_BYTES + 1]));
			}

			async fn privateRedirect_get(State(state): State<Self>) -> Response<Body>
			{
				state.requests.fetch_add(1, Ordering::Relaxed);
				let address = state.address.get().unwrap();
				let mut response = Response::new(Body::empty());
				*response.status_mut() = StatusCode::FOUND;
				response.headers_mut().insert(
					LOCATION,
					HeaderValue::from_str(&format!("http://127.0.0.1:{}/feed", address.port())).unwrap(),
				);
				return response;
			}
		}

		struct RssTestServer
		{
			address: SocketAddr,
			handle: JoinHandle<()>,
			requests: Arc<AtomicUsize>,
		}

		impl RssTestServer
		{
			async fn new() -> Self
			{
				let requests = Arc::new(AtomicUsize::new(0));
				let sharedAddress = Arc::new(std::sync::OnceLock::new());
				let state = RssTestServerState { address: sharedAddress.clone(), requests: requests.clone() };
				let router = Router::new()
					.route("/feed", get(RssTestServerState::feed_get))
					.route("/large", get(RssTestServerState::large_get))
					.route("/private-redirect", get(RssTestServerState::privateRedirect_get))
					.with_state(state);
				let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
				let address = listener.local_addr().unwrap();
				sharedAddress.set(address).unwrap();
				let handle = tokio::spawn(async move {
					axum::serve(listener, router).await.unwrap();
				});
				return Self { address, handle, requests };
			}

			fn destination_get(&self, path: &str) -> ValidatedHttpDestination
			{
				let url = Url::parse(&format!("http://rss.test:{}{}", self.address.port(), path)).unwrap();
				return ValidatedHttpDestination::test_get(url, vec![self.address]);
			}
		}

		impl Drop for RssTestServer
		{
			fn drop(&mut self)
			{
				self.handle.abort();
			}
		}

		struct RssTestCache
		{
			cache: ProxyCache,
			root: PathBuf,
		}

		impl RssTestCache
		{
			fn new() -> Self
			{
				let root = std::env::temp_dir().join(format!("webhome-rss-test-{}", uuid::Uuid::new_v4()));
				let cache = ProxyCache::test_get(root.join("wget")).unwrap();
				return Self { cache, root };
			}
		}

		impl Drop for RssTestCache
		{
			fn drop(&mut self)
			{
				let _ = std::fs::remove_dir_all(&self.root);
			}
		}

		#[test]
		fn cacheMetadata_freshnessUsesServerValidationTime()
		{
			let metadata = RssCacheMetadata { validatedAt: 1_000, ..Default::default() };
			assert!(metadata.fresh_is(1_000 + RssLimits::CACHE_TTL_MILLISECONDS));
			assert!(!metadata.fresh_is(999));
			assert!(!metadata.fresh_is(1_001 + RssLimits::CACHE_TTL_MILLISECONDS));
		}

		#[test]
		fn cacheRecord_notModifiedDependsOnContentVersion()
		{
			let record = RssCacheRecord {
				content: "rss".to_string(),
				metadata: RssCacheMetadata { contentVersion: 42, ..Default::default() },
			};
			assert!(matches!(record.clientResult_get(Some(42)), RssProxyResult::NotModified));
			assert!(matches!(
				record.clientResult_get(Some(41)),
				RssProxyResult::Content { version: 42, .. }
			));
		}

		#[test]
		fn responseBody_refusesChunkBeyondMaximum()
		{
			let mut body = BoundedResponseBody { content: vec![0; RssLimits::BODY_MAXIMUM_BYTES] };
			assert_eq!(body.chunk_add(&[1]).unwrap_err(), proxys_return::RESPONSE_TOO_LARGE);
		}

		#[test]
		fn cacheRecord_serializationPreservesValidators()
		{
			let record = RssCacheRecord {
				content: "rss".to_string(),
				metadata: RssCacheMetadata {
					contentVersion: 42,
					etag: Some("\"version\"".to_string()),
					lastModified: Some("Sat, 08 Aug 2026 12:00:00 GMT".to_string()),
					validatedAt: 84,
				},
			};
			let raw = serde_json::to_vec(&record).unwrap();
			let restored: RssCacheRecord = serde_json::from_slice(&raw).unwrap();
			assert_eq!(restored.metadata.etag.as_deref(), Some("\"version\""));
			assert_eq!(restored.metadata.lastModified.as_deref(), Some("Sat, 08 Aug 2026 12:00:00 GMT"));
		}

		#[test]
		fn cacheRecord_revalidationKeepsContentVersionAndBody()
		{
			let record = RssCacheRecord {
				content: "cached RSS body".to_string(),
				metadata: RssCacheMetadata {
					contentVersion: 42,
					etag: Some("\"old\"".to_string()),
					lastModified: None,
					validatedAt: 1,
				},
			}.revalidated_get(84, Some("\"new\"".to_string()), None);
			assert_eq!(record.content, "cached RSS body");
			assert_eq!(record.metadata.contentVersion, 42);
			assert_eq!(record.metadata.validatedAt, 84);
			assert_eq!(record.metadata.etag.as_deref(), Some("\"new\""));
		}

		#[test]
		#[ignore = "requires local TCP sockets, which may be disabled by the execution sandbox"]
		fn rssProxy_usesFreshCacheThenRevalidatesWithoutLosingContent()
		{
			let runtime = tokio::runtime::Runtime::new().unwrap();
			runtime.block_on(async {
				let server = RssTestServer::new().await;
				let cache = RssTestCache::new();
				let cacheKey = "feed-key";
				let first = RssProxy::new(
					cache.cache.clone(),
					cacheKey.to_string(),
					server.destination_get("/feed"),
				).content_get(None).await.unwrap();
				let RssProxyResult::Content { content: firstContent, version } = first
				else
				{
					panic!("first RSS fetch did not return content");
				};
				assert_eq!(server.requests.load(Ordering::Relaxed), 1);

				let fresh = RssProxy::new(
					cache.cache.clone(),
					cacheKey.to_string(),
					server.destination_get("/feed"),
				).content_get(Some(version)).await.unwrap();
				assert!(matches!(fresh, RssProxyResult::NotModified));
				assert_eq!(server.requests.load(Ordering::Relaxed), 1);

				let raw = cache.cache.load(cacheKey, RssLimits::CACHE_ENTRY_MAXIMUM_BYTES).unwrap().unwrap();
				let mut staleRecord: RssCacheRecord = serde_json::from_slice(&raw).unwrap();
				staleRecord.metadata.validatedAt = 0;
				cache.cache.save(cacheKey, &serde_json::to_vec(&staleRecord).unwrap()).unwrap();

				let revalidated = RssProxy::new(
					cache.cache.clone(),
					cacheKey.to_string(),
					server.destination_get("/feed"),
				).content_get(Some(version)).await.unwrap();
				assert!(matches!(revalidated, RssProxyResult::NotModified));
				assert_eq!(server.requests.load(Ordering::Relaxed), 2);
				let raw = cache.cache.load(cacheKey, RssLimits::CACHE_ENTRY_MAXIMUM_BYTES).unwrap().unwrap();
				let restored: RssCacheRecord = serde_json::from_slice(&raw).unwrap();
				assert_eq!(restored.content, firstContent);
				assert_eq!(restored.metadata.contentVersion, version);
				assert!(restored.metadata.validatedAt > 0);
			});
		}

		#[test]
		#[ignore = "requires local TCP sockets, which may be disabled by the execution sandbox"]
		fn rssProxy_refusesOversizedBodyBeforeCaching()
		{
			let runtime = tokio::runtime::Runtime::new().unwrap();
			runtime.block_on(async {
				let server = RssTestServer::new().await;
				let cache = RssTestCache::new();
				let result = RssProxy::new(
					cache.cache.clone(),
					"large-key".to_string(),
					server.destination_get("/large"),
				).content_get(None).await;
				assert!(matches!(result, Err(proxys_return::RESPONSE_TOO_LARGE)));
				assert_eq!(cache.cache.load("large-key", RssLimits::CACHE_ENTRY_MAXIMUM_BYTES).unwrap(), None);
			});
		}

		#[test]
		#[ignore = "requires local TCP sockets, which may be disabled by the execution sandbox"]
		fn rssProxy_revalidatesRedirectDestination()
		{
			let runtime = tokio::runtime::Runtime::new().unwrap();
			runtime.block_on(async {
				let server = RssTestServer::new().await;
				let cache = RssTestCache::new();
				let result = RssProxy::new(
					cache.cache.clone(),
					"redirect-key".to_string(),
					server.destination_get("/private-redirect"),
				).content_get(None).await;
				assert!(matches!(result, Err(proxys_return::DESTINATION_FORBIDDEN)));
				assert_eq!(server.requests.load(Ordering::Relaxed), 1);
			});
		}
	}
}
