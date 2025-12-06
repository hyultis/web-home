use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};

#[derive(Serialize,Deserialize)]
#[derive(PartialEq)]
pub enum proxys_return {
	NOT_MODIFIED,
	BLANKURL,
	UPDATED(u64,String)
}

/// # API_proxys_wget
///
/// Asynchronous server function responsible for fetching and caching content from a given URL.
/// This function checks the cache for the URL's content and determines whether it needs to fetch
/// new content or return a "not modified" response based on the `lastUpdate` timestamp provided.
/// It makes use of an internal caching mechanism and fetches data via an HTTP client when required.
///
/// ## Arguments
///
/// - `url` (`String`): The URL from which to fetch data.
/// - `lastUpdate` (`Option<u64>`): The optional Unix timestamp representing the last known update
///   of the content. If provided, it is used to determine whether cached content is still valid.
///
/// ## Returns
///
/// Returns a `Result`:
/// - On success, returns a `proxys_return` variant:
///   - `proxys_return::UPDATED(u64, String)`: Contains the updated UNIX timestamp and the fetched content.
///   - `proxys_return::NOT_MODIFIED`: Indicates that the content has not been modified since the `lastUpdate`.
/// - On failure, returns a `ServerFnError` indicating what caused the issue.
///
/// ## Flow
/// 1. The function calculates a hashed version of the URL for identification in the cache.
/// 2. Attempts to retrieve the last update timestamp of the URL's content from the cache.
///    - If a valid timestamp exists and is within the allowed timeframe compared to `lastUpdate`,
///      returns `proxys_return::NOT_MODIFIED`.
/// 3. Fetches the URL content using an HTTP client if necessary:
///    - If the server responds with HTTP status `304` (Not Modified), updates the cache timestamp,
///      and attempts to serve cached content if available.
///    - If the server responds with HTTP status `200` (OK), saves the new content in the cache,
///      updates the cache timestamp, and serves the fetched content.
///    - For all other response statuses, errors out.
/// 4. Errors out with a `ServerFnError` on unexpected scenarios, including server errors.
///
/// ## Errors
/// - Returns `ServerFnError` in the following cases:
///   - Cache initialization failure.
///   - Failed content fetch from the server.
///   - Invalid or missing server response.
///   - Encountering a server-side error.
///
/// ## Caching Behavior
/// - Uses `ProxyCache` to manage cached content for the specific URL under the "wget" namespace.
/// - The cache is checked and updated with the latest content timestamp or new content when applicable.
///
/// ## Notes
/// - The cache expiry tolerance is defined as 5 minutes (60 * 5 seconds) since the last update.
///
/// ## Panics
/// - Panics if the system's time moves backwards, as detected during duration calculations.
#[server]
pub async fn API_proxys_wget(url: String, lastUpdate: Option<u64>) -> Result<proxys_return, ServerFnError>
{
	use reqwest::Client;
	use crate::api::proxys::proxy_cache::ProxyCache;
	use crate::global_security::hash;
	use std::time::SystemTime;

	if(url.is_empty())
	{
		return Ok(proxys_return::BLANKURL);
	}

	let urlHash = hash(url.clone());
	let cache = ProxyCache::get("wget").map_err(|err| ServerFnError::new(format!("{:?}",err)))?;
	if let Some(cacheTime) = cache.content_lastUpdate(&urlHash)
	{
		if let Some(clientTime) = lastUpdate
		{
			if clientTime <= cacheTime + 60 * 5 {
				return Ok(proxys_return::NOT_MODIFIED);
			}
		}
	}

	let client = Client::new();
	let response = inner::fetch_rss_with_cache(&url, None,None).await?;

	match response.status {
		304 => {
			cache.content_updateTime(&urlHash,SystemTime::now());
			if(lastUpdate.is_none())
			{
				if let Some(content) = cache.load(&urlHash)
				{
					let duration = SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("[API_proxys_wget] Time went backwards ?!");
					return Ok(proxys_return::UPDATED(duration.as_secs(), content));
				}
			}
			return Ok(proxys_return::NOT_MODIFIED);
		}
		200 => {
			if let Some(content) = response.content
			{
				cache.save(&urlHash, content.clone());
				let duration = SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("[API_proxys_wget] Time went backwards ?!");
				return Ok(proxys_return::UPDATED(duration.as_secs(), content));
			}
		}
		_ => {}
	}

	return Err(ServerFnError::new("SERVER_ERROR"));
}

#[cfg(feature = "ssr")]
mod inner
{
	use reqwest::header::{ETAG, IF_NONE_MATCH, IF_MODIFIED_SINCE, LAST_MODIFIED};
	use reqwest::Client;
	use leptos::prelude::ServerFnError;

	pub struct CachedFetchResult {
		pub status: u16,
		pub content: Option<String>,
		pub etag: Option<String>,
		pub last_modified: Option<String>,
	}

	pub async fn fetch_rss_with_cache(
		url: &str,
		previous_etag: Option<&str>,
		previous_last_modified: Option<&str>,
	) -> Result<CachedFetchResult, ServerFnError> {
		let client = Client::new();

		let mut request = client.get(url);

		// Ajout des headers conditionnels si disponibles
		if let Some(etag) = previous_etag {
			request = request.header(IF_NONE_MATCH, etag);
		}

		if let Some(last_modified) = previous_last_modified {
			request = request.header(IF_MODIFIED_SINCE, last_modified);
		}

		let response = request.send().await?;

		let status = response.status().as_u16();

		// Gestion du HTTP 304 Not Modified
		if status == 304 {
			return Ok(CachedFetchResult {
				status,
				content: None, // on ne renvoie rien, à toi de garder le flux local
				etag: previous_etag.map(|s| s.to_string()),
				last_modified: previous_last_modified.map(|s| s.to_string()),
			});
		}

		// Sinon, c'est un 200 → on récupère contenu + nouveaux headers
		let etag = response
			.headers()
			.get(ETAG)
			.map(|h| h.to_str().unwrap_or("").to_string());

		let last_modified = response
			.headers()
			.get(LAST_MODIFIED)
			.map(|h| h.to_str().unwrap_or("").to_string());

		let content = Some(response.text().await?);

		Ok(CachedFetchResult {
			status,
			content,
			etag,
			last_modified,
		})
	}
}