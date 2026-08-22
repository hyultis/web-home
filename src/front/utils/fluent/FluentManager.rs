use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use async_lock::{Mutex, RwLock};
use fluent::bundle::FluentBundle;
use fluent::{FluentArgs, FluentResource};
use intl_memoizer::concurrent::IntlLangMemoizer;
use leptos::logging::log;
use leptos::prelude::Resource;
use crate::api::translateBooks::API_translate_getBook;
use crate::front::utils::users_data::ClientState;
use crate::HWebTrace;

struct BookHolder
{
	content: FluentBundle<FluentResource, IntlLangMemoizer>
}

type BookSlot = Arc<Mutex<Option<Arc<BookHolder>>>>;

#[derive(Clone, Copy)]
enum TranslateOutput
{
	Text,
	Html,
}

pub struct FluentManager {
	_resources: RwLock<HashMap<String, BookSlot>>
}

static SINGLETON: OnceLock<FluentManager> = OnceLock::new();

impl FluentManager {
	pub fn singleton() -> &'static FluentManager
	{
		return SINGLETON.get_or_init(|| FluentManager::new());
	}

	/// Same as translate() without the params
	pub async fn translateParamsLess(&self, lang: impl ToString, key: impl ToString) -> String
	{
		return self.translate(lang,key,Arc::new(HashMap::new())).await;
	}

	/// Translates a given key into a string based on the specified language using Fluent resources.
	///
	/// # Parameters
	/// - `lang`: A type that can be converted into a `String`, representing the target language code (e.g., "en", "fr").
	/// - `key`: A type that can be converted into a `String`, representing the message identifier or key to be translated.
	/// - `params`: An `Arc<HashMap<String, String>>` containing key-value pairs for dynamic parameter substitution in the translated message.
	pub async fn translate(&self, lang: impl ToString, key: impl ToString, params: Arc<HashMap<String,String>>) -> String
	{
		return self.translateFor(lang,key,params,TranslateOutput::Text).await;
	}

	#[cfg(all(test, feature="ssr"))]
	async fn translateHtml(&self, lang: impl ToString, key: impl ToString, params: Arc<HashMap<String,String>>) -> String
	{
		return self.translateFor(lang,key,params,TranslateOutput::Html).await;
	}

	async fn translateFor(&self, lang: impl ToString, key: impl ToString, params: Arc<HashMap<String,String>>, output: TranslateOutput) -> String
	{
		let lang = lang.to_string();
		let key = key.to_string();
		let Some(book) = self.book_get(&lang).await else {
			HWebTrace!("missing book {}",lang);
			return key;
		};
		let Some(msg) = book.content.get_message(key.as_str()) else {
			HWebTrace!("missing message for key {}",key);
			return key;
		};
		let Some(pattern) = msg.value() else {
			HWebTrace!("missing pattern for key {}",key);
			return key;
		};
		let mut errors = vec![];

		let mut args = FluentArgs::new();
		params.iter().for_each(|(k,v)| {
			match output
			{
				TranslateOutput::Text => args.set(k, v),
				TranslateOutput::Html => args.set(k, Self::htmlText_escape(v)),
			}
		});

		let result = book.content.format_pattern(pattern, Some(&args), &mut errors);

		if(!errors.is_empty())
		{
			HWebTrace!("Error while formatting fluent pattern: {:?}",errors);
		}

		return result.to_string();
	}

	/// Creates a `Resource<String>` which provides translations for a given string based on the user's language preference.
	///
	/// This function takes a name of a string (such as a key for a translation) and returns a
	/// `Resource` that resolves the current language of the user and provides the translated string.
	///
	/// # Parameters
	///
	/// - `name`: A value that implements `Into<String>`. Represents the key or identifier for the
	///   string to be translated.
	pub fn getAsResource(name: impl Fn() -> String + Send + Sync + Clone + 'static, params: HashMap<String,String>) -> Resource<String>
	{
		return Self::getAsResourceFor(name,params,TranslateOutput::Text);
	}

	pub(crate) fn getAsHtmlResource(name: impl Fn() -> String + Send + Sync + Clone + 'static, params: HashMap<String,String>) -> Resource<String>
	{
		return Self::getAsResourceFor(name,params,TranslateOutput::Html);
	}

	fn getAsResourceFor(name: impl Fn() -> String + Send + Sync + Clone + 'static, params: HashMap<String,String>, output: TranslateOutput) -> Resource<String>
	{
		let params = Arc::new(params);
		return Resource::new(
			move || {
				return ClientState::expect().lang_get();
			},
			move |lang| {
				FluentManager::singleton().translateFor(lang, name.clone()(), params.clone(), output)
			}
		);
	}

	pub fn getAsResourceParamsLess(name: impl Into<String>) -> Resource<String>
	{
		let name = name.into();
		Self::getAsResource(move || name.clone(),HashMap::new())
	}

	//////// PRIVATE

	fn new() -> Self {
		Self {
			_resources: Default::default(),
		}
	}

	async fn book_get(&self, lang: &str) -> Option<Arc<BookHolder>>
	{
		self.book_getWith(lang, Self::addResource(lang,0)).await
	}

	async fn book_getWith<F>(&self, lang: &str, loader: F) -> Option<Arc<BookHolder>>
	where
		F: Future<Output = Option<(String,u64)>>
	{
		let slot = self.bookSlot_get(lang).await;
		let mut book = slot.lock().await;
		if let Some(book) = book.as_ref()
		{
			return Some(book.clone());
		}

		let resource = loader.await?;
		let loadedBook = Arc::new(Self::book_build(lang, resource)?);
		*book = Some(loadedBook.clone());
		return Some(loadedBook);
	}

	async fn bookSlot_get(&self, lang: &str) -> BookSlot
	{
		{
			let resources = self._resources.read().await;
			if let Some(slot) = resources.get(lang)
			{
				return slot.clone();
			}
		}

		let mut resources = self._resources.write().await;
		return resources.entry(lang.to_string())
			.or_insert_with(|| Arc::new(Mutex::new(None)))
			.clone();
	}

	async fn addResource(lang: &str, timestamp: u64) -> Option<(String,u64)>
	{
		return match API_translate_getBook(lang.to_string(), timestamp).await
		{
			Ok(data) => {
				match data {
					None => None,
					Some(data) => Some(data),
				}
			}
			Err(err) => {
				log!("err when return API_translate_getBook {}",err);
				return None;
			}
		};
	}

	fn book_build(lang: &str, (content,_timestamp): (String,u64)) -> Option<BookHolder>
	{
		let Ok(flt_res) = FluentResource::try_new(content) else {
			log!("Failed to parse an FTL string.");
			return None;
		};

		let Ok(langid) = lang.parse() else {
			log!("failed to parse lang ID");
			return None;
		};
		let mut bundle = FluentBundle::new_concurrent(vec![langid]);
		bundle.add_resource_overriding(flt_res);

		return Some(BookHolder {
			content: bundle,
		});
	}

	fn htmlText_escape(value: &str) -> String
	{
		let mut escaped = String::with_capacity(value.len());
		for character in value.chars()
		{
			match character
			{
				'&' => escaped.push_str("&amp;"),
				'<' => escaped.push_str("&lt;"),
				'>' => escaped.push_str("&gt;"),
				'\"' => escaped.push_str("&quot;"),
				'\'' => escaped.push_str("&#39;"),
				_ => escaped.push(character),
			}
		}
		return escaped;
	}
}

#[cfg(all(test, feature="ssr"))]
mod tests
{
	use super::*;
	use regex::Regex;
	use std::collections::{BTreeMap, BTreeSet};
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::time::Duration;

	const EN_BOOK: &str = include_str!("../../../../static/translates/EN/main.flt");
	const FR_BOOK: &str = include_str!("../../../../static/translates/FR/main.flt");

	fn messages_get(source: &str) -> BTreeMap<String,String>
	{
		let mut messages = BTreeMap::new();
		let mut currentKey: Option<String> = None;
		for line in source.lines()
		{
			if !line.starts_with(' ') && !line.starts_with('\t')
			{
				currentKey = line.split_once('=')
					.map(|(key,value)| (key.trim(),value.trim_start()))
					.filter(|(key,_)| !key.is_empty() && !key.starts_with('-') && !key.starts_with('#'))
					.map(|(key,value)| {
						messages.insert(key.to_string(),value.to_string());
						return key.to_string();
					});
			}
			else if let Some(key) = currentKey.as_ref()
			{
				if let Some(value) = messages.get_mut(key)
				{
					value.push('\n');
					value.push_str(line.trim());
				}
			}
		}
		return messages;
	}

	fn bookContent_get(value: &str) -> (String,u64)
	{
		return (format!("message = {value}"), 1);
	}

	#[tokio::test]
	async fn bookGet_sameLanguageUsesSingleFlight()
	{
		let manager = FluentManager::new();
		let fetchCount = AtomicUsize::new(0);
		let firstLoader = async {
			fetchCount.fetch_add(1, Ordering::SeqCst);
			tokio::task::yield_now().await;
			Some(bookContent_get("Hello"))
		};
		let secondLoader = async {
			fetchCount.fetch_add(1, Ordering::SeqCst);
			Some(bookContent_get("Ignored"))
		};

		let (firstBook,secondBook) = tokio::join!(
			manager.book_getWith("EN", firstLoader),
			manager.book_getWith("EN", secondLoader),
		);

		assert_eq!(fetchCount.load(Ordering::SeqCst), 1);
		assert!(Arc::ptr_eq(&firstBook.unwrap(), &secondBook.unwrap()));
	}

	#[tokio::test]
	async fn bookGet_differentLanguagesLoadConcurrently()
	{
		let manager = FluentManager::new();
		let started = AtomicUsize::new(0);
		let englishLoader = async {
			started.fetch_add(1, Ordering::SeqCst);
			while started.load(Ordering::SeqCst) < 2
			{
				tokio::task::yield_now().await;
			}
			Some(bookContent_get("Hello"))
		};
		let frenchLoader = async {
			started.fetch_add(1, Ordering::SeqCst);
			while started.load(Ordering::SeqCst) < 2
			{
				tokio::task::yield_now().await;
			}
			Some(bookContent_get("Bonjour"))
		};

		let result = tokio::time::timeout(Duration::from_secs(1), async {
			tokio::join!(
				manager.book_getWith("EN", englishLoader),
				manager.book_getWith("FR", frenchLoader),
			)
		}).await;

		assert!(result.is_ok());
		assert_eq!(started.load(Ordering::SeqCst), 2);
	}

	#[tokio::test]
	async fn bookGet_failedLoadCanRetry()
	{
		let manager = FluentManager::new();
		let missingBook = manager.book_getWith("EN", async { None }).await;
		let loadedBook = manager.book_getWith("EN", async {
			Some(bookContent_get("Hello"))
		}).await;

		assert!(missingBook.is_none());
		assert!(loadedBook.is_some());
	}

	#[tokio::test]
	async fn translateHtml_keepsBookMarkupAndEscapesDynamicParams()
	{
		let manager = FluentManager::new();
		manager.book_getWith("EN", async {
			Some(("message = <strong>{ $value }</strong>".to_string(), 1))
		}).await.unwrap();
		let params = Arc::new(HashMap::from([
			("value".to_string(), "<img src=x onerror='alert(1)'>&".to_string()),
		]));

		let htmlResult = manager.translateHtml("EN", "message", params.clone()).await;
		let textResult = manager.translate("EN", "message", params).await;

		assert!(htmlResult.starts_with("<strong>"));
		assert!(htmlResult.contains("&lt;img src=x onerror=&#39;alert(1)&#39;&gt;&amp;"));
		assert!(!htmlResult.contains("<img"));
		assert!(textResult.contains("<img src=x onerror='alert(1)'>&"));
	}

	#[tokio::test]
	async fn translate_missingMessageReturnsItsKey()
	{
		let manager = FluentManager::new();
		manager.book_getWith("EN", async {
			Some(bookContent_get("Hello"))
		}).await.unwrap();

		let result = manager.translate("EN", "missing_message", Arc::new(HashMap::new())).await;

		assert_eq!(result,"missing_message");
	}

	#[test]
	fn fluentBooks_parseAndHaveMatchingKeysAndParams()
	{
		assert!(FluentResource::try_new(EN_BOOK.to_string()).is_ok());
		assert!(FluentResource::try_new(FR_BOOK.to_string()).is_ok());
		let englishMessages = messages_get(EN_BOOK);
		let frenchMessages = messages_get(FR_BOOK);
		assert_eq!(
			englishMessages.keys().collect::<Vec<_>>(),
			frenchMessages.keys().collect::<Vec<_>>(),
		);

		let variableRegex = Regex::new(r"\$([A-Za-z][A-Za-z0-9_-]*)").unwrap();
		for (key,englishValue) in &englishMessages
		{
			let englishParams = variableRegex.captures_iter(englishValue)
				.map(|capture| capture[1].to_string())
				.collect::<BTreeSet<_>>();
			let frenchParams = variableRegex.captures_iter(&frenchMessages[key])
				.map(|capture| capture[1].to_string())
				.collect::<BTreeSet<_>>();
			assert_eq!(englishParams,frenchParams,"parameter mismatch for {key}");
		}
	}

	#[test]
	fn fluentBookMarkup_isExplicitAndDoesNotPutParamsInsideTags()
	{
		let englishMessages = messages_get(EN_BOOK);
		let frenchMessages = messages_get(FR_BOOK);
		let expectedMarkupKeys = BTreeSet::from(["pageRoot_foot".to_string()]);
		let literalAngleTextKeys = BTreeSet::from(["MODULE_MAIL_NO_SUBJECT".to_string()]);
		let paramInsideTag = Regex::new(r"(?is)<[^>]*\{\s*\$[^}]+\}[^>]*>").unwrap();
		let dangerousMarkup = Regex::new(r#"(?is)<\s*(script|iframe|object|embed|style|link|meta)\b|\son[a-z0-9_-]+\s*=|javascript\s*:|data\s*:"#).unwrap();

		for messages in [&englishMessages,&frenchMessages]
		{
			let markupKeys = messages.iter()
				.filter(|(_,value)| value.contains('<') && value.contains('>'))
				.filter(|(key,_)| !literalAngleTextKeys.contains(*key))
				.map(|(key,_)| key.clone())
				.collect::<BTreeSet<_>>();
			assert_eq!(markupKeys,expectedMarkupKeys);
			for key in &literalAngleTextKeys
			{
				assert!(messages.contains_key(key),"missing literal angle-bracket text key {key}");
			}
			for (key,value) in messages
			{
				assert!(!paramInsideTag.is_match(value),"parameter used inside HTML tag for {key}");
				if value.contains('<') && value.contains('>')
				{
					assert!(!dangerousMarkup.is_match(value),"dangerous HTML in translation {key}");
				}
			}
		}
	}

	#[test]
	fn auditedHardcodedUiStrings_useFluentKeys()
	{
		let linkSource = include_str!("../../modules/link.rs");
		let homeSource = include_str!("../../pages/home.rs");
		let inscriptionSource = include_str!("../../pages/inscription.rs");
		let mailSource = include_str!("../../modules/mail.rs");
		let dialogSource = include_str!("../dialog.rs");

		for removedLiteral in [">Label<", ">Url<", "placeholder=\"Label\"", "placeholder=\"Url\"", ">add<"]
		{
			assert!(!linkSource.contains(removedLiteral));
		}
		assert!(!homeSource.contains("<span>Type</span>"));
		assert!(!inscriptionSource.contains("\"retour\""));
		assert!(mailSource.contains("<TranslateText key=\"MODULE_MAIL_NO_SUBJECT\"/>"));
		assert!(!dialogSource.contains("<Translate key={data.title}/>"));
		assert!(dialogSource.contains("<TranslateText key=title/>"));
	}
}
