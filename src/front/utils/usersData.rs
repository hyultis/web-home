use base64ct::{Base64, Encoding};
use leptos::prelude::codee::string::JsonSerdeCodec;
use leptos::prelude::{Signal, WriteSignal};
use leptos_use::{use_cookie_with_options, SameSite, UseCookieOptions};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use crate::api::login::{API_user_disconnect, API_user_login, API_user_sign, LoginStatus};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserData
{
	lang: String,
	isConnected: bool,
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

	pub fn login_isConnected(&self) -> bool
	{
		return self.isConnected;
	}

	pub async fn login_set(&mut self, login: String, pwd: String) -> Option<String>
	{
		let generatedId = Self::innerHash(login);
		let hashedPwd = Self::innerHash(pwd);

		return match API_user_login(generatedId.clone(), hashedPwd.clone()).await
		{
			Ok(LoginStatus::USER_CONNECTED) => {
				self.isConnected = true;
				None
			},
			Ok(LoginStatus::SERVER_ERROR) => Some("SERVER_ERROR".to_string()),
			Ok(reason) => Some(format!("LOGIN_{}", reason)),
			Err(_) => Some("SERVER_ERROR".to_string()),
		};
	}

	/// return None if the user is correctly created connected, else return the error in translated string
	pub async fn login_signUp(&mut self, login: String, pwd: String) -> Option<String>
	{
		let generatedId = Self::innerHash(login);
		let hashedPwd = Self::innerHash(pwd);

		return match API_user_sign(generatedId, hashedPwd).await {
			Ok(LoginStatus::USER_CONNECTED) => None,
			Ok(LoginStatus::SERVER_ERROR) => Some("SERVER_ERROR".to_string()),
			Ok(reason) => Some(format!("LOGIN_{}", reason)),
			Err(_) => Some("SERVER_ERROR".to_string()),
		}
	}

	pub fn login_force_connect(&mut self)
	{
		self.isConnected = true;
	}


	/// disconnect the user
	pub async fn login_disconnect(&mut self) -> Option<String>
	{
		if(!self.isConnected)
		{
			return None;
		}

		return match API_user_disconnect().await {
			Ok(LoginStatus::USER_DISCONNECTED) => None,
			Ok(LoginStatus::SERVER_ERROR) => Some("SERVER_ERROR".to_string()),
			Ok(reason) => Some(format!("LOGIN_{}", reason)),
			Err(_) => Some("SERVER_ERROR".to_string()),
		}
	}

	pub fn cookie_signalGet() -> (Signal<Option<UserData>>, WriteSignal<Option<UserData>>)
	{
		return use_cookie_with_options::<UserData, JsonSerdeCodec>("webhome",UseCookieOptions::default()
			.max_age(3600_000) // one hour
			.same_site(SameSite::Lax)
			.secure(true)
			.path("/"));
	}

	fn innerHash(str: String) -> String
	{
		let mut hasher = Sha3_256::new();
		hasher.update(str);
		let result = hasher.finalize();
		return Base64::encode_string(&result)
	}
}

impl Default for UserData
{
	fn default() -> Self
	{
		Self {
			lang: "EN".to_string(),
			isConnected: false,
		}
	}
}