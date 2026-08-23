use aes_gcm::aead::common::Generate;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Config;
use base64ct::{Base64, Encoding};
use leptos::prelude::codee::string::JsonSerdeCodec;
use leptos::prelude::{expect_context, Effect, Get, GetUntracked, RwSignal, Set, Signal, WriteSignal};
use leptos_use::{use_cookie_with_options, SameSite, UseCookieOptions};
use serde::{Deserialize, Serialize};

use crate::api::login::{API_user_login, API_user_logout, API_user_passwordRotation_finalize, API_user_passwordRotation_prepare, API_user_preferences_get, API_user_preferences_set, API_user_salt, API_user_sign};
use crate::api::login::components::{AccountPreferencesError, PasswordRotationContent, PasswordRotationError, PasswordRotationFinalize};
use crate::front::utils::all_front_enum::AllFrontLoginEnum;
use crate::global_security::{generate_salt_raw, hash};
use crate::HWebTrace;

const COOKIE_MAX_AGE: i64 = 24 * 3600 * 1000;
#[cfg(any(not(feature="ssr"),test))]
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
	#[cfg(not(feature="ssr"))]
	STORAGE_UNAVAILABLE,
	#[cfg(not(feature="ssr"))]
	STORAGE_READ,
	#[cfg(not(feature="ssr"))]
	STORAGE_WRITE,
	#[cfg(not(feature="ssr"))]
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

	fn fromPassword(password: String, credentialSalt: String) -> Self
	{
		return Self {
			userSalt: Self::pwdHash(password,credentialSalt),
		};
	}

	fn credential_get(&self) -> String
	{
		return hash(self.userSalt.clone());
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

#[derive(Clone, Serialize, Deserialize)]
struct ClientCryptoPending
{
	crypto: ClientCryptoContext,
	request: PasswordRotationFinalize,
}

#[derive(Clone, Serialize, Deserialize)]
struct ClientCryptoStorageDocument
{
	version: u8,
	active: ClientCryptoContext,
	#[serde(default)]
	pending: Option<ClientCryptoPending>,
}

impl ClientCryptoStorageDocument
{
	const VERSION: u8 = 1;

	fn new(active: ClientCryptoContext, pending: Option<ClientCryptoPending>) -> Self
	{
		return Self {
			version: Self::VERSION,
			active,
			pending,
		};
	}
}

#[cfg(any(not(feature="ssr"),test))]
#[derive(Deserialize)]
#[serde(untagged)]
enum ClientCryptoStorageCompatibility
{
	Current(ClientCryptoStorageDocument),
	Legacy(ClientCryptoContext),
}

#[derive(Clone)]
struct ClientCryptoStorage
{
	value: RwSignal<Option<ClientCryptoContext>>,
	pending: RwSignal<Option<ClientCryptoPending>>,
	error: RwSignal<Option<ClientCryptoError>>,
}

impl ClientCryptoStorage
{
	fn new() -> Self
	{
		let (value, pending, error) = match Self::browser_read()
		{
			Ok(Some(document)) => (Some(document.active),document.pending,None),
			Ok(None) => (None,None,None),
			Err(error) => (None,None,Some(error)),
		};
		return Self {
			value: RwSignal::new(value),
			pending: RwSignal::new(pending),
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

	#[cfg(not(feature = "ssr"))]
	fn browser_sync(&self)
	{
		match Self::browser_read()
		{
			Ok(Some(document)) => {
				self.value.set(Some(document.active));
				self.pending.set(document.pending);
				self.error.set(None);
			},
			Ok(None) => {
				self.value.set(None);
				self.pending.set(None);
				self.error.set(None);
			},
			Err(error) => self.error.set(Some(error)),
		}
	}

	fn set(&self, crypto: ClientCryptoContext) -> Result<(), ClientCryptoError>
	{
		let document = ClientCryptoStorageDocument::new(crypto.clone(),None);
		if let Err(error) = Self::browser_write(&document)
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.value.set(Some(crypto));
		self.pending.set(None);
		self.error.set(None);
		return Ok(());
	}

	fn pending_get(&self) -> Option<ClientCryptoPending>
	{
		return self.pending.get_untracked();
	}

	fn pending_isAvailable(&self) -> bool
	{
		return self.pending.get().is_some();
	}

	fn pending_set(&self, pending: ClientCryptoPending) -> Result<(), ClientCryptoError>
	{
		let active = self.value.get_untracked().ok_or(ClientCryptoError::INVALID_ENVELOPE)?;
		let document = ClientCryptoStorageDocument::new(active,Some(pending.clone()));
		if let Err(error) = Self::browser_write(&document)
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.pending.set(Some(pending));
		self.error.set(None);
		return Ok(());
	}

	fn pending_clear(&self, rotationId: &str) -> Result<bool, ClientCryptoError>
	{
		if (self.pending.get_untracked().as_ref().map(|pending| pending.request.rotationId.as_str()) != Some(rotationId))
		{
			return Ok(false);
		}
		let active = self.value.get_untracked().ok_or(ClientCryptoError::INVALID_ENVELOPE)?;
		let document = ClientCryptoStorageDocument::new(active,None);
		if let Err(error) = Self::browser_write(&document)
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.pending.set(None);
		self.error.set(None);
		return Ok(true);
	}

	fn pending_promote(&self, rotationId: &str) -> Result<(), ClientCryptoError>
	{
		let pending = self.pending.get_untracked().ok_or(ClientCryptoError::INVALID_ENVELOPE)?;
		if (pending.request.rotationId != rotationId)
		{
			return Err(ClientCryptoError::INVALID_ENVELOPE);
		}
		let document = ClientCryptoStorageDocument::new(pending.crypto.clone(),None);
		if let Err(error) = Self::browser_write(&document)
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.value.set(Some(pending.crypto));
		self.pending.set(None);
		self.error.set(None);
		return Ok(());
	}

	fn clear(&self) -> Result<(), ClientCryptoError>
	{
		self.value.set(None);
		self.pending.set(None);
		if let Err(error) = Self::browser_remove()
		{
			self.error.set(Some(error));
			return Err(error);
		}
		self.error.set(None);
		return Ok(());
	}

	#[cfg(feature = "ssr")]
	fn browser_read() -> Result<Option<ClientCryptoStorageDocument>, ClientCryptoError>
	{
		return Ok(None);
	}

	#[cfg(not(feature = "ssr"))]
	fn browser_read() -> Result<Option<ClientCryptoStorageDocument>, ClientCryptoError>
	{
		let storage = web_sys::window()
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?
			.local_storage()
			.map_err(|_| ClientCryptoError::STORAGE_READ)?
			.ok_or(ClientCryptoError::STORAGE_UNAVAILABLE)?;
		let Some(serialized) = storage.get_item(CLIENT_CRYPTO_STORAGE_KEY)
			.map_err(|_| ClientCryptoError::STORAGE_READ)?
		else {return Ok(None)};
		let compatibility = serde_json::from_str::<ClientCryptoStorageCompatibility>(&serialized)
			.map_err(|_| ClientCryptoError::INVALID_ENVELOPE)?;
		return match compatibility
		{
			ClientCryptoStorageCompatibility::Current(document) if document.version == ClientCryptoStorageDocument::VERSION => Ok(Some(document)),
			ClientCryptoStorageCompatibility::Current(_) => Err(ClientCryptoError::INVALID_ENVELOPE),
			ClientCryptoStorageCompatibility::Legacy(active) => Ok(Some(ClientCryptoStorageDocument::new(active,None))),
		};
	}

	#[cfg(feature = "ssr")]
	fn browser_write(_document: &ClientCryptoStorageDocument) -> Result<(), ClientCryptoError>
	{
		return Ok(());
	}

	#[cfg(not(feature = "ssr"))]
	fn browser_write(document: &ClientCryptoStorageDocument) -> Result<(), ClientCryptoError>
	{
		let serialized = serde_json::to_string(document).map_err(|_| ClientCryptoError::SERIALIZATION)?;
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
	#[serde(default)]
	primaryHue: PrimaryHue,
	connected: bool,
	updateVal: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct PrimaryHue(u16);

impl PrimaryHue
{
	const DEFAULT: u16 = 212;
	const MAXIMUM: u16 = 359;

	fn new(value: u16) -> Option<Self>
	{
		return (value <= Self::MAXIMUM).then_some(Self(value));
	}

	fn get(self) -> u16
	{
		return self.0;
	}

	fn fromUnsigned(value: u64) -> Self
	{
		return u16::try_from(value).ok().and_then(Self::new).unwrap_or_default();
	}
}

impl Default for PrimaryHue
{
	fn default() -> Self
	{
		return Self(Self::DEFAULT);
	}
}

impl<'de> Deserialize<'de> for PrimaryHue
{
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct PrimaryHueVisitor;

		impl<'de> serde::de::Visitor<'de> for PrimaryHueVisitor
		{
			type Value = PrimaryHue;

			fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
			{
				return formatter.write_str("an integer hue between 0 and 359");
			}

			fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(PrimaryHue::fromUnsigned(value));
			}

			fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(u64::try_from(value).map(PrimaryHue::fromUnsigned).unwrap_or_default());
			}

			fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				if (!value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > u64::MAX as f64)
				{
					return Ok(PrimaryHue::default());
				}
				return Ok(PrimaryHue::fromUnsigned(value as u64));
			}

			fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(value.parse::<u64>().map(PrimaryHue::fromUnsigned).unwrap_or_default());
			}

			fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(PrimaryHue::default());
			}

			fn visit_none<E>(self) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(PrimaryHue::default());
			}

			fn visit_unit<E>(self) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				return Ok(PrimaryHue::default());
			}
		}

		return deserializer.deserialize_any(PrimaryHueVisitor);
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreferencesPreview
{
	lang: String,
	primaryHue: PrimaryHue,
}

impl PreferencesPreview
{
	fn fromPreferences(preferences: &UserPreferences) -> Self
	{
		return Self {
			lang: UserPreferences::lang_normalized(&preferences.lang),
			primaryHue: preferences.primaryHue,
		};
	}

	fn lang_set(&mut self, lang: &str) -> bool
	{
		let Some(lang) = UserPreferences::lang_supported(lang) else {return false};
		self.lang = lang;
		return true;
	}

	fn primaryHue_set(&mut self, value: u16) -> bool
	{
		let Some(primaryHue) = PrimaryHue::new(value) else {return false};
		self.primaryHue = primaryHue;
		return true;
	}
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct AccountPreferences
{
	version: u8,
	lang: String,
	primaryHue: PrimaryHue,
}

impl AccountPreferences
{
	const VERSION: u8 = 1;

	fn fromUserPreferences(preferences: &UserPreferences) -> Self
	{
		return Self {
			version: Self::VERSION,
			lang: UserPreferences::lang_normalized(&preferences.lang),
			primaryHue: preferences.primaryHue,
		};
	}

	fn fromPreview(preview: &PreferencesPreview) -> Self
	{
		return Self {
			version: Self::VERSION,
			lang: UserPreferences::lang_normalized(&preview.lang),
			primaryHue: preview.primaryHue,
		};
	}

	fn deserialize(content: &str) -> Result<Self, AccountPreferencesError>
	{
		let mut preferences = serde_json::from_str::<Self>(content)
			.map_err(|_| AccountPreferencesError::CONTENT_INVALID)?;
		if (preferences.version != Self::VERSION)
		{
			return Err(AccountPreferencesError::CONTENT_INVALID);
		}
		preferences.lang = UserPreferences::lang_supported(&preferences.lang)
			.ok_or(AccountPreferencesError::CONTENT_INVALID)?;
		return Ok(preferences);
	}

	fn serialize(&self) -> Result<String, AccountPreferencesError>
	{
		return serde_json::to_string(self).map_err(|_| AccountPreferencesError::CONTENT_INVALID);
	}
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
		self.lang = Self::lang_normalized(&lang.into());
	}

	fn lang_supported(lang: &str) -> Option<String>
	{
		let lang = lang.split('-').next().unwrap_or("EN").to_uppercase();
		return ["EN", "FR"].contains(&lang.as_str()).then_some(lang);
	}

	fn lang_normalized(lang: &str) -> String
	{
		return Self::lang_supported(lang).unwrap_or_else(|| "EN".to_string());
	}

	fn normalize(&mut self) -> bool
	{
		let lang = Self::lang_normalized(&self.lang);
		if (lang == self.lang)
		{
			return false;
		}
		self.lang = lang;
		return true;
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
			primaryHue: PrimaryHue::default(),
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
			primaryHue: PrimaryHue::default(),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientDisconnectReason
{
	LOCAL_CONNECTION_CLOSED,
	LOCAL_CRYPTO_REMOVED,
}

impl ClientDisconnectReason
{
	pub(crate) fn traceKey_get(self) -> &'static str
	{
		return match self
		{
			Self::LOCAL_CONNECTION_CLOSED => "local_connection_closed",
			Self::LOCAL_CRYPTO_REMOVED => "local_crypto_removed",
		};
	}
}

/// Owns the browser stores involved in the local user state and migration.
#[derive(Clone)]
pub(crate) struct ClientState
{
	preferences: RwSignal<UserPreferences>,
	preferencesMirror: Signal<Option<UserPreferences>>,
	setPreferencesMirror: WriteSignal<Option<UserPreferences>>,
	preferencesMirrorReady: RwSignal<bool>,
	connection: RwSignal<bool>,
	preferencesPreview: RwSignal<Option<PreferencesPreview>>,
	passwordRotationRunning: RwSignal<bool>,
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
		let (preferencesMirror, setPreferencesMirror) = UserPreferences::cookie_signalGet();
		let crypto = ClientCryptoStorage::new();
		let initialPreferences = preferencesMirror.get_untracked().unwrap_or_default();
		let initialConnection = preferencesMirror.get_untracked()
			.map(|preferences| preferences.connected)
			.unwrap_or_else(|| crypto.get().is_some());
		#[cfg(not(feature="ssr"))]
		{
			let storageCrypto = crypto.clone();
			let _ = leptos_use::use_event_listener(leptos_use::use_window(),leptos::ev::storage,move |event| {
				if (event.key().as_deref() == Some(CLIENT_CRYPTO_STORAGE_KEY) || event.key().is_none())
				{
					storageCrypto.browser_sync();
				}
			});
		}
		let (legacyCrypto, setLegacyCrypto) = ClientCryptoContext::legacyCookie_signalGet();
		let (legacy, setLegacy) = LegacyUserData::cookie_signalGet();
		let clientState = Self {
			preferences: RwSignal::new(initialPreferences),
			preferencesMirror,
			setPreferencesMirror,
			preferencesMirrorReady: RwSignal::new(false),
			connection: RwSignal::new(initialConnection),
			preferencesPreview: RwSignal::new(None),
			passwordRotationRunning: RwSignal::new(false),
			crypto,
			legacyCrypto,
			setLegacyCrypto,
			legacy,
			setLegacy,
		};
		let mirrorState = clientState.clone();
		Effect::new(move || {
			if (!mirrorState.preferencesMirrorReady.get())
			{
				return;
			}
			let mirror = mirrorState.preferencesMirror.get();
			if (mirrorState.preferencesMirror_reconcile(mirror))
			{
				HWebTrace!("client preference mirror restored");
			}
		});
		return clientState;
	}

	pub(crate) fn expect() -> Self
	{
		return expect_context::<Self>();
	}

	pub(crate) fn initialize(&self, defaultLang: impl Into<String>) -> Result<(), ClientCryptoError>
	{
		let defaultLang = defaultLang.into();
		let preferencesMirror = self.preferencesMirror.get_untracked();
		let preferencesMirrorWasMissing = preferencesMirror.is_none();
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

		let scopedCrypto = self.legacyCrypto.get_untracked();
		let localCryptoAvailable = self.crypto.get().is_some()
			|| scopedCrypto.is_some()
			|| legacyCrypto.is_some();
		let mut preferences = preferencesMirror
			.or(legacyPreferences)
			.unwrap_or_else(|| UserPreferences::new(defaultLang));
		preferences.normalize();
		let connection = preferences.connected
			|| (preferencesMirrorWasMissing && localCryptoAvailable);
		self.connection.set(connection);
		preferences.connected = connection;
		self.preferences_set(preferences);
		self.preferencesMirrorReady.set(true);

		if (!self.login_isConnected_untracked())
		{
			let mut preferences = self.preferences.get_untracked();
			if (preferences.primaryHue != PrimaryHue::default())
			{
				preferences.primaryHue = PrimaryHue::default();
				preferences.valUpdate();
				self.preferences_set(preferences);
			}
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

	pub(crate) async fn login_apply(&self, crypto: ClientCryptoContext) -> Result<Option<AccountPreferencesError>, AccountPreferencesError>
	{
		let currentPreferences = self.preferences.get_untracked();
		let accountPreferencesResult = match API_user_preferences_get().await
		{
			Ok(Some(content)) => {
				crypto.decrypt(&content)
					.map_err(|_| AccountPreferencesError::CRYPTO_FAILED)
					.and_then(|plaintext| AccountPreferences::deserialize(&plaintext))
			},
			Ok(None) => {
				let accountPreferences = AccountPreferences::fromUserPreferences(&currentPreferences);
				match accountPreferences.serialize()
					.and_then(|plaintext| crypto.encrypt(&plaintext).map_err(|_| AccountPreferencesError::CRYPTO_FAILED))
				{
					Ok(content) => API_user_preferences_set(content).await.map(|_| accountPreferences),
					Err(error) => Err(error),
				}
			},
			Err(error) => Err(error),
		};
		let (accountPreferences,warning) = match accountPreferencesResult
		{
			Ok(accountPreferences) => (accountPreferences,None),
			Err(AccountPreferencesError::AUTH_REQUIRED) => return Err(AccountPreferencesError::AUTH_REQUIRED),
			Err(error) => (AccountPreferences::fromUserPreferences(&currentPreferences),Some(error)),
		};

		self.crypto.set(crypto).map_err(|_| AccountPreferencesError::STORAGE_FAILED)?;
		if let Err(error) = self.legacyCookies_clear()
		{
			let _ = self.crypto.clear();
			let _ = error;
			return Err(AccountPreferencesError::STORAGE_FAILED);
		}
		self.connection.set(true);
		let mut preferences = self.preferences.get_untracked();
		preferences.lang = accountPreferences.lang;
		preferences.primaryHue = accountPreferences.primaryHue;
		preferences.connected = true;
		preferences.valUpdate();
		self.preferences_set(preferences);
		return Ok(warning);
	}

	pub(crate) fn local_clear(&self) -> Result<(), ClientCryptoError>
	{
		let clearResult = self.crypto.clear();
		let legacyClearResult = self.legacyCookies_clear();
		self.connection.set(false);
		let mut preferences = self.preferences.get_untracked();
		preferences.primaryHue = PrimaryHue::default();
		preferences.connected = false;
		preferences.valUpdate();
		self.preferencesPreview.set(None);
		self.preferences_set(preferences);
		return clearResult.and(legacyClearResult);
	}

	pub(crate) fn refresh(&self)
	{
		let mut preferences = self.preferences.get_untracked();
		preferences.valUpdate();
		self.preferences_set(preferences);
	}

	pub(crate) fn login_isConnected_untracked(&self) -> bool
	{
		return self.connection.get_untracked();
	}

	pub(crate) fn disconnectReason_get(&self) -> Option<ClientDisconnectReason>
	{
		return Self::disconnectReason_resolve(self.connection.get(),self.crypto.isAvailable());
	}

	pub(crate) fn crypto_get(&self) -> Option<ClientCryptoContext>
	{
		return self.crypto.get();
	}

	pub(crate) fn passwordRotation_pendingIsAvailable(&self) -> bool
	{
		return self.crypto.pending_isAvailable();
	}

	pub(crate) fn passwordRotation_pendingIsAvailable_untracked(&self) -> bool
	{
		return self.crypto.pending_get().is_some();
	}

	pub(crate) fn passwordRotation_runningIsActive(&self) -> bool
	{
		return self.passwordRotationRunning.get();
	}

	pub(crate) fn passwordRotation_runningIsActive_untracked(&self) -> bool
	{
		return self.passwordRotationRunning.get_untracked();
	}

	pub(crate) fn passwordRotation_runningSet(&self, running: bool)
	{
		self.passwordRotationRunning.set(running);
	}

	pub(crate) fn passwordRotation_canClose(&self) -> bool
	{
		return !self.passwordRotationRunning.get()
			&& !self.crypto.pending_isAvailable();
	}

	pub(crate) async fn passwordRotation_change(&self, currentPassword: String, newPassword: String, confirmation: String) -> Result<(), PasswordRotationError>
	{
		if (!ClientCryptoContext::signPassword_isValid(&newPassword))
		{
			return Err(PasswordRotationError::NEW_TOO_SHORT);
		}
		if (newPassword != confirmation)
		{
			return Err(PasswordRotationError::CONFIRMATION_MISMATCH);
		}
		if (currentPassword == newPassword)
		{
			return Err(PasswordRotationError::UNCHANGED);
		}

		let activeCrypto = self.crypto.get().ok_or(PasswordRotationError::REAUTH_REQUIRED)?;
		let snapshot = API_user_passwordRotation_prepare().await?;
		let oldCrypto = ClientCryptoContext::fromPassword(currentPassword,snapshot.credentialSalt.clone());
		let newCrypto = ClientCryptoContext::fromPassword(newPassword,snapshot.credentialSalt);
		drop(confirmation);

		if (oldCrypto != activeCrypto)
		{
			return Err(PasswordRotationError::CURRENT_INVALID);
		}
		if (newCrypto == oldCrypto)
		{
			return Err(PasswordRotationError::UNCHANGED);
		}

		let mut contents = Vec::with_capacity(snapshot.contents.len());
		for content in snapshot.contents
		{
			let plaintext = oldCrypto.decrypt(&content.content)
				.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
			let encrypted = newCrypto.encrypt(&plaintext)
				.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
			contents.push(PasswordRotationContent {
				id: content.id,
				content: encrypted,
			});
		}

		let request = PasswordRotationFinalize {
			rotationId: snapshot.rotationId,
			credentialVersion: snapshot.credentialVersion,
			revision: snapshot.revision,
			oldCredential: oldCrypto.credential_get(),
			newCredential: newCrypto.credential_get(),
			preferences: match snapshot.preferences
			{
				Some(content) => {
					let plaintext = oldCrypto.decrypt(&content)
						.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
					Some(newCrypto.encrypt(&plaintext)
						.map_err(|_| PasswordRotationError::CONTENT_INVALID)?)
				},
				None => None,
			},
			contents,
		};
		let pending = ClientCryptoPending {
			crypto: newCrypto,
			request,
		};
		self.crypto.pending_set(pending.clone()).map_err(|_| PasswordRotationError::STORAGE_FAILED)?;
		return self.passwordRotation_finalizePending(pending).await;
	}

	pub(crate) async fn passwordRotation_resume(&self) -> Result<bool, PasswordRotationError>
	{
		let Some(pending) = self.crypto.pending_get() else {return Ok(false)};
		self.passwordRotation_finalizePending(pending).await?;
		return Ok(true);
	}

	async fn passwordRotation_finalizePending(&self, pending: ClientCryptoPending) -> Result<(), PasswordRotationError>
	{
		let rotationId = pending.request.rotationId.clone();
		match API_user_passwordRotation_finalize(pending.request).await
		{
			Ok(()) => {
				self.crypto.pending_promote(&rotationId).map_err(|_| PasswordRotationError::STORAGE_FAILED)?;
				return Ok(());
			},
			Err(error) => {
				if (Self::passwordRotation_errorIsDefinitive(error))
				{
					let _ = self.crypto.pending_clear(&rotationId).map_err(|_| PasswordRotationError::STORAGE_FAILED)?;
				}
				return Err(error);
			},
		}
	}

	fn passwordRotation_errorIsDefinitive(error: PasswordRotationError) -> bool
	{
		return matches!(error,
			PasswordRotationError::REAUTH_REQUIRED
			| PasswordRotationError::CURRENT_INVALID
			| PasswordRotationError::NEW_TOO_SHORT
			| PasswordRotationError::CONFIRMATION_MISMATCH
			| PasswordRotationError::UNCHANGED
			| PasswordRotationError::CONTENT_INVALID
			| PasswordRotationError::CONFLICT
		);
	}

	pub(crate) fn lang_get(&self) -> String
	{
		if let Some(preview) = self.preferencesPreview.get()
		{
			return preview.lang;
		}
		return self.preferences.get().lang;
	}

	pub(crate) fn lang_get_untracked(&self) -> String
	{
		if let Some(preview) = self.preferencesPreview.get_untracked()
		{
			return preview.lang;
		}
		return self.preferences.get_untracked().lang;
	}

	pub(crate) fn primaryHue_get(&self) -> u16
	{
		if let Some(preview) = self.preferencesPreview.get()
		{
			return preview.primaryHue.get();
		}
		return self.preferences.get().primaryHue.get();
	}

	pub(crate) fn preferencesPreview_begin(&self)
	{
		let preferences = self.preferences.get_untracked();
		self.preferencesPreview.set(Some(PreferencesPreview::fromPreferences(&preferences)));
	}

	pub(crate) fn preferencesPreview_langSet(&self, lang: &str) -> bool
	{
		let Some(mut preview) = self.preferencesPreview.get_untracked() else {return false};
		if (!preview.lang_set(lang))
		{
			return false;
		}
		self.preferencesPreview.set(Some(preview));
		return true;
	}

	pub(crate) fn preferencesPreview_primaryHueSet(&self, value: u16) -> bool
	{
		let Some(mut preview) = self.preferencesPreview.get_untracked() else {return false};
		if (!preview.primaryHue_set(value))
		{
			return false;
		}
		self.preferencesPreview.set(Some(preview));
		return true;
	}

	pub(crate) async fn preferencesPreview_commit(&self) -> Result<(), AccountPreferencesError>
	{
		let preview = self.preferencesPreview.get_untracked().ok_or(AccountPreferencesError::CONTENT_INVALID)?;
		let crypto = self.crypto.get().ok_or(AccountPreferencesError::CRYPTO_FAILED)?;
		let accountPreferences = AccountPreferences::fromPreview(&preview);
		let plaintext = accountPreferences.serialize()?;
		let content = crypto.encrypt(&plaintext).map_err(|_| AccountPreferencesError::CRYPTO_FAILED)?;
		API_user_preferences_set(content).await?;

		let mut preferences = self.preferences.get_untracked();
		preferences.lang = accountPreferences.lang;
		preferences.primaryHue = accountPreferences.primaryHue;
		preferences.valUpdate();
		self.preferences_set(preferences);
		self.preferencesPreview.set(None);
		return Ok(());
	}

	pub(crate) fn preferencesPreview_cancel(&self)
	{
		self.preferencesPreview.set(None);
	}

	fn preferences_set(&self, mut preferences: UserPreferences)
	{
		preferences.connected = self.connection.get_untracked();
		self.preferences.set(preferences.clone());
		self.setPreferencesMirror.set(Some(preferences));
	}

	fn preferencesMirror_reconcile(&self, mirror: Option<UserPreferences>) -> bool
	{
		let Some(mut preferences) = mirror else {
			let mut preferences = self.preferences.get_untracked();
			preferences.connected = self.connection.get_untracked();
			self.preferences.set(preferences.clone());
			self.setPreferencesMirror.set(Some(preferences));
			return true;
		};
		preferences.connected = self.connection.get_untracked();
		preferences.normalize();
		self.preferences.set(preferences);
		return false;
	}

	fn disconnectReason_resolve(connectionAvailable: bool, cryptoAvailable: bool) -> Option<ClientDisconnectReason>
	{
		if (!connectionAvailable)
		{
			return Some(ClientDisconnectReason::LOCAL_CONNECTION_CLOSED);
		}
		if (!cryptoAvailable)
		{
			return Some(ClientDisconnectReason::LOCAL_CRYPTO_REMOVED);
		}
		return None;
	}

	fn legacyCookies_clear(&self) -> Result<(), ClientCryptoError>
	{
		let scopedClearResult = Self::legacyCookie_expire(LEGACY_CRYPTO_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_PATH);
		let rootClearResult = Self::legacyCookie_expire(LEGACY_COOKIE_NAME, ROOT_COOKIE_PATH);
		self.setLegacyCrypto.set(None);
		self.setLegacy.set(None);
		return scopedClearResult.and(rootClearResult);
	}

	#[cfg(any(not(feature="ssr"),test))]
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
	use leptos::prelude::{GetUntracked, Owner, Set};
	use crate::api::login::components::{AccountPreferencesError, PasswordRotationContent, PasswordRotationFinalize};
	use crate::api::modules::components::ModuleID;
	use super::{AccountPreferences, ClientCryptoContext, ClientCryptoPending, ClientCryptoStorage, ClientCryptoStorageCompatibility, ClientDisconnectReason, ClientState, LegacyUserData, PreferencesPreview, PrimaryHue, UserPreferences, CLIENT_CRYPTO_STORAGE_KEY, LEGACY_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_NAME, LEGACY_CRYPTO_COOKIE_PATH, ROOT_COOKIE_PATH};

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
		assert_eq!(preferences.primaryHue.get(), PrimaryHue::DEFAULT);
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
	fn cryptoDecrypt_readsLegacyAesGcmRc3Payload()
	{
		let crypto = ClientCryptoContext {
			userSalt: "client-secret".to_string(),
		};
		let legacyPayload = r#"{"salt":"cRzEPuLwOzBQnHkZfrs3vQ==","nonce":"EbbQRVrRkad+Kado","content":"zvRFqW+SHlCY43aPGtXtg62ZsSiK7LESi/vtcGwoPQ=="}"#;

		assert_eq!(crypto.decrypt(legacyPayload).unwrap(), "private content");
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
	fn cryptoStorage_readsLegacyDocumentAndKeepsPendingSeparateFromActive()
	{
		let legacy = r#"{"userSalt":"legacy-secret"}"#;
		let ClientCryptoStorageCompatibility::Legacy(legacyCrypto) = serde_json::from_str(legacy).unwrap()
		else {panic!("legacy crypto context was not recognized")};
		assert_eq!(legacyCrypto.userSalt,"legacy-secret");
		let current = r#"{"version":1,"active":{"userSalt":"current-secret"},"pending":null}"#;
		let ClientCryptoStorageCompatibility::Current(currentDocument) = serde_json::from_str(current).unwrap()
		else {panic!("versioned crypto document was not recognized")};
		assert_eq!(currentDocument.version,1);
		assert_eq!(currentDocument.active.userSalt,"current-secret");

		let owner = Owner::new();
		owner.with(|| {
			let storage = ClientCryptoStorage::new();
			let active = ClientCryptoContext {userSalt: "active-secret".to_string()};
			let next = ClientCryptoContext {userSalt: "next-secret".to_string()};
			storage.set(active.clone()).unwrap();
			storage.pending_set(ClientCryptoPending {
				crypto: next.clone(),
				request: PasswordRotationFinalize {
					rotationId: "test-rotation".to_string(),
					credentialVersion: 0,
					revision: "test-revision".to_string(),
						oldCredential: "old-credential".to_string(),
						newCredential: "new-credential".to_string(),
						preferences: None,
					contents: vec![PasswordRotationContent {
						id: ModuleID {id: "module".to_string()},
						content: "new-ciphertext".to_string(),
					}],
				},
			}).unwrap();

			assert_eq!(storage.get(),Some(active));
			assert!(storage.pending_get().is_some());
			assert!(!storage.pending_clear("another-rotation").unwrap());
			assert!(storage.pending_get().is_some());
			storage.pending_promote("test-rotation").unwrap();
			assert_eq!(storage.get(),Some(next));
			assert!(storage.pending_get().is_none());
		});
		owner.cleanup();
	}

	#[test]
	fn passwordDerivedContexts_reencryptWithoutChangingPlaintext()
	{
		let oldCrypto = ClientCryptoContext::fromPassword("old password".to_string(),"credential salt".to_string());
		let newCrypto = ClientCryptoContext::fromPassword("new password".to_string(),"credential salt".to_string());
		let oldCiphertext = oldCrypto.encrypt("private module data").unwrap();
		let plaintext = oldCrypto.decrypt(&oldCiphertext).unwrap();
		let newCiphertext = newCrypto.encrypt(&plaintext).unwrap();

		assert_ne!(oldCrypto,newCrypto);
		assert_ne!(oldCrypto.credential_get(),newCrypto.credential_get());
		assert_eq!(newCrypto.decrypt(&newCiphertext).unwrap(),"private module data");
		assert!(oldCrypto.decrypt(&newCiphertext).is_err());
	}

	#[test]
	fn accountPreferences_areVersionedAndEncryptedWithoutConnectionState()
	{
		let crypto = ClientCryptoContext::test_get();
		let preferences = UserPreferences {
			lang: "FR".to_string(),
			primaryHue: PrimaryHue::new(42).unwrap(),
			connected: true,
			updateVal: 99,
		};
		let accountPreferences = AccountPreferences::fromUserPreferences(&preferences);
		let plaintext = accountPreferences.serialize().unwrap();
		let encrypted = crypto.encrypt(&plaintext).unwrap();

		assert_ne!(encrypted,plaintext);
		assert!(serde_json::from_str::<super::ClientCiphertext>(&encrypted).is_ok());
		assert!(!plaintext.contains("connected"));
		assert!(!plaintext.contains("updateVal"));
		let decrypted = crypto.decrypt(&encrypted).unwrap();
		assert_eq!(AccountPreferences::deserialize(&decrypted).unwrap(),accountPreferences);
	}

	#[test]
	fn accountPreferences_rejectUnknownVersionAndLanguage()
	{
		assert_eq!(
			AccountPreferences::deserialize(r#"{"version":2,"lang":"FR","primaryHue":42}"#),
			Err(AccountPreferencesError::CONTENT_INVALID),
		);
		assert_eq!(
			AccountPreferences::deserialize(r#"{"version":1,"lang":"DE","primaryHue":42}"#),
			Err(AccountPreferencesError::CONTENT_INVALID),
		);
	}

	#[test]
	fn localLogout_resetsHueButKeepsLanguage()
	{
		let owner = Owner::new();
		owner.with(|| {
			let clientState = ClientState::new();
			clientState.connection.set(true);
			clientState.preferences_set(UserPreferences {
				lang: "FR".to_string(),
				primaryHue: PrimaryHue::new(42).unwrap(),
				connected: true,
				updateVal: 3,
			});
			clientState.preferencesPreview_begin();
			clientState.crypto.set(ClientCryptoContext::test_get()).unwrap();

			clientState.local_clear().unwrap();

			let preferences = clientState.preferences.get_untracked();
			assert_eq!(preferences.lang,"FR");
			assert_eq!(preferences.primaryHue,PrimaryHue::default());
			assert!(!preferences.connected);
			assert_eq!(preferences.updateVal,4);
			assert!(!clientState.login_isConnected_untracked());
			assert!(clientState.preferencesPreview.get_untracked().is_none());
			assert!(clientState.crypto.get().is_none());
		});
		owner.cleanup();
	}

	#[test]
	fn missingPreferenceMirror_recoversFromLocalCrypto()
	{
		let owner = Owner::new();
		owner.with(|| {
			let clientState = ClientState::new();
			clientState.crypto.set(ClientCryptoContext::test_get()).unwrap();
			clientState.setPreferencesMirror.set(None);

			clientState.initialize("FR").unwrap();

			let preferences = clientState.preferences.get_untracked();
			assert!(clientState.login_isConnected_untracked());
			assert!(clientState.crypto.get().is_some());
			assert_eq!(preferences.lang,"FR");
			assert!(preferences.connected);
			assert!(clientState.preferencesMirror.get_untracked().is_some());
		});
		owner.cleanup();
	}

	#[test]
	fn disconnectedPreferenceMirror_remainsFailClosed()
	{
		let owner = Owner::new();
		owner.with(|| {
			let clientState = ClientState::new();
			clientState.crypto.set(ClientCryptoContext::test_get()).unwrap();
			clientState.setPreferencesMirror.set(Some(UserPreferences {
				lang: "FR".to_string(),
				primaryHue: PrimaryHue::new(42).unwrap(),
				connected: false,
				updateVal: 4,
			}));

			clientState.initialize("EN").unwrap();

			let preferences = clientState.preferences.get_untracked();
			assert!(!clientState.login_isConnected_untracked());
			assert!(clientState.crypto.get().is_none());
			assert_eq!(preferences.lang,"FR");
			assert_eq!(preferences.primaryHue,PrimaryHue::default());
			assert!(!preferences.connected);
		});
		owner.cleanup();
	}

	#[test]
	fn preferenceMirrorExpiration_preservesRuntimeSessionAndPreferences()
	{
		let owner = Owner::new();
		owner.with(|| {
			let clientState = ClientState::new();
			clientState.connection.set(true);
			clientState.crypto.set(ClientCryptoContext::test_get()).unwrap();
			clientState.preferences_set(UserPreferences {
				lang: "FR".to_string(),
				primaryHue: PrimaryHue::new(42).unwrap(),
				connected: true,
				updateVal: 7,
			});

			assert!(clientState.preferencesMirror_reconcile(None));

			let preferences = clientState.preferences.get_untracked();
			assert!(clientState.login_isConnected_untracked());
			assert!(clientState.crypto.get().is_some());
			assert_eq!(preferences.lang,"FR");
			assert_eq!(preferences.primaryHue,PrimaryHue::new(42).unwrap());
			assert!(clientState.preferencesMirror.get_untracked().is_some());
		});
		owner.cleanup();
	}

	#[test]
	fn preferenceMirrorUpdate_cannotChangeRuntimeConnection()
	{
		let owner = Owner::new();
		owner.with(|| {
			let clientState = ClientState::new();
			clientState.connection.set(true);
			clientState.crypto.set(ClientCryptoContext::test_get()).unwrap();
			clientState.preferencesMirror_reconcile(Some(UserPreferences {
				lang: "FR".to_string(),
				primaryHue: PrimaryHue::new(42).unwrap(),
				connected: false,
				updateVal: 8,
			}));

			assert!(clientState.login_isConnected_untracked());
			assert!(clientState.preferences.get_untracked().connected);

			clientState.connection.set(false);
			clientState.preferencesMirror_reconcile(Some(UserPreferences {
				lang: "EN".to_string(),
				primaryHue: PrimaryHue::new(84).unwrap(),
				connected: true,
				updateVal: 9,
			}));

			assert!(!clientState.login_isConnected_untracked());
			assert!(!clientState.preferences.get_untracked().connected);
		});
		owner.cleanup();
	}

	#[test]
	fn disconnectReason_requiresLocalClosureOrCryptoRemoval()
	{
		assert_eq!(ClientState::disconnectReason_resolve(true,true),None);
		assert_eq!(
			ClientState::disconnectReason_resolve(false,true),
			Some(ClientDisconnectReason::LOCAL_CONNECTION_CLOSED),
		);
		assert_eq!(
			ClientState::disconnectReason_resolve(true,false),
			Some(ClientDisconnectReason::LOCAL_CRYPTO_REMOVED),
		);
	}

	#[test]
	fn rootPreferencesNeverSerializeClientSecret()
	{
		let preferences = UserPreferences::new("fr-FR");
		let serialized = serde_json::to_string(&preferences).unwrap();

		assert_eq!(preferences.lang, "FR");
		assert_eq!(preferences.primaryHue.get(), PrimaryHue::DEFAULT);
		assert!(serialized.contains("\"primaryHue\":212"));
		assert!(!serialized.contains("userSalt"));
		assert!(!serialized.contains("generatedId"));
	}

	#[test]
	fn preferencesLegacyCookie_defaultsPrimaryHue()
	{
		let preferences: UserPreferences = serde_json::from_str(r#"{"lang":"FR","connected":true,"updateVal":3}"#).unwrap();

		assert_eq!(preferences.lang, "FR");
		assert_eq!(preferences.primaryHue.get(), 212);
		assert!(preferences.connected);
	}

	#[test]
	fn primaryHueDeserialization_rejectsUnsafeValuesToDefault()
	{
		for rawHue in ["-1", "360", "99999", "true", "null", "\"invalid\""]
		{
			let json = format!(r#"{{"lang":"EN","primaryHue":{},"connected":false,"updateVal":0}}"#, rawHue);
			let preferences: UserPreferences = serde_json::from_str(&json).unwrap();

			assert_eq!(preferences.primaryHue.get(), PrimaryHue::DEFAULT, "raw hue {rawHue}");
		}
	}

	#[test]
	fn preferencesPreview_acceptsOnlySupportedLanguageAndBoundedHue()
	{
		let preferences = UserPreferences::default();
		let mut preview = PreferencesPreview::fromPreferences(&preferences);

		assert!(preview.lang_set("fr"));
		assert_eq!(preview.lang, "FR");
		assert!(!preview.lang_set("DE"));
		assert_eq!(preview.lang, "FR");
		assert!(preview.primaryHue_set(0));
		assert_eq!(preview.primaryHue.get(), 0);
		assert!(preview.primaryHue_set(359));
		assert_eq!(preview.primaryHue.get(), 359);
		assert!(!preview.primaryHue_set(360));
		assert_eq!(preview.primaryHue.get(), 359);
	}

	#[test]
	fn preferencesNormalize_fallsBackToEnglish()
	{
		let mut preferences = UserPreferences::new("de-DE");

		assert_eq!(preferences.lang, "EN");
		preferences.lang = "invalid".to_string();
		assert!(preferences.normalize());
		assert_eq!(preferences.lang, "EN");
		assert!(!preferences.normalize());
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
