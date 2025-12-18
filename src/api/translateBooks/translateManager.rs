use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use crate::api::translateBooks::translateBook::TranslateBook;

pub struct TranslateManager
{
	_datas: Arc<Mutex<HashMap<String, TranslateBook>>>
}

static SINGLETON: OnceLock<TranslateManager> = OnceLock::new();

impl TranslateManager
{
	pub fn singleton() -> &'static Self
	{
		SINGLETON.get_or_init(||Self {
			_datas: Default::default()
		})
	}

	pub fn getBookContent(lang: String,timestamp: u64) -> anyhow::Result<Option<(String,u64)>>
	{
		let lang = Self::filterLang(lang);
		let mut binding = Self::singleton()._datas.lock().unwrap();
		if let Some(book) = binding.get(&lang)
		{
			if(book.getTime() > timestamp)
			{
				return Ok(Some(book.get()));
			}
			return Ok(None);
		}


		// if a book not existing
		let book = TranslateBook::load(&lang)?;
		binding.insert(lang.clone(),book);
		return Ok(Some(binding.get(&lang).unwrap().get()));
	}

	///// PRIVATE

	fn filterLang(lang: String) -> String
	{
		let lang = lang.to_uppercase();
		let allowed = ["EN","FR"];
		if(allowed.contains(&lang.as_str()))
		{
			return lang;
		}

		return "EN".to_string();
	}


}