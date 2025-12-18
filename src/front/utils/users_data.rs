use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use argon2::Config;
use leptos::prelude::codee::string::JsonSerdeCodec;
use leptos::prelude::{Read, Signal, WriteSignal};
use leptos_use::{use_cookie_with_options, SameSite, UseCookieOptions};
use serde::{Deserialize, Serialize};
use crate::api::login::{API_user_login, API_user_salt, API_user_sign};
use crate::api::login::components::{LoginStatus, SaltReturn};
use crate::global_security::{generate_salt_raw, hash};
use base64ct::{Base64, Encoding};
use crate::front::utils::all_front_enum::AllFrontLoginEnum;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserDataCypher
{
	pub salt: String,
	pub nonce: String,
	pub content: String,
}

/// note about salt, hash and secure send/storage client data
///
/// the objectif is to disallow the server to know anything about any client data
/// So the client must send all data hash or crypted
/// Also the server must control the "global" salt
///
/// To do that is use some step for each action.
///
/// Note all "hash" here, use the global_security::hash() method, that use an auto-generated salt
///
/// # Login
///
/// The login is simply hash by the client, this hash is used to store all client data into a "<hash>.json" file
///
/// # data
///
/// step by step:
/// * Using the hashed login (generatedId)
/// * Send it to API_user_salt(generatedId) that combine the generatedId and the server salt, get the result as serverUserSalt
/// * client hash serverUserSalt and user password into user_salt
/// * TODO crypt using symmetric stuff
///
/// # password
///
/// the password is like a data, but instead of crypt, client hash it and send it to the server on sign up or login only

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserData
{
	lang: String,
	userSalt: Option<String>,
	generatedId: Option<String>
}

impl UserData {
	pub fn new(lang: &String) -> Self
	{
		let mut new = Self::default();
		new.lang_set(lang);
		return new;
	}

	pub fn lang_get(&self) -> String
	{
		self.lang.clone()
	}

	pub fn lang_set(&mut self, lang: impl Into<String>)
	{
		let lang = lang.into();
		let splittedVal = lang.split('-').collect::<Vec<&str>>();
		let newlang = splittedVal.first().unwrap_or(&"EN").to_string().to_uppercase();
		if(self.lang != newlang) {
			self.lang = splittedVal.first().unwrap_or(&"EN").to_string().to_uppercase();
		}
	}

	pub fn login_get(&self) -> Option<String>
	{
		return self.generatedId.clone();
	}

	pub fn login_isConnected(&self) -> bool
	{
		return self.userSalt.is_some();
	}

	pub async fn login_set(&mut self, login: String, pwd: String) -> Option<String>
	{
		let generatedId = hash(login);
		let Ok(SaltReturn::SALT(serverUserSalt)) = API_user_salt(generatedId.clone()).await else {return None};
		let user_salt = Self::pwdHash(pwd, serverUserSalt.clone());

		return match API_user_login(generatedId.clone(), hash(user_salt.clone())).await
		{
			Ok(LoginStatus::USER_CONNECTED) => {
				self.userSalt = Some(user_salt);
				self.generatedId = Some(generatedId);
				None
			},
			Ok(LoginStatus::SERVER_ERROR) => Some("FRONTERROR_SERVER_ERROR".to_string()),
			Ok(reason) => Some(AllFrontLoginEnum::fromLoginStatus(reason).to_string()),
			Err(_) => Some("FRONTERROR_SERVER_ERROR".to_string()),
		};
	}

	/// return None if the user is correctly created connected, else return the error in translated string
	pub async fn login_signUp(&mut self, login: String, pwd: String) -> Option<String>
	{
		let generatedId = hash(login);
		let Ok(SaltReturn::SALT(serverUserSalt)) = API_user_salt(generatedId.clone()).await else {return None};
		let user_salt = Self::pwdHash(pwd, serverUserSalt);

		return match API_user_sign(generatedId, hash(user_salt)).await {
			Ok(LoginStatus::USER_CONNECTED) => None,
			Ok(LoginStatus::SERVER_ERROR) => Some("FRONTERROR_SERVER_ERROR".to_string()),
			Ok(reason) => Some(AllFrontLoginEnum::fromLoginStatus(reason).to_string()),
			Err(_) => Some("FRONTERROR_SERVER_ERROR".to_string()),
		}
	}

	fn derive_key_from_password(password: &str, salt: &[u8]) -> [u8; 32] {
		let hash = argon2::hash_raw(password.as_bytes(), salt, &Config::default()).unwrap_or_default();

		let mut key = [0u8; 32];
		key.copy_from_slice(&hash[..32]);
		key
	}

	pub fn crypt_with_password(&self, plaintext: &String) -> Option<UserDataCypher>
	{
		let Some(password) = &self.userSalt else {return None};
		let Ok(salt) = generate_salt_raw() else {return None};

		// dérive la clé AES-256
		let key_bytes = Self::derive_key_from_password(&password, &salt);
		let Ok(key) = Key::<Aes256Gcm>::try_from(key_bytes);
		let cipher = Aes256Gcm::new(&key);
		let Ok(nonce) = Aes256Gcm::generate_nonce() else {return None};

		// chiffrement
		let Ok(ciphertext) = cipher.encrypt(&nonce, plaintext.as_bytes()) else {return None};

		Some(UserDataCypher {
			salt: Base64::encode_string(&salt),
			nonce: Base64::encode_string(& nonce.as_slice()),
			content: Base64::encode_string(&ciphertext),
		})
	}

	pub fn decrypt_with_password(&self,content: &String) -> Option<String>
	{
		let Ok(content): Result<UserDataCypher,_> = serde_json::from_str(content) else {return None};
		let salt_b64 = content.salt.as_str();
		let nonce_b64 = content.nonce.as_str();
		let ciphertext_b64 = content.content.as_str();

		let Some(password) = &self.userSalt else {return None};
		let Ok(salt) = generate_salt_raw() else {return None};

		let Ok(salt) = Base64::decode_vec(salt_b64) else {return None};
		let Ok(nonce_bytes) = Base64::decode_vec(nonce_b64) else {return None};
		let Ok(ciphertext) = Base64::decode_vec(ciphertext_b64) else {return None};

		let key_bytes = Self::derive_key_from_password(password, &salt);
		let key = Key::<Aes256Gcm>::try_from(key_bytes).unwrap_or_default();
		let cipher = Aes256Gcm::new(&key);
		let nonce = Nonce::try_from(nonce_bytes.as_slice()).unwrap_or_default();

		let Ok(plaintext_bytes) = cipher.decrypt(&nonce, ciphertext.as_ref()) else {return None};
		return match String::from_utf8(plaintext_bytes) {
			Ok(result) => Some(result),
			Err(_) => None
		};
	}


	/// disconnect the user
	pub async fn login_disconnect(&mut self)
	{
		self.userSalt = None;
	}

	pub fn cookie_signalGet() -> (Signal<Option<UserData>>, WriteSignal<Option<UserData>>)
	{
		let time = 24 * 3600 * 1000; // 1 day
		return use_cookie_with_options::<UserData, JsonSerdeCodec>("webhome",UseCookieOptions::default()
			.max_age(time) // one hour
			.same_site(SameSite::Strict)
			.secure(true)
			.path("/"));
	}

	pub fn lang_get_from_cookie() -> Option<String>
	{
		let (userData, setUserData) = UserData::cookie_signalGet();
		let Some(userData) = userData.read().clone() else {return None};
		let Some(login) = userData.login_get() else {return None};

		return Some(login);
	}

	pub fn login_get_from_cookie() -> Option<String>
	{
		let (userData, setUserData) = UserData::cookie_signalGet();
		let Some(userData) = userData.read().clone() else {return None};
		return Some(userData.lang_get());
	}

	pub fn loginLang_get_from_cookie() -> Option<(String,String)>
	{
		let (userData, setUserData) = UserData::cookie_signalGet();
		let Some(userData) = userData.read().clone() else {return None};
		let Some(login) = userData.login_get() else {return None};

		return Some((login,userData.lang_get()));
	}

	fn pwdHash(str: String, salt: String) -> String
	{
		return hash(format!("{}{}",salt,str));
	}
}

impl Default for UserData
{
	fn default() -> Self
	{
		Self {
			lang: "EN".to_string(),
			userSalt: None,
			generatedId: None,
		}
	}
}