use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use anyhow::anyhow;
use crate::api::translateBooks::translateBook::TranslateBook;

type TranslateBookSlot = Arc<Mutex<Option<TranslateBook>>>;

pub(super) struct TranslateManager
{
	_datas: Mutex<HashMap<String, TranslateBookSlot>>
}

static SINGLETON: OnceLock<TranslateManager> = OnceLock::new();

impl TranslateManager
{
	fn singleton() -> &'static Self
	{
		SINGLETON.get_or_init(Self::new)
	}

	pub(super) fn getBookContent(lang: String,timestamp: u64) -> anyhow::Result<Option<(String,u64)>>
	{
		let lang = Self::filterLang(lang);
		let book = Self::singleton().book_get(&lang)?;
		if(book.1 > timestamp)
		{
			return Ok(Some(book));
		}
		return Ok(None);
	}

	///// PRIVATE
	fn new() -> Self
	{
		return Self {
			_datas: Default::default(),
		};
	}

	fn book_get(&self, lang: &str) -> anyhow::Result<(String,u64)>
	{
		let slot = self.bookSlot_get(lang)?;
		let mut book = slot.lock().map_err(|_| anyhow!("translation book lock poisoned"))?;
		if book.is_none()
		{
			*book = Some(TranslateBook::load(lang)?);
		}
		let Some(book) = book.as_ref() else {
			return Err(anyhow!("translation book initialization failed"));
		};
		return Ok(book.get());
	}

	fn bookSlot_get(&self, lang: &str) -> anyhow::Result<TranslateBookSlot>
	{
		let mut books = self._datas.lock().map_err(|_| anyhow!("translation cache lock poisoned"))?;
		return Ok(books.entry(lang.to_string())
			.or_insert_with(|| Arc::new(Mutex::new(None)))
			.clone());
	}

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

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn managerKeepsBookImmutableForRelease()
	{
		let manager = TranslateManager::new();
		let lang = "EN".to_string();
		let firstBook = manager.book_get(&lang).unwrap();
		let secondBook = manager.book_get(&lang).unwrap();

		assert_eq!(firstBook, secondBook);
		assert!(firstBook.1 > 0);
	}

	#[test]
	fn managerNormalizesUnsupportedLanguageToEnglish()
	{
		assert_eq!(TranslateManager::filterLang("DE".to_string()), "EN");
		assert_eq!(TranslateManager::filterLang("fr".to_string()), "FR");
	}
}
