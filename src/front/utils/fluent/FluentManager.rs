use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use fluent::bundle::FluentBundle;
use fluent::{FluentArgs, FluentResource};
use intl_memoizer::concurrent::IntlLangMemoizer;
use leptos::logging::log;
use leptos::prelude::{Read, Resource};
use crate::api::translateBooks::API_translate_getBook;
use crate::front::utils::users_data::UserData;
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

	/// Same as translate() without the params
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
			HWebTrace!("missing book {}",lang);
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
		let params = Arc::new(params);
		return Resource::new(
			move || {
				let (userData, _) = UserData::cookie_signalGet();
				let mut lang = "EN".to_string();
				if let Some(userDataContent) = userData.try_read()
				{
					if let Some(userDataContent) = &*userDataContent
					{
						lang = userDataContent.lang_get().clone();
					}
				}
				lang
			},
			move |lang| {
				FluentManager::singleton().translate(lang, name.clone()(), params.clone())
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

	async fn addResource(&self, lang: &String, timestamp: u64)
	{

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