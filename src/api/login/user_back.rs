use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use argon2::Config;
use base64ct::{Base64, Encoding};
use Hconfig::Errors;
use Hconfig::HConfig::HConfig;
use Hconfig::HConfigManager::HConfigManager;
use Hconfig::IO::json::WrapperJson;
use Hconfig::tinyjson::JsonValue;
use Htrace::components::level::Level;
use Htrace::HTrace;
use leptos::prelude::{ServerFnError, ServerFnErrorErr};
use leptos_axum::extract;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use time::OffsetDateTime;
use tower_sessions::Session;

use crate::api::login::components::LoginStatusErrors;
use crate::global_security::generate_salt_raw;

const LOGIN_ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy::new("login.attempts", 3, Duration::from_secs(15 * 60));
const SIGN_ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy::new("sign.attempts", 1, Duration::from_secs(24 * 60 * 60));

#[derive(Debug)]
pub(crate) enum UserBackHelperError
{
	HConfigError(Errors),
	ServerError(ServerFnErrorErr),
	SessionError(String),
	CredentialVerifierError(String),
	LoginError(LoginStatusErrors),
}

impl From<UserBackHelperError> for ServerFnError
{
	fn from(value: UserBackHelperError) -> Self
	{
		return match value
		{
			UserBackHelperError::HConfigError(err) => ServerFnError::new(format!("HConfigError: {}", err)),
			UserBackHelperError::ServerError(err) => ServerFnError::new(format!("ServerError: {}", err)),
			UserBackHelperError::SessionError(err) => ServerFnError::new(format!("SessionError: {}", err)),
			UserBackHelperError::CredentialVerifierError(err) => ServerFnError::new(format!("CredentialVerifierError: {}", err)),
			UserBackHelperError::LoginError(err) => ServerFnError::new(format!("LoginError: {}", err)),
		};
	}
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct UserConfigIdentity(String);

impl UserConfigIdentity
{
	fn fromGeneratedId(generatedId: &str) -> Result<Self, UserBackHelperError>
	{
		if (!UserBackHelper::generatedId_isValid(generatedId))
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_INVALID_PWD));
		}

		let mut hasher = Sha3_256::new();
		hasher.update(generatedId);
		let result = hasher.finalize();
		return Ok(Self(Base64::encode_string(&result).replace("/", "LL")));
	}

	fn configName_get(&self) -> &str
	{
		return &self.0;
	}
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AuthenticatedUser
{
	identity: UserConfigIdentity,
}

impl AuthenticatedUser
{
	const SESSION_KEY: &'static str = "auth.user";

	fn new(identity: UserConfigIdentity) -> Self
	{
		return Self { identity };
	}

	pub(crate) async fn current() -> Result<Self, UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::fromSession(&session).await;
	}

	async fn fromSession(session: &Session) -> Result<Self, UserBackHelperError>
	{
		let user = session.get::<Self>(Self::SESSION_KEY).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;

		return user.ok_or(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED));
	}

	async fn establish(session: &Session, identity: UserConfigIdentity) -> Result<(), UserBackHelperError>
	{
		session.cycle_id().await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		session.insert(Self::SESSION_KEY, Self::new(identity)).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		return Ok(());
	}

	pub(crate) async fn logout() -> Result<(), UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::logoutFromSession(&session).await;
	}

	async fn logoutFromSession(session: &Session) -> Result<(), UserBackHelperError>
	{
		session.flush().await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		return Ok(());
	}

	pub(crate) fn userConfig_get(&self) -> Result<HConfig, UserBackHelperError>
	{
		return UserBackHelper::getUserConfigFromIdentity(&self.identity, false);
	}
}

#[derive(Clone, Copy)]
struct AttemptPolicy
{
	sessionKey: &'static str,
	maxAttempts: u8,
	resetAfter: Duration,
}

impl AttemptPolicy
{
	const fn new(sessionKey: &'static str, maxAttempts: u8, resetAfter: Duration) -> Self
	{
		return Self { sessionKey, maxAttempts, resetAfter };
	}

	fn resetSeconds_get(&self) -> i64
	{
		return i64::try_from(self.resetAfter.as_secs()).unwrap_or(i64::MAX);
	}
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct AttemptState
{
	attempts: u8,
	lastAttempt: i64,
}

impl AttemptState
{
	fn active_get(self, policy: AttemptPolicy, now: i64) -> Self
	{
		if (self.lastAttempt == 0 || now.saturating_sub(self.lastAttempt) >= policy.resetSeconds_get())
		{
			return Self::default();
		}
		return self;
	}

	fn attemptRecorded_get(self, now: i64) -> Self
	{
		return Self {
			attempts: self.attempts.saturating_add(1),
			lastAttempt: now,
		};
	}

	fn lockedUntil_get(&self, policy: AttemptPolicy) -> Option<i64>
	{
		if (self.attempts < policy.maxAttempts)
		{
			return None;
		}
		return Some(self.lastAttempt.saturating_add(policy.resetSeconds_get()));
	}
}

struct SessionAttempts
{
	session: Session,
	policy: AttemptPolicy,
	state: AttemptState,
}

impl SessionAttempts
{
	async fn current(policy: AttemptPolicy) -> Result<Self, UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::fromSession(session, policy, OffsetDateTime::now_utc().unix_timestamp()).await;
	}

	async fn fromSession(session: Session, policy: AttemptPolicy, now: i64) -> Result<Self, UserBackHelperError>
	{
		let storedState = session.get::<AttemptState>(policy.sessionKey).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?
			.unwrap_or_default();
		let state = storedState.active_get(policy, now);

		if (storedState != state)
		{
			let _ = session.remove::<AttemptState>(policy.sessionKey).await
				.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		}

		if let Some(lockedUntil) = state.lockedUntil_get(policy)
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::LOCKED(lockedUntil)));
		}

		return Ok(Self { session, policy, state });
	}

	async fn attempt_record(&self, now: i64) -> Result<Option<i64>, UserBackHelperError>
	{
		let state = self.state.attemptRecorded_get(now);
		self.session.insert(self.policy.sessionKey, state).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		return Ok(state.lockedUntil_get(self.policy));
	}

	async fn clear(&self) -> Result<(), UserBackHelperError>
	{
		let _ = self.session.remove::<AttemptState>(self.policy.sessionKey).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		return Ok(());
	}

	fn session_get(&self) -> &Session
	{
		return &self.session;
	}
}

#[derive(Default)]
struct AccountAttemptRegistry
{
	attempts: Mutex<HashMap<UserConfigIdentity, AttemptState>>,
}

impl AccountAttemptRegistry
{
	const MAX_TRACKED_ACCOUNTS: usize = 10_000;

	fn singleton() -> &'static Self
	{
		static SINGLETON: OnceLock<AccountAttemptRegistry> = OnceLock::new();
		return SINGLETON.get_or_init(Self::default);
	}

	fn lockedUntil_get(&self, identity: &UserConfigIdentity, policy: AttemptPolicy, now: i64) -> Result<Option<i64>, UserBackHelperError>
	{
		let mut attempts = self.attempts.lock()
			.map_err(|_| UserBackHelperError::SessionError("account attempt registry lock poisoned".to_string()))?;
		attempts.retain(|_, state| state.active_get(policy, now) != AttemptState::default());
		let state = attempts.get(identity).copied().unwrap_or_default().active_get(policy, now);
		return Ok(state.lockedUntil_get(policy));
	}

	fn attempt_record(&self, identity: &UserConfigIdentity, policy: AttemptPolicy, now: i64) -> Result<Option<i64>, UserBackHelperError>
	{
		let mut attempts = self.attempts.lock()
			.map_err(|_| UserBackHelperError::SessionError("account attempt registry lock poisoned".to_string()))?;
		attempts.retain(|_, state| state.active_get(policy, now) != AttemptState::default());

		if (!attempts.contains_key(identity) && attempts.len() >= Self::MAX_TRACKED_ACCOUNTS)
		{
			return Ok(Some(now.saturating_add(policy.resetSeconds_get())));
		}

		let state = attempts.get(identity).copied().unwrap_or_default()
			.active_get(policy, now)
			.attemptRecorded_get(now);
		attempts.insert(identity.clone(), state);
		return Ok(state.lockedUntil_get(policy));
	}

	fn clear(&self, identity: &UserConfigIdentity) -> Result<(), UserBackHelperError>
	{
		let mut attempts = self.attempts.lock()
			.map_err(|_| UserBackHelperError::SessionError("account attempt registry lock poisoned".to_string()))?;
		attempts.remove(identity);
		return Ok(());
	}
}

#[derive(Debug, Eq, PartialEq)]
enum CredentialVerification
{
	Current,
	Legacy,
	Invalid,
}

struct CredentialVerifier;

impl CredentialVerifier
{
	const CONFIG_FIELD: &'static str = "hashedPwd";
	const FORMAT_PREFIX: &'static str = "$argon2";
	const DUMMY_CREDENTIAL: &'static [u8] = b"webhome-dummy-credential";
	const DUMMY_SALT: &'static [u8] = b"webhome-dummy-salt";

	fn credential_isValid(credential: &str) -> bool
	{
		return Base64::decode_vec(credential).map(|decoded| decoded.len() == 32).unwrap_or(false);
	}

	async fn create(credential: String) -> Result<String, UserBackHelperError>
	{
		let salt = generate_salt_raw()
			.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()))?;
		return tokio::task::spawn_blocking(move || {
			argon2::hash_encoded(credential.as_bytes(), &salt, &Config::default())
		})
			.await
			.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()))?
			.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()));
	}

	async fn verify(storedVerifier: String, credential: String) -> Result<CredentialVerification, UserBackHelperError>
	{
		if (storedVerifier.starts_with(Self::FORMAT_PREFIX))
		{
			let valid = tokio::task::spawn_blocking(move || {
				return argon2::verify_encoded(&storedVerifier, credential.as_bytes()).unwrap_or(false);
			})
				.await
				.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()))?;
			return Ok(if valid { CredentialVerification::Current } else { CredentialVerification::Invalid });
		}

		if (Self::constantTime_equals(&storedVerifier, &credential))
		{
			return Ok(CredentialVerification::Legacy);
		}

		Self::dummyVerify(credential).await?;
		return Ok(CredentialVerification::Invalid);
	}

	async fn dummyVerify(credential: String) -> Result<(), UserBackHelperError>
	{
		static DUMMY_VERIFIER: OnceLock<String> = OnceLock::new();

		tokio::task::spawn_blocking(move || -> Result<(), argon2::Error> {
			if let Some(verifier) = DUMMY_VERIFIER.get()
			{
				let _ = argon2::verify_encoded(verifier, credential.as_bytes())?;
				return Ok(());
			}

			let verifier = argon2::hash_encoded(Self::DUMMY_CREDENTIAL, Self::DUMMY_SALT, &Config::default())?;
			let _ = DUMMY_VERIFIER.set(verifier);
			return Ok(());
		})
			.await
			.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()))?
			.map_err(|err| UserBackHelperError::CredentialVerifierError(err.to_string()))?;
		return Ok(());
	}

	fn constantTime_equals(left: &str, right: &str) -> bool
	{
		let leftHash = Sha3_256::digest(left.as_bytes());
		let rightHash = Sha3_256::digest(right.as_bytes());
		let mut difference = 0u8;
		for (leftByte, rightByte) in leftHash.iter().zip(rightHash.iter())
		{
			difference |= leftByte ^ rightByte;
		}
		return difference == 0;
	}
}

pub(crate) struct UserBackHelper;

impl UserBackHelper
{
	pub(super) fn generatedId_isValid(generatedId: &str) -> bool
	{
		return Base64::decode_vec(generatedId).map(|decoded| decoded.len() == 32).unwrap_or(false);
	}

	/// Check if the user is already created and create it if not.
	pub(crate) async fn signCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<bool, UserBackHelperError>
	{
		let allowRegistration = crate::api::ALLOW_REGISTRATION.get()
			.map(|value| value.load(std::sync::atomic::Ordering::Relaxed))
			.unwrap_or(false);
		if (!allowRegistration)
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::SIGN_DISABLED));
		}
		if (!CredentialVerifier::credential_isValid(&hashedPwd))
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_INVALID_PWD));
		}

		let identity = UserConfigIdentity::fromGeneratedId(&generatedId)?;
		let sessionAttempts = SessionAttempts::current(SIGN_ATTEMPT_POLICY).await?;
		let mut config = Self::getUserConfigFromIdentity(&identity, true)?;
		let alreadyExists = config.value_get("dateSignUp").is_some();

		if (alreadyExists)
		{
			CredentialVerifier::dummyVerify(hashedPwd).await?;
			let _ = sessionAttempts.attempt_record(OffsetDateTime::now_utc().unix_timestamp()).await?;
			return Ok(false);
		}

		let verifier = CredentialVerifier::create(hashedPwd).await?;
		config.value_set("dateSignUp", JsonValue::String(format!("{}", OffsetDateTime::now_utc())));
		config.value_set(CredentialVerifier::CONFIG_FIELD, JsonValue::String(verifier));
		config.file_save().map_err(UserBackHelperError::HConfigError)?;
		let _ = sessionAttempts.attempt_record(OffsetDateTime::now_utc().unix_timestamp()).await?;
		return Ok(true);
	}

	/// Verify the credential and establish the authenticated server session.
	pub(crate) async fn loginCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<(), UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::loginCheckAndCreateFromSession(generatedId, hashedPwd, session, OffsetDateTime::now_utc().unix_timestamp()).await;
	}

	async fn loginCheckAndCreateFromSession(generatedId: String, hashedPwd: String, session: Session, now: i64) -> Result<(), UserBackHelperError>
	{
		if (!CredentialVerifier::credential_isValid(&hashedPwd))
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_INVALID_PWD));
		}

		let identity = UserConfigIdentity::fromGeneratedId(&generatedId)?;
		let sessionAttempts = SessionAttempts::fromSession(session, LOGIN_ATTEMPT_POLICY, now).await?;
		if let Some(lockedUntil) = AccountAttemptRegistry::singleton().lockedUntil_get(&identity, LOGIN_ATTEMPT_POLICY, now)?
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::LOCKED(lockedUntil)));
		}

		let mut config = match Self::getUserConfigFromIdentity(&identity, false)
		{
			Ok(config) => config,
			Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_NOT_FOUND)) =>
			{
				CredentialVerifier::dummyVerify(hashedPwd).await?;
				return Err(UserBackHelperError::LoginError(Self::loginFailure_record(&sessionAttempts, &identity, now).await?));
			},
			Err(error) => return Err(error),
		};

		let Some(configVerifier) = config.value_get(CredentialVerifier::CONFIG_FIELD) else
		{
			CredentialVerifier::dummyVerify(hashedPwd).await?;
			return Err(UserBackHelperError::LoginError(Self::loginFailure_record(&sessionAttempts, &identity, now).await?));
		};
		let configVerifier: String = configVerifier.try_into().unwrap_or_default();

		match CredentialVerifier::verify(configVerifier, hashedPwd.clone()).await?
		{
			CredentialVerification::Invalid =>
			{
				return Err(UserBackHelperError::LoginError(Self::loginFailure_record(&sessionAttempts, &identity, now).await?));
			},
			CredentialVerification::Legacy =>
			{
				let verifier = CredentialVerifier::create(hashedPwd).await?;
				config.value_set(CredentialVerifier::CONFIG_FIELD, JsonValue::String(verifier));
				config.file_save().map_err(UserBackHelperError::HConfigError)?;
			},
			CredentialVerification::Current => {},
		}

		sessionAttempts.clear().await?;
		AccountAttemptRegistry::singleton().clear(&identity)?;
		AuthenticatedUser::establish(sessionAttempts.session_get(), identity).await?;
		return Ok(());
	}

	async fn loginFailure_record(sessionAttempts: &SessionAttempts, identity: &UserConfigIdentity, now: i64) -> Result<LoginStatusErrors, UserBackHelperError>
	{
		let sessionLockedUntil = sessionAttempts.attempt_record(now).await?;
		let accountLockedUntil = AccountAttemptRegistry::singleton().attempt_record(identity, LOGIN_ATTEMPT_POLICY, now)?;
		let lockedUntil = sessionLockedUntil.into_iter().chain(accountLockedUntil).max();
		return Ok(match lockedUntil
		{
			Some(timestamp) => LoginStatusErrors::LOCKED(timestamp),
			None => LoginStatusErrors::USER_INVALID_PWD,
		});
	}

	fn getUserConfigFromIdentity(identity: &UserConfigIdentity, createIfAbsent: bool) -> Result<HConfig, UserBackHelperError>
	{
		let usersPath = format!("{}/users", HConfigManager::singleton().confPath_get());
		let filepath = format!("{}/{}.json", usersPath, identity.configName_get());
		if (!createIfAbsent && !Path::new(&filepath).is_file())
		{
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_NOT_FOUND));
		}

		return HConfig::new::<WrapperJson>(identity.configName_get().to_string(), usersPath)
			.map_err(|err| {
				HTrace!((Level::ERROR) "UserBackHelper::getUserConfig : {}", err);
				return UserBackHelperError::HConfigError(err);
			});
	}
}

#[cfg(test)]
mod tests
{
	use std::path::PathBuf;
	use std::sync::{Arc, Mutex, MutexGuard, Once};

	use axum::body::{to_bytes, Body};
	use axum::extract::Path as AxumPath;
	use axum::http::header::{ACCEPT, CONTENT_TYPE, COOKIE, SET_COOKIE};
	use axum::http::{Request, Response, StatusCode};
	use axum::routing::{get, post};
	use axum::Router;
	use leptos::server_fn::codec::{Json, PostUrl};
	use leptos::server_fn::{ContentType, ServerFn};
	use serde::de::DeserializeOwned;
	use tower::ServiceExt;
	use tower_sessions::{MemoryStore, Session, SessionStore};

	use crate::api::modules::components::{ModuleContent, ModuleID};
	use crate::api::modules::{ApiModuleRetrieve, ApiModuleUpdate, ModuleApiError, ModuleReturnRetrieve};
	use crate::api::Htrace::ApiHtraceLog;
	use crate::api::proxys::imap::ApiProxysImapListbox;
	use crate::api::proxys::imap_components::imap_connector;
	use crate::api::proxys::imap_error::ImapError;
	use crate::api::proxys::wget::{ApiProxysWget, proxys_return};

	use super::*;

	static CONFIG_PATH_LOCK: Mutex<()> = Mutex::new(());
	static TRACE_INITIALIZATION: Once = Once::new();

	struct ConfigPathGuard
	{
		_lock: MutexGuard<'static, ()>,
		previousPath: String,
		testPath: PathBuf,
	}

	impl ConfigPathGuard
	{
		fn new() -> Self
		{
			let lock = CONFIG_PATH_LOCK.lock().unwrap_or_else(|err| err.into_inner());
			let previousPath = HConfigManager::singleton().confPath_get();
			let testPath = std::env::temp_dir().join(format!("webhome-auth-test-{}", uuid::Uuid::new_v4()));
			std::fs::create_dir_all(testPath.join("users")).unwrap();
			HConfigManager::singleton().confPath_set(testPath.to_string_lossy().to_string());
			return Self { _lock: lock, previousPath, testPath };
		}
	}

	impl Drop for ConfigPathGuard
	{
		fn drop(&mut self)
		{
			HConfigManager::singleton().confPath_set(self.previousPath.clone());
			let _ = std::fs::remove_dir_all(&self.testPath);
		}
	}

	fn validGeneratedId_get() -> String
	{
		return Base64::encode_string(&[7u8; 32]);
	}

	fn validCredential_get() -> String
	{
		return Base64::encode_string(&[11u8; 32]);
	}

	struct ModuleAuthorizationTest;

	impl ModuleAuthorizationTest
	{
		async fn authenticate(AxumPath(seed): AxumPath<u8>, session: Session) -> StatusCode
		{
			let generatedId = Base64::encode_string(&[seed; 32]);
			let Ok(identity) = UserConfigIdentity::fromGeneratedId(&generatedId)
			else
			{
				return StatusCode::INTERNAL_SERVER_ERROR;
			};
			let Ok(mut config) = UserBackHelper::getUserConfigFromIdentity(&identity, true)
			else
			{
				return StatusCode::INTERNAL_SERVER_ERROR;
			};
			config.value_set("dateSignUp", JsonValue::String("module authorization test".to_string()));
			if (config.file_save().is_err())
			{
				return StatusCode::INTERNAL_SERVER_ERROR;
			}
			if (AuthenticatedUser::establish(&session, identity).await.is_err())
			{
				return StatusCode::INTERNAL_SERVER_ERROR;
			}
			return StatusCode::NO_CONTENT;
		}

		fn router_get() -> Router
		{
			TRACE_INITIALIZATION.call_once(|| {
				Htrace::htracer::HTracer::globalContext_set(Htrace::components::context::Context::default());
			});
			return Router::new()
				.route("/test/auth/{seed}", get(Self::authenticate))
				.route(ApiModuleUpdate::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiModuleRetrieve::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiHtraceLog::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiProxysWget::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiProxysImapListbox::PATH, post(leptos_axum::handle_server_fns))
				.layer(tower_sessions::SessionManagerLayer::new(MemoryStore::default()));
		}

		fn serverRequest_get(path: &str, body: String, cookie: Option<&str>) -> Request<Body>
		{
			let mut request = Request::builder()
				.method("POST")
				.uri(path)
				.header(CONTENT_TYPE, PostUrl::CONTENT_TYPE)
				.header(ACCEPT, Json::CONTENT_TYPE);
			if let Some(cookie) = cookie
			{
				request = request.header(COOKIE, cookie);
			}
			return request.body(Body::from(body)).unwrap();
		}

		async fn cookie_get(router: &Router, seed: u8) -> String
		{
			let response = router.clone().oneshot(
				Request::builder()
					.uri(format!("/test/auth/{}", seed))
					.body(Body::empty())
					.unwrap()
			).await.unwrap();
			assert_eq!(response.status(), StatusCode::NO_CONTENT);
			return response.headers().get(SET_COOKIE).unwrap().to_str().unwrap()
				.split(';').next().unwrap().to_string();
		}

		async fn responseJson_get<T>(response: Response<Body>) -> T
		where
			T: DeserializeOwned,
		{
			let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
			return serde_json::from_slice(&body).unwrap();
		}

		async fn responseText_get(response: Response<Body>) -> String
		{
			let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
			return String::from_utf8(body.to_vec()).unwrap();
		}

		async fn module_update(router: &Router, cookie: Option<&str>, content: ModuleContent) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("content[id]", &content.id.id);
			body.append_pair("content[typeModule]", &content.typeModule);
			body.append_pair("content[timestamp]", &content.timestamp.to_string());
			body.append_pair("content[content]", &content.content);
			body.append_pair("content[pos][0]", &content.pos[0].to_string());
			body.append_pair("content[pos][1]", &content.pos[1].to_string());
			body.append_pair("content[size][0]", &content.size[0].to_string());
			body.append_pair("content[size][1]", &content.size[1].to_string());
			body.append_pair("content[depth]", &content.depth.to_string());
			body.append_pair("overwrite", "true");
			return router.clone().oneshot(Self::serverRequest_get(
				ApiModuleUpdate::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn module_retrieve(router: &Router, cookie: Option<&str>, moduleId: ModuleID) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("moduleData[key]", &moduleId.id);
			body.append_pair("moduleData[timestamp]", "0");
			return router.clone().oneshot(Self::serverRequest_get(
				ApiModuleRetrieve::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn proxy_wget(router: &Router, cookie: Option<&str>, url: &str) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("url", url);
			return router.clone().oneshot(Self::serverRequest_get(
				ApiProxysWget::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn proxy_imap_listbox(router: &Router, cookie: Option<&str>, config: &imap_connector) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("config[host]", &config.host);
			body.append_pair("config[port]", &config.port.to_string());
			body.append_pair("config[username]", &config.username);
			body.append_pair("config[password]", &config.password);
			return router.clone().oneshot(Self::serverRequest_get(
				ApiProxysImapListbox::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn trace_log(router: &Router, cookie: Option<&str>) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("content", "trace content must not be accepted anonymously");
			body.append_pair("htype", "DEBUG");
			body.append_pair("file", "anonymous-test.rs");
			body.append_pair("line", "1");
			return router.clone().oneshot(Self::serverRequest_get(
				ApiHtraceLog::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}
	}

	#[test]
	fn attemptState_resetsOnlyAfterPositiveElapsedDelay()
	{
		let policy = AttemptPolicy::new("test", 3, Duration::from_secs(60));
		let state = AttemptState { attempts: 2, lastAttempt: 1_000 };

		assert_eq!(state.active_get(policy, 1_059), state);
		assert_eq!(state.active_get(policy, 1_060), AttemptState::default());
		assert_eq!(state.active_get(policy, 900), state);
	}

	#[test]
	fn attemptState_returnsUnlockTimestampAtLimit()
	{
		let policy = AttemptPolicy::new("test", 3, Duration::from_secs(60));
		let state = AttemptState { attempts: 3, lastAttempt: 1_000 };

		assert_eq!(state.lockedUntil_get(policy), Some(1_060));
	}

	#[test]
	fn accountAttempts_cannotBeBypassedByChangingSession()
	{
		let registry = AccountAttemptRegistry::default();
		let identity = UserConfigIdentity::fromGeneratedId(&validGeneratedId_get()).unwrap();

		assert_eq!(registry.attempt_record(&identity, LOGIN_ATTEMPT_POLICY, 1_000).unwrap(), None);
		assert_eq!(registry.attempt_record(&identity, LOGIN_ATTEMPT_POLICY, 1_001).unwrap(), None);
		assert_eq!(registry.attempt_record(&identity, LOGIN_ATTEMPT_POLICY, 1_002).unwrap(), Some(1_902));
		assert_eq!(registry.lockedUntil_get(&identity, LOGIN_ATTEMPT_POLICY, 1_003).unwrap(), Some(1_902));
		assert_eq!(registry.lockedUntil_get(&identity, LOGIN_ATTEMPT_POLICY, 1_902).unwrap(), None);
	}

	#[test]
	fn authenticatedUser_cyclesAndInvalidatesPreviousSession()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let store = Arc::new(MemoryStore::default());
			let session = Session::new(None, store.clone(), None);
			session.insert("guest.value", 42u8).await.unwrap();
			session.save().await.unwrap();
			let previousId = session.id().unwrap();
			let identity = UserConfigIdentity::fromGeneratedId(&validGeneratedId_get()).unwrap();

			AuthenticatedUser::establish(&session, identity.clone()).await.unwrap();
			session.save().await.unwrap();
			let authenticatedId = session.id().unwrap();

			assert_ne!(previousId, authenticatedId);
			assert!(store.load(&previousId).await.unwrap().is_none());
			let restored = Session::new(Some(authenticatedId), store, None);
			assert_eq!(AuthenticatedUser::fromSession(&restored).await.unwrap(), AuthenticatedUser::new(identity));
		});
	}

	#[test]
	fn authenticatedUser_logoutFlushesServerSession()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let store = Arc::new(MemoryStore::default());
			let session = Session::new(None, store.clone(), None);
			let identity = UserConfigIdentity::fromGeneratedId(&validGeneratedId_get()).unwrap();
			AuthenticatedUser::establish(&session, identity).await.unwrap();
			session.save().await.unwrap();
			let authenticatedId = session.id().unwrap();

			AuthenticatedUser::logoutFromSession(&session).await.unwrap();

			assert!(session.id().is_none());
			assert!(store.load(&authenticatedId).await.unwrap().is_none());
		});
	}

	#[test]
	fn moduleApis_rejectAnonymousAndIsolateAuthenticatedAccounts()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let configPath = ConfigPathGuard::new();
			let router = ModuleAuthorizationTest::router_get();
			let moduleId = ModuleID { id: "shared-module".to_string() };
			let contentA = ModuleContent {
				id: moduleId.clone(),
				typeModule: "TEST".to_string(),
				timestamp: 100,
				content: "ciphertext-account-a".to_string(),
				pos: [1, 2],
				size: [3, 4],
				depth: 5,
			};

			let anonymousUpdate = ModuleAuthorizationTest::module_update(&router, None, contentA.clone()).await;
			assert_eq!(anonymousUpdate.status(), StatusCode::INTERNAL_SERVER_ERROR);
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<ModuleApiError>(anonymousUpdate).await,
				ModuleApiError::AUTH_REQUIRED
			);
			assert_eq!(std::fs::read_dir(configPath.testPath.join("users")).unwrap().count(), 0);
			let anonymousRetrieve = ModuleAuthorizationTest::module_retrieve(&router, None, moduleId.clone()).await;
			assert_eq!(anonymousRetrieve.status(), StatusCode::INTERNAL_SERVER_ERROR);

			let cookieA = ModuleAuthorizationTest::cookie_get(&router, 7).await;
			let cookieB = ModuleAuthorizationTest::cookie_get(&router, 9).await;
			let updateA = ModuleAuthorizationTest::module_update(&router, Some(&cookieA), contentA.clone()).await;
			assert_eq!(updateA.status(), StatusCode::OK);

			let retrieveBeforeWriteB = ModuleAuthorizationTest::module_retrieve(&router, Some(&cookieB), moduleId.clone()).await;
			assert_eq!(retrieveBeforeWriteB.status(), StatusCode::OK);
			assert!(matches!(
				ModuleAuthorizationTest::responseJson_get::<ModuleReturnRetrieve>(retrieveBeforeWriteB).await,
				ModuleReturnRetrieve::SAME
			));

			let mut contentB = contentA.clone();
			contentB.timestamp = 200;
			contentB.content = "ciphertext-account-b".to_string();
			let updateB = ModuleAuthorizationTest::module_update(&router, Some(&cookieB), contentB.clone()).await;
			assert_eq!(updateB.status(), StatusCode::OK);

			let retrieveA = ModuleAuthorizationTest::module_retrieve(&router, Some(&cookieA), moduleId.clone()).await;
			let ModuleReturnRetrieve::UPDATED(retrievedA) = ModuleAuthorizationTest::responseJson_get(retrieveA).await
			else
			{
				panic!("account A did not retrieve its module");
			};
			assert_eq!(retrievedA.timestamp, contentA.timestamp);
			assert_eq!(retrievedA.content, contentA.content);

			let retrieveB = ModuleAuthorizationTest::module_retrieve(&router, Some(&cookieB), moduleId).await;
			let ModuleReturnRetrieve::UPDATED(retrievedB) = ModuleAuthorizationTest::responseJson_get(retrieveB).await
			else
			{
				panic!("account B did not retrieve its module");
			};
			assert_eq!(retrievedB.timestamp, contentB.timestamp);
			assert_eq!(retrievedB.content, contentB.content);
		});
	}

	#[test]
	fn proxyApis_requireAuthenticationBeforeDestinationAccess()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let router = ModuleAuthorizationTest::router_get();
			let imapConfig = imap_connector {
				host: "127.0.0.1".to_string(),
				port: 993,
				username: "not-used".to_string(),
				password: "not-used".to_string(),
				extra: None,
			};

			let anonymousRss = ModuleAuthorizationTest::proxy_wget(&router, None, "http://127.0.0.1/feed").await;
			assert_eq!(anonymousRss.status(), StatusCode::INTERNAL_SERVER_ERROR);
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<proxys_return>(anonymousRss).await,
				proxys_return::AUTH_REQUIRED,
			);

			let anonymousImap = ModuleAuthorizationTest::proxy_imap_listbox(&router, None, &imapConfig).await;
			assert_eq!(anonymousImap.status(), StatusCode::INTERNAL_SERVER_ERROR);
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<ImapError>(anonymousImap).await,
				ImapError::AUTH_REQUIRED,
			);
			let anonymousTrace = ModuleAuthorizationTest::trace_log(&router, None).await;
			assert_eq!(anonymousTrace.status(), StatusCode::INTERNAL_SERVER_ERROR);
			assert!(ModuleAuthorizationTest::responseText_get(anonymousTrace).await.contains("Authentication required"));

			let cookie = ModuleAuthorizationTest::cookie_get(&router, 13).await;
			let authenticatedRss = ModuleAuthorizationTest::proxy_wget(&router, Some(&cookie), "http://127.0.0.1/feed").await;
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<proxys_return>(authenticatedRss).await,
				proxys_return::DESTINATION_FORBIDDEN,
			);
			let authenticatedImap = ModuleAuthorizationTest::proxy_imap_listbox(&router, Some(&cookie), &imapConfig).await;
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<ImapError>(authenticatedImap).await,
				ImapError::DESTINATION_FORBIDDEN,
			);
		});
	}

	#[test]
	fn credentialVerifier_acceptsCurrentAndLegacyFormats()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let credential = validCredential_get();
			let verifier = CredentialVerifier::create(credential.clone()).await.unwrap();

			assert!(verifier.starts_with(CredentialVerifier::FORMAT_PREFIX));
			assert_eq!(CredentialVerifier::verify(verifier, credential.clone()).await.unwrap(), CredentialVerification::Current);
			assert_eq!(CredentialVerifier::verify(credential.clone(), credential).await.unwrap(), CredentialVerification::Legacy);
		});
	}

	#[test]
	fn legacyVerifier_isMigratedDuringSuccessfulLogin()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let generatedId = validGeneratedId_get();
			let credential = validCredential_get();
			let identity = UserConfigIdentity::fromGeneratedId(&generatedId).unwrap();
			let mut config = UserBackHelper::getUserConfigFromIdentity(&identity, true).unwrap();
			config.value_set("dateSignUp", JsonValue::String("legacy".to_string()));
			config.value_set(CredentialVerifier::CONFIG_FIELD, JsonValue::String(credential.clone()));
			config.file_save().unwrap();
			drop(config);

			let store = Arc::new(MemoryStore::default());
			let session = Session::new(None, store, None);
			UserBackHelper::loginCheckAndCreateFromSession(generatedId, credential.clone(), session.clone(), 1_000).await.unwrap();

			let migratedConfig = UserBackHelper::getUserConfigFromIdentity(&identity, false).unwrap();
			let migratedVerifier: String = migratedConfig.value_get(CredentialVerifier::CONFIG_FIELD).unwrap().try_into().unwrap();
			assert!(migratedVerifier.starts_with(CredentialVerifier::FORMAT_PREFIX));
			assert_eq!(CredentialVerifier::verify(migratedVerifier, credential).await.unwrap(), CredentialVerification::Current);
			assert_eq!(AuthenticatedUser::fromSession(&session).await.unwrap(), AuthenticatedUser::new(identity));
		});
	}
}
