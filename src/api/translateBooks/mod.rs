#[cfg(feature = "ssr")]
pub mod translateBook;
#[cfg(feature = "ssr")]
pub mod translateManager;

use leptos::server;
use leptos::server_fn::ServerFnError;

#[cfg(feature = "ssr")]
use translateManager::TranslateManager;

#[server]
pub async fn API_translate_getBook(lang: String, oldtime: u64) -> Result<Option<(String,u64)>, ServerFnError>
{
	use Htrace::HTrace;
	use Htrace::components::level::Level;

	return TranslateManager::getBookContent(lang,oldtime)
		.map(|content|content)
		.map_err(|err| {
			HTrace!((Level::ERROR) "translation err: {}",err);
			ServerFnError::Response(err.to_string())
		});
}