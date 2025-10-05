use std::sync::OnceLock;
use dashmap::DashMap;
use crate::api::translateBooks::translateBook::TranslateBook;

pub struct TranslateManager
{
	_datas: DashMap<String, TranslateBook>
}

static SINGLETON: OnceLock<TranslateManager> = OnceLock::new();

impl TranslateManager
{
	pub fn singleton() -> &'static Self
	{
		SINGLETON.get_or_init(||Self {
			_datas: DashMap::new()
		})
	}

	pub fn getBookContent(lang: String,timestamp: u64) -> anyhow::Result<Option<(String,u64)>>
	{
		let lang = Self::filterLang(lang);
		if let Some(book) = Self::singleton()._datas.get(&lang)
		{
			if(book.getTime() > timestamp)
			{
				return Ok(Some(book.value().get()));
			}
			return Ok(None);
		}


		// if book not existing
		let book = TranslateBook::load(&lang)?;
		Self::singleton()._datas.insert(lang.clone(),book);
		return Ok(Some(Self::singleton()._datas.get(&lang).unwrap().get()));
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