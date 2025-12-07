use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server;
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

#[derive(Serialize,Deserialize,PartialEq,Debug,Clone)]
pub enum proxys_return {
	NOT_MODIFIED,
	BLANK_URL,
	SERVER_ERROR,
}

impl FromServerFnError for proxys_return {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		proxys_return::SERVER_ERROR
	}
}

/// # API_proxys_wget
///
/// Asynchronous server function is responsible for fetching and caching content from a given URL.
/// This function checks the cache for the URL's content and determines whether it needs to fetch
/// new content or return a "not modified" response based on the `lastUpdate` timestamp provided.
/// It makes use of an internal caching mechanism and fetches data via an HTTP client when required.
///
/// ## Caching Behavior
/// - Uses `ProxyCache` to manage cached content for the specific URL under the "wget" namespace.
/// - The cache is checked and updated with the latest content timestamp or new content when applicable.
#[server]
pub async fn API_proxys_wget(url: String, lastUpdate: Option<u64>) -> Result<(u64,String), proxys_return>
{
	use reqwest::Client;
	use crate::api::proxys::proxy_cache::ProxyCache;
	use crate::global_security::hash;
	use std::time::SystemTime;

	if(url.is_empty())
	{
		return Err(proxys_return::BLANK_URL);
	}

	let urlHash = hash(url.clone());
	let cache = ProxyCache::get("wget")?;
	if let Some(cacheTime) = cache.content_lastUpdate(&urlHash)
	{
		if let Some(clientTime) = lastUpdate
		{
			if clientTime <= cacheTime + 60 * 5 {
				return Err(proxys_return::NOT_MODIFIED);
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
					return Ok((duration.as_secs(), content));
				}
			}
			return Err(proxys_return::NOT_MODIFIED);
		}
		200 => {
			if let Some(content) = response.content
			{
				cache.save(&urlHash, content.clone());
				let duration = SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("[API_proxys_wget] Time went backwards ?!");
				return Ok((duration.as_secs(), content));
			}
		}
		_ => {}
	}

	return Err(proxys_return::SERVER_ERROR);
}

#[cfg(feature = "ssr")]
mod inner
{
	use Htrace::HTrace;
	use reqwest::header::{ETAG, IF_NONE_MATCH, IF_MODIFIED_SINCE, LAST_MODIFIED};
	use reqwest::{Client, Error};
	use crate::api::proxys::wget::proxys_return;

	impl From<std::io::Error> for proxys_return {
		fn from(value: std::io::Error) -> Self {
			HTrace!("[io::error] Error : {}",value.to_string());
			proxys_return::SERVER_ERROR
		}
	}

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
	) -> Result<CachedFetchResult, Error> {
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

	impl From<Error> for proxys_return {
		fn from(value: Error) -> Self {
			HTrace!("[reqwest] Error : {}",value.to_string());
			return proxys_return::SERVER_ERROR;
		}
	}
}