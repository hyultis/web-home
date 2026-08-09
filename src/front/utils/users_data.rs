use aes_gcm::aead::common::Generate;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Config;
use base64ct::{Base64, Encoding};
use leptos::prelude::codee::string::JsonSerdeCodec;
use leptos::prelude::{expect_context, Get, GetUntracked, RwSignal, Set, Signal, WriteSignal};
use leptos_use::{use_cookie_with_options, SameSite, UseCookieOptions};
use serde::{Deserialize, Serialize};

use crate::api::login::{API_user_login, API_user_logout, API_user_salt, API_user_sign};
use crate::front::utils::all_front_enum::AllFrontLoginEnum;
use crate::global_security::{generate_salt_raw, hash};

const COOKIE_MAX_AGE: i64 = 24 * 3600 * 1000;
const CLIENT_CRYPTO_STORAGE_KEY: &str = "webhome-crypto";
const LEGACY_CRYPTO_COOKIE_NAME: &str = "webhome-crypto";
const LEGACY_CRYPTO_COOKIE_PATH: &str = "/home";
const USER_PREFERENCES_COOKIE_NAME: &str = "webhome-preferences";
const LEGACY_COOKIE_NAME: &str = "webhome";
const ROOT_COOKIE_PATH: &str = "/";

#[derive(Serialize, Deserialize, Clone)]
struct ClientCiphertext
{
	salt: String,
	nonce: String,
	content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientCryptoError
{
	RANDOM_GENERATION,
	KEY_DERIVATION,
	INVALID_ENVELOPE,
	INVALID_BASE64,
	INVALID_KEY,
	INVALID_NONCE,
	ENCRYPTION,
	DECRYPTION,
	INVALID_UTF8,
	SERIALIZATION,
	STORAGE_UNAVAILABLE,
	STORAGE_READ,
	STORAGE_WRITE,
	LEGACY_COOKIE_WRITE,
}

/// Browser context used to derive the AES key for persisted modules.
///
/// This type is serialized only into the origin-scoped `webhome-crypto`
/// local-storage entry. It must never be added to a cookie, URL, log or
/// server-function contract.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ClientCryptoContext
{
	userSalt: String,
}

impl std::fmt::Debug for ClientCryptoContext
{
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("ClientCryptoContext")
			.field("userSalt", &"[REDACTED]")
			.finish();
	}
}

impl ClientCryptoContext
{
	pub(crate) async fn login_get(login: String, pwd: String) -> Result<Self, AllFrontLoginEnum>
	{
		let generatedId = hash(login);
		let serverUserSalt = API_user_salt(generatedId.clone()).await
			.map_err(|_| AllFrontLoginEnum::SALT_INVALID)?;
		let userSalt = Self::pwdHash(pwd, serverUserSalt);

		API_user_login(generatedId.clone(), hash(userSalt.clone())).await
			.map_err(AllFrontLoginEnum::fromLoginStatus)?;

		return Ok(Self {
			userSalt,
		});
	}

	pub(crate) async fn signUp(login: String, pwd: String) -> Result<(), AllFrontLoginEnum>
	{
		if (!Self::signPassword_isValid(&pwd))
		{
			return Err(AllFrontLoginEnum::SIGN_PASSWORD_TOO_SHORT);
		}

		let generatedId = hash(login);
		let serverUserSalt = API_user_salt(generatedId.clone()).await
			.map_err(|_| AllFrontLoginEnum::SALT_INVALID)?;
		let userSalt = Self::pwdHash(pwd, serverUserSalt);

		return API_user_sign(generatedId, hash(userSalt)).await
			.map_err(AllFrontLoginEnum::fromLoginStatus);
	}

	pub(crate) async fn logout() -> Option<AllFrontLoginEnum>
	{
		return API_user_logout().await.err().map(AllFrontLoginEnum::fromLoginStatus);
	}

	pub(crate) fn encrypt(&self, plaintext: &str) -> Result<String, ClientCryptoError>
	{
		let salt = generate_salt_raw().map_err(|_| ClientCryptoError::RANDOM_GENERATION)?;
		let keyBytes = Self::derive_key_from_password(&self.userSalt, &salt)?;
		let key = Key::<Aes256Gcm>::try_from(keyBytes.as_slice()).map_err(|_| ClientCryptoError::INVALID_KEY)?;
		let cipher = Aes256Gcm::new(&key);
		let nonce = Nonce::generate();
		let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes()).map_err(|_| ClientCryptoError::ENCRYPTION)?;
		let envelope = ClientCiphertext {
			salt: Base64::encode_string(&salt),
			nonce: Base64::encode_string(nonce.as_slice()),
			content: Base64::encode_string(&ciphertext),
		};

		return serde_json::to_string(&envelope).map_err(|_| ClientCryptoError::SERIALIZATION);
	}

	pub(crate) fn decrypt(&self, content: &str) -> Result<String, ClientCryptoError>
	{
		let envelope: ClientCiphertext = serde_json::from_str(content).map_err(|_| ClientCryptoError::INVALID_ENVELOPE)?;
		let salt = Base64::decode_vec(&envelope.salt).map_err(|_| ClientCryptoError::INVALID_BASE64)?;
		let nonceBytes = Base64::decode_vec(&envelope.nonce).map_err(|_| ClientCryptoError::INVALID_BASE64)?;
		let ciphertext = Base64::decode_vec(&envelope.content).map_err(|_| ClientCryptoError::INVALID_BASE64)?;
		let keyBytes = Self::derive_key_from_password(&self.userSalt, &salt)?;
		let key = Key::<Aes256Gcm>::try_from(keyBytes.as_slice()).map_err(|_| ClientCryptoError::INVALID_KEY)?;
		let nonce = Nonce::try_from(nonceBytes.as_slice()).map_err(|_| ClientCryptoError::INVALID_NONCE)?;
		let cipher = Aes256Gcm::new(&key);
		let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref()).map_err(|_| ClientCryptoError::DECRYPTION)?;

		return String::from_utf8(plaintext).map_err(|_| ClientCryptoError::INVALID_UTF8);
	}

	fn derive_key_from_password(password: &str, salt: &[u8]) -> Result<[u8; 32], ClientCryptoError>
	{
		let derived = argon2::hash_raw(password.as_bytes(), salt, &Config::default())
			.map_err(|_| ClientCryptoError::KEY_DERIVATION)?;
		let keySlice = derived.get(..32).ok_or(ClientCryptoError::KEY_DERIVATION)?;
		let mut key = [0u8; 32];
		key.copy_from_slice(keySlice);
		return Ok(key);
	}

	fn pwdHash(value: String, salt: String) -> String
	{
		return hash(format!("{}{}", salt, value));
	}

	fn signPassword_isValid(password: &str) -> bool
	{
		return password.chars().count() >= 12;
	}

	fn legacyCookie_signalGet() -> (Signal<Option<Self>>, WriteSignal<Option<Self>>)
	{
		return use_cookie_with_options::<Self, JsonSerdeCodec>(LEGACY_CRYPTO_COOKIE_NAME, UseCookieOptions::default()
			.max_age(COOKIE_MAX_AGE)
			.same_site(SameSite::Strict)
			.secure(true)
			.path(LEGACY_CRYPTO_COOKIE_PATH));
	}

	#[cfg(test)]
	pub(crate) fn test_get() -> Self
	{
		return Self {
			userSalt: "test-client-secret".to_string(),
		};
	}
}

#[derive(Clone)]
struct ClientCryptoStorage
{
	value: RwSignal<Option<ClientCryptoContext>>,
	error: RwSignal<Option<ClientCryptoError>>,
}

impl ClientCryptoStorage
{
	fn new() -> Self
	{
		let (value, error) = match Self::browser_read()
		{
			Ok(value) => (value, None),
			Err(error) => (None, Some(error)),
		};
		return Self {
			value: RwSignal::new(value),
			error: RwSignal::new(error),
		};
	}

	fn get(&self) -> Option<ClientCryptoContext>
	{
		return self.value.get_untracked();
	}

	fn isAvailable(&self) -> bool
	{
		return self.value.get().is_some();
	}

	fn error_get(&self) -> Option<ClientCryptoError>
	{
		return self.error.get_untracked();
	}

	fn set(&self, crypto: ClientCryptoContext) -> Result<(), ClientCryptoError>
	{
		if let Err(error) = Self::browser_write(&crypto)
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.value.set(Some(crypto));
		self.error.set(None);
		return Ok(());
	}

	fn clear(&self) -> Result<(), ClientCryptoError>
	{
		self.value.set(None);
		if let Err(error) = Self::browser_remove()
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.error.set(None);
		return Ok(());
	}

	#[cfg(feature = "ssr")]
	fn browser_read() -> Result<Option<ClientCryptoContext>, ClientCryptoError>
	{
		return Ok(None);
	}

	#[cfg(not(feature = "ssr"))]
	fn browser_read() -> Result<Option<ClientCryptoContext>, ClientCryptoError>
	{
		let storage = web_sys::window()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?
			.local_storage()
			.map_err(|_| ClientCryptoError::STORAGE_READ)?
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?;
		let Some(serialized) = storage.get_item(CLIENT_CRYPTO_STORAGE_KEY)
			.map_err(|_| ClientCryptoError::STORAGE_READ)?
		else {return Ok(None)};
		return serde_json::from_str(&serialized)
			.map(Some)
			.map_err(|_| ClientCryptoError::INVALID_ENVELOPE);
	}

	#[cfg(feature = "ssr")]
	fn browser_write(_crypto: &ClientCryptoContext) -> Result<(), ClientCryptoError>
	{
		return Ok(());
	}

	#[cfg(not(feature = "ssr"))]
	fn browser_write(crypto: &ClientCryptoContext) -> Result<(), ClientCryptoError>
	{
		let serialized = serde_json::to_string(crypto).map_err(|_| ClientCryptoError::SERIALIZATION)?;
		let storage = web_sys::window()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?
			.local_storage()
			.map_err(|_| ClientCryptoError::STORAGE_WRITE)?
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?;
		return storage.set_item(CLIENT_CRYPTO_STORAGE_KEY, &serialized)
			.map_err(|_| ClientCryptoError::STORAGE_WRITE);
	}

	#[cfg(feature = "ssr")]
	fn browser_remove() -> Result<(), ClientCryptoError>
	{
		return Ok(());
	}

	#[cfg(not(feature = "ssr"))]
	fn browser_remove() -> Result<(), ClientCryptoError>
	{
		let storage = web_sys::window()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?
			.local_storage()
			.map_err(|_| ClientCryptoError::STORAGE_WRITE)?
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?;
		return storage.remove_item(CLIENT_CRYPTO_STORAGE_KEY)
			.map_err(|_| ClientCryptoError::STORAGE_WRITE);
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct UserPreferences
{
	lang: String,
	connected: bool,
	updateVal: u128,
}

impl UserPreferences
{
	fn new(lang: impl Into<String>) -> Self
	{
		let mut preferences = Self::default();
		preferences.lang_set(lang);
		return preferences;
	}

	fn lang_set(&mut self, lang: impl Into<String>)
	{
		let lang = lang.into();
		self.lang = lang.split('-').next().unwrap_or("EN").to_uppercase();
	}

	fn valUpdate(&mut self)
	{
		self.updateVal += 1;
	}

	fn cookie_signalGet() -> (Signal<Option<Self>>, WriteSignal<Option<Self>>)
	{
		return use_cookie_with_options::<Self, JsonSerdeCodec>(USER_PREFERENCES_COOKIE_NAME, UseCookieOptions::default()
			.max_age(COOKIE_MAX_AGE)
			.same_site(SameSite::Strict)
			.secure(true)
			.path(ROOT_COOKIE_PATH));
	}
}

impl Default for UserPreferences
{
	fn default() -> Self
	{
		return Self {
			lang: "EN".to_string(),
			connected: false,
			updateVal: 0,
		};
	}
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct LegacyUserData
{
	#[serde(default = "LegacyUserData::lang_default")]
	lang: String,
	#[serde(default)]
	userSalt: Option<String>,
	#[serde(default)]
	generatedId: Option<String>,
	#[serde(default)]
	updateVal: u128,
}

impl std::fmt::Debug for LegacyUserData
{
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("LegacyUserData")
			.field("lang", &self.lang)
			.field("userSalt", &self.userSalt.as_ref().map(|_| "[REDACTED]"))
			.field("generatedId", &self.generatedId.as_ref().map(|_| "[REDACTED]"))
			.field("updateVal", &self.updateVal)
			.finish();
	}
}

impl LegacyUserData
{
	fn split(self) -> (UserPreferences, Option<ClientCryptoContext>)
	{
		let crypto = self.userSalt.map(|userSalt| ClientCryptoContext {
			userSalt,
		});
		let _ = self.generatedId;
		let preferences = UserPreferences {
			lang: self.lang,
			connected: crypto.is_some(),
			updateVal: self.updateVal,
		};
		return (preferences, crypto);
	}

	fn lang_default() -> String
	{
		return "EN".to_string();
	}

	fn cookie_signalGet() -> (Signal<Option<Self>>, WriteSignal<Option<Self>>)
	{
		return use_cookie_with_options::<Self, JsonSerdeCodec>(LEGACY_COOKIE_NAME, UseCookieOptions::default()
			.max_age(COOKIE_MAX_AGE)
			.same_site(SameSite::Strict)
			.secure(true)
			.path(ROOT_COOKIE_PATH));
	}
}

/// Owns the browser stores involved in the local user state and migration.
#[derive(Clone)]
pub(crate) struct ClientState
{
	preferences: Signal<Option<UserPreferences>>,
	setPreferences: WriteSignal<Option<UserPreferences>>,
	crypto: ClientCryptoStorage,
	legacyCrypto: Signal<Option<ClientCryptoContext>>,
	setLegacyCrypto: WriteSignal<Option<ClientCryptoContext>>,
	legacy: Signal<Option<LegacyUserData>>,
	setLegacy: WriteSignal<Option<LegacyUserData>>,
}

impl ClientState
{
	pub(crate) fn new() -> Self
	{
		let (preferences, setPreferences) = UserPreferences::cookie_signalGet();
		let crypto = ClientCryptoStorage::new();
		let (legacyCrypto, setLegacyCrypto) = ClientCryptoContext::legacyCookie_signalGet();
		let (legacy, setLegacy) = LegacyUserData::cookie_signalGet();
		return Self {
			preferences,
			setPreferences,
			crypto,
			legacyCrypto,
			setLegacyCrypto,
			legacy,
			setLegacy,
		};
	}

	pub(crate) fn expect() -> Self
	{
		return expect_context::<Self>();
	}

	pub(crate) fn initialize(&self, defaultLang: impl Into<String>) -> Result<(), ClientCryptoError>
	{
		let defaultLang = defaultLang.into();
		let legacy = self.legacy.get_untracked();
		let legacyHadCrypto = legacy.as_ref().and_then(|legacy| legacy.userSalt.as_ref()).is_some();
		let (legacyPreferences, legacyCrypto) = match legacy.clone()
		{
			Some(legacy) => {
				let (preferences, crypto) = legacy.split();
				(Some(preferences), crypto)
			},
			None => (None, None),
		};

		if (self.preferences.get_untracked().is_none())
		{
			self.setPreferences.set(Some(legacyPreferences.unwrap_or_else(|| UserPreferences::new(defaultLang))));
		}

		let scopedCrypto = self.legacyCrypto.get_untracked();
		if (!self.login_isConnected_untracked())
		{
			let clearResult = if (self.crypto.get().is_some() || self.crypto.error_get().is_some())
			{
				self.crypto.clear()
			}
			else
			{
				Ok(())
			};
			let legacyClearResult = self.legacyCookies_clear();
			return clearResult.and(legacyClearResult);
		}

		if (self.crypto.get().is_none())
		{
			if let Some(crypto) = scopedCrypto.clone().or(legacyCrypto)
			{
				self.crypto.set(crypto)?;
			}
			else if let Some(error) = self.crypto.error_get()
			{
				return Err(error);
			}
		}

		let cryptoMigrated = self.crypto.get().is_some();
		if (cryptoMigrated)
		{
			self.legacyCookies_clear()?;
		}
		else if (legacy.is_some() && !legacyHadCrypto)
		{
			self.setLegacy.set(None);
		}
		return Ok(());
	}

	pub(crate) fn login_apply(&self, crypto: ClientCryptoContext) -> Result<(), ClientCryptoError>
	{
		self.crypto.set(crypto)?;
		if let Err(error) = self.legacyCookies_clear()
		{
			let _ = self.crypto.clear();
			return Err(error);
		}
		let mut preferences = self.preferences.get_untracked().unwrap_or_default();
		preferences.connected = true;
		preferences.valUpdate();
		self.setPreferences.set(Some(preferences));
		return Ok(());
	}

	pub(crate) fn local_clear(&self) -> Result<(), ClientCryptoError>
	{
		let clearResult = self.crypto.clear();
		let legacyClearResult = self.legacyCookies_clear();
		let mut preferences = self.preferences.get_untracked().unwrap_or_default();
		preferences.connected = false;
		preferences.valUpdate();
		self.setPreferences.set(Some(preferences));
		return clearResult.and(legacyClearResult);
	}

	pub(crate) fn refresh(&self)
	{
		let mut preferences = self.preferences.get_untracked().unwrap_or_default();
		preferences.valUpdate();
		self.setPreferences.set(Some(preferences));
	}

	pub(crate) fn login_isConnected(&self) -> bool
	{
		return self.preferences.get().map(|preferences| preferences.connected).unwrap_or(false);
	}

	pub(crate) fn login_isConnected_untracked(&self) -> bool
	{
		return self.preferences.get_untracked().map(|preferences| preferences.connected).unwrap_or(false);
	}

	pub(crate) fn crypto_isAvailable(&self) -> bool
	{
		return self.crypto.isAvailable();
	}

	pub(crate) fn crypto_get(&self) -> Option<ClientCryptoContext>
	{
		return self.crypto.get();
	}

	pub(crate) fn lang_get(&self) -> String
	{
		return self.preferences.get().map(|preferences| preferences.lang).unwrap_or_else(|| "EN".to_string());
	}

	pub(crate) fn lang_get_untracked(&self) -> String
	{
		return self.preferences.get_untracked().map(|preferences| preferences.lang).unwrap_or_else(|| "EN".to_string());
	}

	fn legacyCookies_clear(&self) -> Result<(), ClientCryptoError>
	{
		let scopedClearResult = Self::legacyCookie_expire(LEGACY_CRYPTO_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_PATH);
		let rootClearResult = Self::legacyCookie_expire(LEGACY_COOKIE_NAME, ROOT_COOKIE_PATH);
		self.setLegacyCrypto.set(None);
		self.setLegacy.set(None);
		return scopedClearResult.and(rootClearResult);
	}

	fn legacyCookie_expiration_get(name: &str, path: &str) -> String
	{
		return format!("{}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path={}; SameSite=Strict; Secure", name, path);
	}

	#[cfg(feature = "ssr")]
	fn legacyCookie_expire(_name: &str, _path: &str) -> Result<(), ClientCryptoError>
	{
		return Ok(());
	}

	#[cfg(not(feature = "ssr"))]
	fn legacyCookie_expire(name: &str, path: &str) -> Result<(), ClientCryptoError>
	{
		use wasm_bindgen::JsCast;

		let document = web_sys::window()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?
			.document()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?;
		let document: &web_sys::HtmlDocument = document.unchecked_ref();
		return document.set_cookie(&Self::legacyCookie_expiration_get(name, path))
			.map_err(|_| ClientCryptoError::LEGACY_COOKIE_WRITE);
	}
}

#[cfg(test)]
mod tests
{
	use super::{ClientCryptoContext, ClientState, LegacyUserData, UserPreferences, CLIENT_CRYPTO_STORAGE_KEY, LEGACY_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_PATH, ROOT_COOKIE_PATH};

	fn cookiePath_matches(cookiePath: &str, requestPath: &str) -> bool
	{
		if (cookiePath == requestPath)
		{
			return true;
		}
		let Some(remainingPath) = requestPath.strip_prefix(cookiePath) else {return false};
		return cookiePath.ends_with('/') || remainingPath.starts_with('/');
	}

	#[test]
	fn signPassword_requiresTwelveCharacters()
	{
		assert!(!ClientCryptoContext::signPassword_isValid("12345678901"));
		assert!(ClientCryptoContext::signPassword_isValid("123456789012"));
	}

	#[test]
	fn legacyUserData_splitsSecretFromRootPreferences()
	{
		let legacy = LegacyUserData {
			lang: "FR".to_string(),
			userSalt: Some("secret".to_string()),
			generatedId: Some("account".to_string()),
			updateVal: 7,
		};
		let (preferences, crypto) = legacy.split();

		assert_eq!(preferences.lang, "FR");
		assert!(preferences.connected);
		let crypto = crypto.unwrap();
		assert_eq!(crypto.userSalt, "secret");
		assert!(!serde_json::to_string(&crypto).unwrap().contains("generatedId"));
		assert!(!serde_json::to_string(&preferences).unwrap().contains("userSalt"));
	}

	#[test]
	fn legacyCryptoCookiePath_excludesLeptosServerFunctionsDuringMigration()
	{
		let serverFnPrefix = option_env!("SERVER_FN_PREFIX").unwrap_or("/api");

		assert_eq!(LEGACY_CRYPTO_COOKIE_PATH, "/home");
		assert!(!cookiePath_matches(LEGACY_CRYPTO_COOKIE_PATH, serverFnPrefix));
		assert!(!cookiePath_matches(LEGACY_CRYPTO_COOKIE_PATH, "/api/API_modules_update"));
		assert!(cookiePath_matches(LEGACY_CRYPTO_COOKIE_PATH, "/home"));
		assert!(cookiePath_matches(LEGACY_CRYPTO_COOKIE_PATH, "/home/settings"));
		assert!(!cookiePath_matches(LEGACY_CRYPTO_COOKIE_PATH, "/homepage"));
	}

	#[test]
	fn legacyCookieExpiration_targetsHistoricalPath()
	{
		let scopedExpiration = ClientState::legacyCookie_expiration_get(LEGACY_CRYPTO_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_PATH);
		let rootExpiration = ClientState::legacyCookie_expiration_get(LEGACY_COOKIE_NAME, ROOT_COOKIE_PATH);

		assert!(scopedExpiration.starts_with("webhome-crypto=; Max-Age=0;"));
		assert!(scopedExpiration.contains("Path=/home;"));
		assert!(scopedExpiration.contains("SameSite=Strict; Secure"));
		assert!(rootExpiration.starts_with("webhome=; Max-Age=0;"));
		assert!(rootExpiration.contains("Path=/;"));
	}

	#[test]
	fn cryptoRoundTrip_rejectsAlteredContent()
	{
		let crypto = ClientCryptoContext {
			userSalt: "client-secret".to_string(),
		};
		let encrypted = crypto.encrypt("private content").unwrap();

		assert_eq!(crypto.decrypt(&encrypted).unwrap(), "private content");
		assert!(crypto.decrypt("not a ciphertext").is_err());
	}

	#[test]
	fn cryptoLocalStoragePayload_containsOnlyDerivationContext()
	{
		let crypto = ClientCryptoContext {
			userSalt: "client-secret".to_string(),
		};
		let serialized = serde_json::to_string(&crypto).unwrap();

		assert_eq!(CLIENT_CRYPTO_STORAGE_KEY, "webhome-crypto");
		assert!(serialized.contains("client-secret"));
		assert!(!serialized.contains("generatedId"));
		assert!(!serialized.contains("updateVal"));
	}

	#[test]
	fn rootPreferencesNeverSerializeClientSecret()
	{
		let preferences = UserPreferences::new("fr-FR");
		let serialized = serde_json::to_string(&preferences).unwrap();

		assert_eq!(preferences.lang, "FR");
		assert!(!serialized.contains("userSalt"));
		assert!(!serialized.contains("generatedId"));
	}

	#[test]
	fn secretContainers_debugOutputIsRedacted()
	{
		let crypto = ClientCryptoContext {
			userSalt: "raw-client-secret".to_string(),
		};
		let legacy = LegacyUserData {
			lang: "FR".to_string(),
			userSalt: Some("legacy-client-secret".to_string()),
			generatedId: Some("legacy-account-id".to_string()),
			updateVal: 4,
		};
		let debugOutput = format!("{:?} {:?}", crypto, legacy);

		assert!(!debugOutput.contains("raw-client-secret"));
		assert!(!debugOutput.contains("legacy-client-secret"));
		assert!(!debugOutput.contains("legacy-account-id"));
		assert!(debugOutput.contains("[REDACTED]"));
	}
}
