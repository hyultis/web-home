use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use fluent::bundle::FluentBundle;
use fluent::{FluentArgs, FluentResource};
use intl_memoizer::concurrent::IntlLangMemoizer;
use leptos::logging::log;
use leptos::prelude::{Read, Resource};
use crate::api::translateBooks::API_translate_getBook;
use crate::front::utils::usersData::UserData;
use crate::HWebTrace;

struct BookHolder
{
	content: FluentBundle<FluentResource, IntlLangMemoizer>,
	timstamp: u64
}

pub struct FluentManager {
	_resources: RwLock<HashMap<String, BookHolder>>
}

static SINGLETON: OnceLock<FluentManager> = OnceLock::new();

impl FluentManager {
	pub fn singleton() -> &'static FluentManager
	{
		return SINGLETON.get_or_init(|| FluentManager::new());
	}

	/// Asynchronously translates a given key into the specified language, utilizing the default (empty) set
	/// of parameters.
	///
	/// # Parameters
	/// - `lang`: A value that can be converted into a `String` representing the target language code
	///   (e.g., "en" for English, "fr" for French).
	/// - `key`: A value that can be converted into a `String` representing the translation key or identifier.
	///
	/// # Returns
	/// A `String` containing the translated message.
	///
	/// # Example
	/// ```rust
	/// let result = instance.translateParamsLess("en", "greeting_key").await;
	/// println!("{}", result);  // Outputs the translated message for "greeting_key" in English
	/// ```
	///
	/// # Notes
	/// This method is a shorthand for calling `translate` with an empty parameter map. If your translation
	/// depends on parameters, consider using the `translate` method directly.
	pub async fn translateParamsLess(&self, lang: impl Into<String>, key: impl Into<String>) -> String
	{
		return self.translate(lang,key,Arc::new(HashMap::new())).await;
	}

	/// Translates a given key into a string based on the specified language using Fluent resources.
	///
	/// # Parameters
	/// - `lang`: A type that can be converted into a `String`, representing the target language code (e.g., "en", "fr").
	/// - `key`: A type that can be converted into a `String`, representing the message identifier or key to be translated.
	/// - `params`: An `Arc<HashMap<String, String>>` containing key-value pairs for dynamic parameter substitution in the translated message.
	///
	/// # Returns
	/// A `String` containing the translated and formatted message. If the language, key, or formatting resources are missing, the `key` will be returned as a fallback.
	///
	/// # Behavior
	/// 1. Validates whether the target language resources are loaded. If not, it dynamically adds the language resource asynchronously.
	/// 2. Attempts to retrieve the corresponding translation bundle for the specified language.
	/// 3. Fetches the message associated with the key from the translation bundle.
	/// 4. Substitutes dynamic parameters within the message, if present.
	/// 5. Formats the message and returns the result.
	///
	/// # Logging and Errors
	/// - Logs an error if the translation bundle for the language does not exist.
	/// - Logs an error if the specified key does not have a corresponding message.
	/// - Logs an error if the message template exists but has no value.
	/// - Logs a vector of errors encountered during the message formatting process.
	///
	/// # Example
	/// ```rust
	/// use std::sync::Arc;
	/// use std::collections::HashMap;
	///
	/// let params = Arc::new(HashMap::from([("name".to_string(), "Alice".to_string())]));
	/// let translated = my_translator.translate("en", "greeting", params).await;
	/// assert_eq!(translated, "Hello, Alice!");
	/// ```
	///
	/// # Note
	/// This function assumes that the necessary language resources are either preloaded or can be dynamically fetched using the `addResource` method.
	pub async fn translate(&self, lang: impl Into<String>, key: impl Into<String>, params: Arc<HashMap<String,String>>) -> String
	{
		let lang = lang.into();
		let key = key.into();
		if(!self._resources.read().unwrap().contains_key(&lang))
		{
			// TODO add a get into timestamp
			self.addResource(&lang,0).await;
		}

		let bindingMap = self._resources.read().unwrap();
		let Some(bundle) = bindingMap.get(&lang) else {
			HWebTrace!("missing book");
			return key;
		};
		let Some(msg) = bundle.content.get_message(key.as_str()) else {
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
			args.set(k, v);
		});

		let result = bundle.content.format_pattern(pattern, Some(&args), &mut errors);

		if(!errors.is_empty())
		{
			HWebTrace!("Error while formatting fluent pattern: {:?}",errors);
		}

		return result.to_string();
	}

	pub fn getAsResource(name: impl Into<String>) -> Resource<String>
	{
		let name = name.into();
		return Resource::new(
			move || {
				let (userData, _) = UserData::cookie_signalGet();
				let mut lang = "EN".to_string();
				if let Some(userDataContent) = &*userData.read()
				{
					lang = userDataContent.lang_get().clone();
				}
				lang
			},
			move |lang| {
				FluentManager::singleton().translateParamsLess(lang, name.clone())
			}
		);
	}

	//////// PRIVATE

	fn new() -> Self {
		Self {
			_resources: Default::default(),
		}
	}

	async fn addResource(&self, lang: &String, timestamp: u64)
	{

		println!("addResource seek book");
		let (content,newtime) = match API_translate_getBook(lang.clone(), timestamp).await
		{
			Ok(data) => {
				match data {
					None => return,
					Some(data) => data,
				}
			}
			Err(err) => {
				log!("err when return API_translate_getBook {}",err);
				return;
			}
		};

		let Ok(flt_res) = FluentResource::try_new(content) else {
			log!("Failed to parse an FTL string.");
			return;
		};

		let mut bindingMap = self._resources.write().unwrap();
		match bindingMap.get_mut(lang)
		{
			Some(bundle) => {
				bundle.content.add_resource_overriding(flt_res);
			},
			None => {
				let Ok(langid) = lang.parse() else {
					log!("failed to parse lang ID");
					return;
				};
				let mut bundle = FluentBundle::new_concurrent(vec![langid]);

				bundle.add_resource_overriding(flt_res);

				bindingMap.insert(lang.clone(), BookHolder {
					content: bundle,
					timstamp: newtime,
				});
			}
		}
	}
}