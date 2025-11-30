use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};

#[derive(Serialize,Deserialize)]
pub enum proxys_return {
	NOT_MODIFIED,
	UPDATED(String)
}

#[server]
pub async fn API_proxys_wget(url: String) -> Result<proxys_return, ServerFnError>
{
	// TODO : add cache
	use reqwest::Client;

	let client = Client::new();
	let response = inner::fetch_rss_with_cache(&url, None,None).await?;
	let content = response.content.unwrap_or("".to_string());

	match response.status {
		304 => {
			return Ok(proxys_return::NOT_MODIFIED);
		}
		200 => {
			return Ok(proxys_return::UPDATED(content));
		}
		_ => {}
	}

	return Err(ServerFnError::new("SERVEUR_ERROR"));
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