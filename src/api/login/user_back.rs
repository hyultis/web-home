use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use async_lock::{Mutex as AsyncMutex, MutexGuardArc};
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

use crate::api::login::components::{AccountPreferencesError, LoginStatusErrors, PasswordRotationContent, PasswordRotationError, PasswordRotationFinalize, PasswordRotationSnapshot};
use crate::api::modules::components::ModuleContent;
use crate::global_security::generate_salt_raw;

const LOGIN_ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy::new("login.attempts", 3, Duration::from_secs(15 * 60));
const SIGN_ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy::new("sign.attempts", 1, Duration::from_secs(24 * 60 * 60));
pub(crate) const PASSWORD_ROTATION_REQUEST_MAXIMUM_BYTES: usize = 72 * 1024 * 1024;

#[derive(Debug)]
pub(crate) enum UserBackHelperError
{
	HConfigError(Errors),
	ServerError(ServerFnErrorErr),
	SessionError(String),
	CredentialRotationInProgress,
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
			UserBackHelperError::CredentialRotationInProgress => ServerFnError::new("Credential rotation in progress"),
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

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AuthenticatedUser
{
	identity: UserConfigIdentity,
	#[serde(default)]
	credentialSalt: Option<String>,
	#[serde(default)]
	credentialVersion: u64,
	#[serde(default)]
	passwordRotationId: Option<String>,
}

impl std::fmt::Debug for AuthenticatedUser
{
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("AuthenticatedUser")
			.field("identity",&"[REDACTED]")
			.field("credentialSalt",&self.credentialSalt.as_ref().map(|_| "[REDACTED]"))
			.field("credentialVersion",&self.credentialVersion)
			.field("passwordRotationPending",&self.passwordRotationId.is_some())
			.finish();
	}
}

impl AuthenticatedUser
{
	const SESSION_KEY: &'static str = "auth.user";

	fn new(identity: UserConfigIdentity, credentialSalt: String, credentialVersion: u64) -> Self
	{
		return Self {
			identity,
			credentialSalt: Some(credentialSalt),
			credentialVersion,
			passwordRotationId: None,
		};
	}

	pub(crate) async fn current() -> Result<Self, UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		let (user, _) = Self::fromSessionWithConfig(&session).await?;
		return Ok(user);
	}

	async fn fromSession(session: &Session) -> Result<Self, UserBackHelperError>
	{
		let user = session.get::<Self>(Self::SESSION_KEY).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;

		return user.ok_or(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED));
	}

	pub(crate) async fn currentWithConfig() -> Result<(Self,HConfig), UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::fromSessionWithConfig(&session).await;
	}

	async fn fromSessionWithConfig(session: &Session) -> Result<(Self,HConfig), UserBackHelperError>
	{
		let user = Self::fromSession(session).await?;
		let config = user.userConfig_get()?;
		if (!user.credentialVersion_isCurrent(&config))
		{
			let _ = session.flush().await;
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED));
		}
		return Ok((user,config));
	}

	pub(super) async fn session_isAuthenticated(session: &Session) -> Result<bool, UserBackHelperError>
	{
		let user = match Self::fromSession(session).await
		{
			Ok(user) => user,
			Err(UserBackHelperError::LoginError(_)) => return Ok(false),
			Err(error) => return Err(error),
		};
		let config = match user.userConfig_get()
		{
			Ok(config) => config,
			Err(UserBackHelperError::LoginError(_)) => return Ok(false),
			Err(error) => return Err(error),
		};
		// This middleware-only check must not flush a stale session. A final
		// password-rotation response can fail after the atomic file save; keeping
		// its rotation id lets the exact pending request recover through the receipt.
		return Ok(user.credentialVersion_isCurrent(&config));
	}

	pub(crate) async fn session_passwordRotationBody_isAllowed(session: &Session) -> bool
	{
		let Ok(user) = Self::fromSession(session).await else {return false};
		let Ok(config) = user.userConfig_get() else {return false};
		return user.credentialVersion_isCurrent(&config)
			|| user.passwordRotationId.as_deref()
				.is_some_and(|rotationId| UserBackHelper::passwordRotationReceiptId_matches(&config,rotationId));
	}

	async fn establish(session: &Session, identity: UserConfigIdentity, credentialSalt: String, credentialVersion: u64) -> Result<(), UserBackHelperError>
	{
		session.cycle_id().await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		session.insert(Self::SESSION_KEY, Self::new(identity,credentialSalt,credentialVersion)).await
			.map_err(|err| UserBackHelperError::SessionError(err.to_string()))?;
		return Ok(());
	}

	async fn session_update(&self, session: &Session) -> Result<(), UserBackHelperError>
	{
		session.insert(Self::SESSION_KEY,self.clone()).await
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

	fn credentialVersion_isCurrent(&self, config: &HConfig) -> bool
	{
		return self.credentialVersion == UserBackHelper::credentialVersion_get(config);
	}

	pub(crate) async fn mutation_begin() -> Result<AuthenticatedUserMutation, UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		return Self::mutation_beginFromSession(session).await;
	}

	async fn mutation_beginFromSession(session: Session) -> Result<AuthenticatedUserMutation, UserBackHelperError>
	{
		let requestUser = Self::fromSession(&session).await?;
		let guard = UserMutationRegistry::singleton().lock_get(&requestUser.identity)?.lock_arc().await;
		let currentUser = Self::fromSession(&session).await?;
		if (currentUser != requestUser)
		{
			if (currentUser.identity == requestUser.identity
				&& (currentUser.passwordRotationId.is_some()
					|| requestUser.passwordRotationId.is_some()
					|| currentUser.credentialVersion != requestUser.credentialVersion))
			{
				return Err(UserBackHelperError::CredentialRotationInProgress);
			}
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED));
		}
		let config = requestUser.userConfig_get()?;
		if (!requestUser.credentialVersion_isCurrent(&config))
		{
			if (currentUser.passwordRotationId.as_deref()
				.is_some_and(|rotationId| UserBackHelper::passwordRotationReceiptId_matches(&config,rotationId)))
			{
				return Err(UserBackHelperError::CredentialRotationInProgress);
			}
			else
			{
				let _ = session.flush().await;
			}
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED));
		}
		return Ok(AuthenticatedUserMutation {
			_config: config,
			_guard: guard,
		});
	}
}

pub(crate) struct AuthenticatedUserMutation
{
	_config: HConfig,
	_guard: MutexGuardArc<()>,
}

impl AuthenticatedUserMutation
{
	pub(crate) fn config_getMut(&mut self) -> &mut HConfig
	{
		return &mut self._config;
	}
}

#[derive(Default)]
struct UserMutationRegistry
{
	locks: Mutex<HashMap<UserConfigIdentity,Weak<AsyncMutex<()>>>>,
}

impl UserMutationRegistry
{
	fn singleton() -> &'static Self
	{
		static SINGLETON: OnceLock<UserMutationRegistry> = OnceLock::new();
		return SINGLETON.get_or_init(Self::default);
	}

	fn lock_get(&self, identity: &UserConfigIdentity) -> Result<Arc<AsyncMutex<()>>, UserBackHelperError>
	{
		let mut locks = self.locks.lock()
			.map_err(|_| UserBackHelperError::SessionError("user mutation registry lock poisoned".to_string()))?;
		locks.retain(|_,lock| lock.strong_count() > 0);
		if let Some(lock) = locks.get(identity).and_then(Weak::upgrade)
		{
			return Ok(lock);
		}
		let lock = Arc::new(AsyncMutex::new(()));
		locks.insert(identity.clone(),Arc::downgrade(&lock));
		return Ok(lock);
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

#[derive(Deserialize, Serialize)]
struct PasswordRotationReceipt
{
	rotationId: String,
	credentialVersion: u64,
	resultDigest: String,
}

pub(crate) struct UserBackHelper;

impl UserBackHelper
{
	const CREDENTIAL_VERSION_FIELD: &'static str = "credentialVersion";
	const ROTATION_RECEIPT_FIELD: &'static str = "passwordRotationReceipt";
	const ACCOUNT_PREFERENCES_FIELD: &'static str = "preferences";
	const ACCOUNT_PREFERENCES_MAXIMUM_BYTES: usize = 16 * 1024;
	const ROTATION_CONTENT_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const ROTATION_TOTAL_MAXIMUM_BYTES: usize = 64 * 1024 * 1024;
	const ROTATION_MODULE_MAXIMUM: usize = 4_096;
	const ROTATION_MODULE_ID_MAXIMUM_BYTES: usize = 512;

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
		let _guard = UserMutationRegistry::singleton().lock_get(&identity)?.lock_arc().await;
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
		Self::credentialVersion_set(&mut config,0);
		config.file_save().map_err(UserBackHelperError::HConfigError)?;
		let _ = sessionAttempts.attempt_record(OffsetDateTime::now_utc().unix_timestamp()).await?;
		return Ok(true);
	}

	/// Verify the credential and establish the authenticated server session.
	pub(crate) async fn loginCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<(), UserBackHelperError>
	{
		let session = extract::<Session>().await.map_err(UserBackHelperError::ServerError)?;
		let credentialSalt = crate::api::login::salt::getSiteSaltForUser(generatedId.clone())
			.ok_or(UserBackHelperError::LoginError(LoginStatusErrors::SALT_INVALID))?;
		return Self::loginCheckAndCreateFromSession(generatedId, hashedPwd, credentialSalt, session, OffsetDateTime::now_utc().unix_timestamp()).await;
	}

	async fn loginCheckAndCreateFromSession(generatedId: String, hashedPwd: String, credentialSalt: String, session: Session, now: i64) -> Result<(), UserBackHelperError>
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

		let _guard = UserMutationRegistry::singleton().lock_get(&identity)?.lock_arc().await;
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
		let credentialVersion = Self::credentialVersion_get(&config);
		AuthenticatedUser::establish(sessionAttempts.session_get(),identity,credentialSalt,credentialVersion).await?;
		return Ok(());
	}

	fn credentialVersion_get(config: &HConfig) -> u64
	{
		return match config.value_get(Self::CREDENTIAL_VERSION_FIELD)
		{
			Some(JsonValue::String(value)) => value.parse().unwrap_or(0),
			Some(JsonValue::Number(value)) if value >= 0.0 => value as u64,
			_ => 0,
		};
	}

	fn credentialVersion_set(config: &mut HConfig, version: u64)
	{
		config.value_set(Self::CREDENTIAL_VERSION_FIELD,JsonValue::String(version.to_string()));
	}

	pub(crate) async fn accountPreferences_get() -> Result<Option<String>, AccountPreferencesError>
	{
		let (_,config) = AuthenticatedUser::currentWithConfig().await.map_err(Self::accountPreferencesError_fromUserBack)?;
		return Self::accountPreferences_getFromConfig(&config);
	}

	pub(crate) async fn accountPreferences_set(content: String) -> Result<(), AccountPreferencesError>
	{
		Self::accountPreferencesContent_validate(&content)?;
		let mut mutation = AuthenticatedUser::mutation_begin().await.map_err(Self::accountPreferencesError_fromUserBack)?;
		let config = mutation.config_getMut();
		config.value_set(Self::ACCOUNT_PREFERENCES_FIELD,JsonValue::String(content));
		return config.file_save().map_err(|_| AccountPreferencesError::SERVER_ERROR);
	}

	fn accountPreferences_getFromConfig(config: &HConfig) -> Result<Option<String>, AccountPreferencesError>
	{
		return match config.value_get(Self::ACCOUNT_PREFERENCES_FIELD)
		{
			None => Ok(None),
			Some(JsonValue::String(content)) => {
				Self::accountPreferencesContent_validate(&content)?;
				Ok(Some(content))
			},
			Some(_) => Err(AccountPreferencesError::CONTENT_INVALID),
		};
	}

	fn accountPreferencesContent_validate(content: &str) -> Result<(), AccountPreferencesError>
	{
		if (content.is_empty() || content.len() > Self::ACCOUNT_PREFERENCES_MAXIMUM_BYTES)
		{
			return Err(AccountPreferencesError::CONTENT_INVALID);
		}
		return Ok(());
	}

	pub(crate) async fn passwordRotation_prepare() -> Result<PasswordRotationSnapshot, PasswordRotationError>
	{
		let session = extract::<Session>().await.map_err(|_| PasswordRotationError::SERVER_ERROR)?;
		return Self::passwordRotation_prepareFromSession(session).await;
	}

	async fn passwordRotation_prepareFromSession(session: Session) -> Result<PasswordRotationSnapshot, PasswordRotationError>
	{
		let requestUser = AuthenticatedUser::fromSession(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		let guard = UserMutationRegistry::singleton().lock_get(&requestUser.identity)
			.map_err(Self::passwordRotationError_fromUserBack)?
			.lock_arc().await;
		let user = AuthenticatedUser::fromSession(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		if (user.identity != requestUser.identity)
		{
			return Err(PasswordRotationError::AUTH_REQUIRED);
		}
		let config = user.userConfig_get().map_err(Self::passwordRotationError_fromUserBack)?;
		if (!user.credentialVersion_isCurrent(&config))
		{
			if (user.passwordRotationId.as_deref()
				.is_some_and(|rotationId| Self::passwordRotationReceiptId_matches(&config,rotationId)))
			{
				return Err(PasswordRotationError::CONFLICT);
			}
			let _ = session.flush().await;
			return Err(PasswordRotationError::AUTH_REQUIRED);
		}
		let credentialSalt = user.credentialSalt.clone().ok_or(PasswordRotationError::REAUTH_REQUIRED)?;
		let storedContents = ModuleContent::retrieveAll(&config).map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		let contents = storedContents.iter()
			.map(|content| PasswordRotationContent {
				id: content.id.clone(),
				content: content.content.clone(),
			})
			.collect::<Vec<_>>();
		Self::passwordRotationContents_validate(&contents)?;
		let preferences = Self::accountPreferences_getFromConfig(&config)
			.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		let revision = Self::passwordRotationRevision_get(user.credentialVersion,&storedContents,preferences.as_deref())?;
		let rotationId = uuid::Uuid::new_v4().to_string();
		drop(guard);

		return Ok(PasswordRotationSnapshot {
			rotationId,
			credentialSalt,
			credentialVersion: user.credentialVersion,
			revision,
			preferences,
			contents,
		});
	}

	pub(crate) async fn passwordRotation_finalize(request: PasswordRotationFinalize) -> Result<(), PasswordRotationError>
	{
		let session = extract::<Session>().await.map_err(|_| PasswordRotationError::SERVER_ERROR)?;
		return Self::passwordRotation_finalizeFromSession(request,session).await;
	}

	async fn passwordRotation_finalizeFromSession(request: PasswordRotationFinalize, session: Session) -> Result<(), PasswordRotationError>
	{
		Self::passwordRotationRequest_validate(&request)?;
		let nextVersion = request.credentialVersion.checked_add(1).ok_or(PasswordRotationError::CONFLICT)?;
		let resultDigest = Self::passwordRotationResultDigest_get(&request)?;
		let mut requestUser = AuthenticatedUser::fromSession(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		let requestConfig = requestUser.userConfig_get().map_err(Self::passwordRotationError_fromUserBack)?;
		let requestCurrentVersion = Self::credentialVersion_get(&requestConfig);
		if (requestUser.credentialVersion != requestCurrentVersion)
		{
			let recoveryIsOwned = requestUser.passwordRotationId.as_deref() == Some(&request.rotationId)
				&& Self::passwordRotationReceipt_matches(&requestConfig,&request.rotationId,nextVersion,&resultDigest);
			if (!recoveryIsOwned)
			{
				let _ = session.flush().await;
				return Err(PasswordRotationError::AUTH_REQUIRED);
			}
		}
		else
		{
			requestUser.passwordRotationId = Some(request.rotationId.clone());
			requestUser.session_update(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		}
		let _guard = UserMutationRegistry::singleton().lock_get(&requestUser.identity)
			.map_err(Self::passwordRotationError_fromUserBack)?
			.lock_arc().await;
		let mut user = AuthenticatedUser::fromSession(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		if (user.identity != requestUser.identity)
		{
			return Err(PasswordRotationError::AUTH_REQUIRED);
		}
		let mut config = user.userConfig_get().map_err(Self::passwordRotationError_fromUserBack)?;
		let currentVersion = Self::credentialVersion_get(&config);

		if (Self::passwordRotationReceipt_matches(&config,&request.rotationId,nextVersion,&resultDigest))
		{
			Self::passwordRotationCredential_require(&config,request.newCredential.clone()).await?;
			user.credentialVersion = currentVersion;
			user.passwordRotationId = None;
			user.session_update(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
			return Ok(());
		}
		if (user.passwordRotationId.as_deref() != Some(&request.rotationId))
		{
			return Err(if user.credentialVersion == currentVersion
			{
				PasswordRotationError::CONFLICT
			}
			else
			{
				PasswordRotationError::AUTH_REQUIRED
			});
		}

		if (user.credentialVersion != currentVersion)
		{
			let _ = session.flush().await;
			return Err(PasswordRotationError::AUTH_REQUIRED);
		}
		if (request.credentialVersion != currentVersion)
		{
			return Err(PasswordRotationError::CONFLICT);
		}

		Self::passwordRotationCredential_require(&config,request.oldCredential.clone()).await?;
		let storedContents = ModuleContent::retrieveAll(&config).map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		let storedPreferences = Self::accountPreferences_getFromConfig(&config)
			.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		let currentRevision = Self::passwordRotationRevision_get(currentVersion,&storedContents,storedPreferences.as_deref())?;
		if (currentRevision != request.revision)
		{
			return Err(PasswordRotationError::CONFLICT);
		}
		Self::passwordRotationContents_match(&storedContents,&request.contents)?;
		if (storedPreferences.is_some() != request.preferences.is_some())
		{
			return Err(PasswordRotationError::CONFLICT);
		}

		let newVerifier = CredentialVerifier::create(request.newCredential.clone()).await
			.map_err(Self::passwordRotationError_fromUserBack)?;
		for content in request.contents.iter()
		{
			ModuleContent::encryptedContent_set(&mut config,&content.id,content.content.clone())
				.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		}
		if let Some(preferences) = request.preferences.clone()
		{
			Self::accountPreferencesContent_validate(&preferences)
				.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
			config.value_set(Self::ACCOUNT_PREFERENCES_FIELD,JsonValue::String(preferences));
		}
		config.value_set(CredentialVerifier::CONFIG_FIELD,JsonValue::String(newVerifier));
		Self::credentialVersion_set(&mut config,nextVersion);
		let receipt = PasswordRotationReceipt {
			rotationId: request.rotationId.clone(),
			credentialVersion: nextVersion,
			resultDigest,
		};
		let receipt = serde_json::to_string(&receipt).map_err(|_| PasswordRotationError::SERVER_ERROR)?;
		config.value_set(Self::ROTATION_RECEIPT_FIELD,JsonValue::String(receipt));
		config.file_save().map_err(|_| PasswordRotationError::SERVER_ERROR)?;

		user.credentialVersion = nextVersion;
		user.passwordRotationId = None;
		user.session_update(&session).await.map_err(Self::passwordRotationError_fromUserBack)?;
		return Ok(());
	}

	async fn passwordRotationCredential_require(config: &HConfig, credential: String) -> Result<(), PasswordRotationError>
	{
		let Some(configVerifier) = config.value_get(CredentialVerifier::CONFIG_FIELD) else {return Err(PasswordRotationError::CURRENT_INVALID)};
		let configVerifier: String = configVerifier.try_into().unwrap_or_default();
		return match CredentialVerifier::verify(configVerifier,credential).await
			.map_err(Self::passwordRotationError_fromUserBack)?
		{
			CredentialVerification::Current | CredentialVerification::Legacy => Ok(()),
			CredentialVerification::Invalid => Err(PasswordRotationError::CURRENT_INVALID),
		};
	}

	fn passwordRotationRequest_validate(request: &PasswordRotationFinalize) -> Result<(), PasswordRotationError>
	{
		if (uuid::Uuid::parse_str(&request.rotationId).is_err()
			|| !CredentialVerifier::credential_isValid(&request.oldCredential)
			|| !CredentialVerifier::credential_isValid(&request.newCredential)
			|| request.oldCredential == request.newCredential
			|| Base64::decode_vec(&request.revision).map(|value| value.len() != 32).unwrap_or(true))
		{
			return Err(PasswordRotationError::CONTENT_INVALID);
		}
		if let Some(preferences) = &request.preferences
		{
			Self::accountPreferencesContent_validate(preferences)
				.map_err(|_| PasswordRotationError::CONTENT_INVALID)?;
		}
		return Self::passwordRotationContents_validate(&request.contents);
	}

	fn passwordRotationContents_validate(contents: &[PasswordRotationContent]) -> Result<(), PasswordRotationError>
	{
		if (contents.len() > Self::ROTATION_MODULE_MAXIMUM)
		{
			return Err(PasswordRotationError::CONTENT_INVALID);
		}
		let mut ids = HashSet::with_capacity(contents.len());
		let mut totalBytes = 0usize;
		for content in contents
		{
			if (content.id.id.is_empty()
				|| content.id.id.len() > Self::ROTATION_MODULE_ID_MAXIMUM_BYTES
				|| content.content.is_empty()
				|| content.content.len() > Self::ROTATION_CONTENT_MAXIMUM_BYTES
				|| !ids.insert(content.id.clone()))
			{
				return Err(PasswordRotationError::CONTENT_INVALID);
			}
			totalBytes = totalBytes.checked_add(content.content.len()).ok_or(PasswordRotationError::CONTENT_INVALID)?;
			if (totalBytes > Self::ROTATION_TOTAL_MAXIMUM_BYTES)
			{
				return Err(PasswordRotationError::CONTENT_INVALID);
			}
		}
		return Ok(());
	}

	fn passwordRotationContents_match(stored: &[ModuleContent], submitted: &[PasswordRotationContent]) -> Result<(), PasswordRotationError>
	{
		if (stored.len() != submitted.len())
		{
			return Err(PasswordRotationError::CONFLICT);
		}
		let submitted = submitted.iter().map(|content| (&content.id,&content.content)).collect::<HashMap<_,_>>();
		for storedContent in stored
		{
			let Some(submittedContent) = submitted.get(&storedContent.id) else {return Err(PasswordRotationError::CONFLICT)};
			let minimumLength = storedContent.content.len().saturating_sub(1_024);
			let maximumLength = storedContent.content.len().saturating_add(1_024);
			if (submittedContent.len() < minimumLength || submittedContent.len() > maximumLength)
			{
				return Err(PasswordRotationError::CONTENT_INVALID);
			}
		}
		return Ok(());
	}

	fn passwordRotationRevision_get(credentialVersion: u64, contents: &[ModuleContent], preferences: Option<&str>) -> Result<String, PasswordRotationError>
	{
		let serialized = serde_json::to_vec(&(credentialVersion,contents,preferences)).map_err(|_| PasswordRotationError::SERVER_ERROR)?;
		return Ok(Base64::encode_string(&Sha3_256::digest(serialized)));
	}

	fn passwordRotationResultDigest_get(request: &PasswordRotationFinalize) -> Result<String, PasswordRotationError>
	{
		let mut request = request.clone();
		request.contents.sort_by(|left,right| left.id.cmp(&right.id));
		let serialized = serde_json::to_vec(&request).map_err(|_| PasswordRotationError::SERVER_ERROR)?;
		return Ok(Base64::encode_string(&Sha3_256::digest(serialized)));
	}

	fn passwordRotationReceipt_matches(config: &HConfig, rotationId: &str, credentialVersion: u64, resultDigest: &str) -> bool
	{
		let Some(receipt) = Self::passwordRotationReceipt_get(config) else {return false};
		return receipt.rotationId == rotationId
			&& receipt.credentialVersion == credentialVersion
			&& receipt.resultDigest == resultDigest
			&& Self::credentialVersion_get(config) == credentialVersion;
	}

	fn passwordRotationReceiptId_matches(config: &HConfig, rotationId: &str) -> bool
	{
		return Self::passwordRotationReceipt_get(config)
			.is_some_and(|receipt| receipt.rotationId == rotationId);
	}

	fn passwordRotationReceipt_get(config: &HConfig) -> Option<PasswordRotationReceipt>
	{
		let Some(JsonValue::String(receipt)) = config.value_get(Self::ROTATION_RECEIPT_FIELD) else {return None};
		return serde_json::from_str(&receipt).ok();
	}

	fn passwordRotationError_fromUserBack(error: UserBackHelperError) -> PasswordRotationError
	{
		return match error
		{
			UserBackHelperError::LoginError(_) => PasswordRotationError::AUTH_REQUIRED,
			_ => PasswordRotationError::SERVER_ERROR,
		};
	}

	fn accountPreferencesError_fromUserBack(error: UserBackHelperError) -> AccountPreferencesError
	{
		return match error
		{
			UserBackHelperError::LoginError(_) => AccountPreferencesError::AUTH_REQUIRED,
			_ => AccountPreferencesError::SERVER_ERROR,
		};
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
	use axum::{middleware, Router};
	use leptos::server_fn::codec::{Json, PostUrl};
	use leptos::server_fn::{ContentType, ServerFn};
	use serde::de::DeserializeOwned;
	use tower::ServiceExt;
	use tower_sessions::{MemoryStore, Session, SessionStore};

	use crate::api::modules::components::{ModuleContent, ModuleID};
	use crate::api::modules::{
		ApiModuleRetrieve,ApiModuleUpdate,ApiModuleUpdateifcurrent,ModuleApiError,
		ModuleReturnRetrieve,ModuleReturnUpdate,
	};
	use crate::api::login::{ApiUserPreferencesGet, ApiUserPreferencesSet};
	use crate::api::login::session::SessionCookie;
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
			trace_initialize();
			let lock = CONFIG_PATH_LOCK.lock().unwrap_or_else(|err| err.into_inner());
			let previousPath = HConfigManager::singleton().confPath_get();
			let testPath = std::env::temp_dir().join(format!("webhome-auth-test-{}", uuid::Uuid::new_v4()));
			std::fs::create_dir_all(testPath.join("users")).unwrap();
			HConfigManager::singleton().confPath_set(testPath.to_string_lossy().to_string());
			return Self { _lock: lock, previousPath, testPath };
		}
	}

	fn trace_initialize()
	{
		TRACE_INITIALIZATION.call_once(|| {
			Htrace::htracer::HTracer::globalContext_set(Htrace::components::context::Context::default());
		});
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

	async fn rotationAccount_create(seed: u8, credential: String, contents: Vec<ModuleContent>) -> (UserConfigIdentity,Session,Session)
	{
		let generatedId = Base64::encode_string(&[seed; 32]);
		let identity = UserConfigIdentity::fromGeneratedId(&generatedId).unwrap();
		let mut config = UserBackHelper::getUserConfigFromIdentity(&identity,true).unwrap();
		let verifier = CredentialVerifier::create(credential).await.unwrap();
		config.value_set("dateSignUp",JsonValue::String("password rotation test".to_string()));
		config.value_set(CredentialVerifier::CONFIG_FIELD,JsonValue::String(verifier));
		UserBackHelper::credentialVersion_set(&mut config,0);
		config.file_save().unwrap();
		for content in contents
		{
			content.update(&mut config,true).unwrap();
		}

		let store = Arc::new(MemoryStore::default());
		let sessionA = Session::new(None,store.clone(),None);
		let sessionB = Session::new(None,store,None);
		AuthenticatedUser::establish(&sessionA,identity.clone(),"test-credential-salt".to_string(),0).await.unwrap();
		AuthenticatedUser::establish(&sessionB,identity.clone(),"test-credential-salt".to_string(),0).await.unwrap();
		return (identity,sessionA,sessionB);
	}

	fn rotationRequest_get(snapshot: PasswordRotationSnapshot, oldCredential: String, newCredential: String) -> PasswordRotationFinalize
	{
		return PasswordRotationFinalize {
			rotationId: snapshot.rotationId,
			credentialVersion: snapshot.credentialVersion,
			revision: snapshot.revision,
			oldCredential,
			newCredential,
			preferences: snapshot.preferences.map(|content| format!("rotated-{}",content)),
			contents: snapshot.contents.into_iter()
				.map(|content| PasswordRotationContent {
					id: content.id,
					content: format!("rotated-{}",content.content),
				})
				.collect(),
		};
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
			if (AuthenticatedUser::establish(&session,identity,"test-credential-salt".to_string(),0).await.is_err())
			{
				return StatusCode::INTERNAL_SERVER_ERROR;
			}
			return StatusCode::NO_CONTENT;
		}

		fn router_get() -> Router
		{
			trace_initialize();
			return Router::new()
				.route("/test/auth/{seed}", get(Self::authenticate))
				.route(ApiUserPreferencesGet::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiUserPreferencesSet::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiModuleUpdate::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiModuleUpdateifcurrent::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiModuleRetrieve::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiHtraceLog::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiProxysWget::PATH, post(leptos_axum::handle_server_fns))
				.route(ApiProxysImapListbox::PATH, post(leptos_axum::handle_server_fns))
				.layer(middleware::from_fn(SessionCookie::serverErrorActivity_renew))
				.layer(SessionCookie::layer_get());
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

		async fn module_updateIfCurrent(
			router: &Router,
			cookie: Option<&str>,
			content: ModuleContent,
			expectedTimestamp: i64,
		) -> Response<Body>
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
			body.append_pair("expectedTimestamp", &expectedTimestamp.to_string());
			return router.clone().oneshot(Self::serverRequest_get(
				ApiModuleUpdateifcurrent::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn accountPreferences_set(router: &Router, cookie: Option<&str>, content: &str) -> Response<Body>
		{
			let mut body = url::form_urlencoded::Serializer::new(String::new());
			body.append_pair("content",content);
			return router.clone().oneshot(Self::serverRequest_get(
				ApiUserPreferencesSet::PATH,
				body.finish(),
				cookie,
			)).await.unwrap();
		}

		async fn accountPreferences_get(router: &Router, cookie: Option<&str>) -> Response<Body>
		{
			return router.clone().oneshot(Self::serverRequest_get(
				ApiUserPreferencesGet::PATH,
				String::new(),
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

			AuthenticatedUser::establish(&session,identity.clone(),"test-credential-salt".to_string(),0).await.unwrap();
			session.save().await.unwrap();
			let authenticatedId = session.id().unwrap();

			assert_ne!(previousId, authenticatedId);
			assert!(store.load(&previousId).await.unwrap().is_none());
			let restored = Session::new(Some(authenticatedId), store, None);
			assert_eq!(AuthenticatedUser::fromSession(&restored).await.unwrap(), AuthenticatedUser::new(identity,"test-credential-salt".to_string(),0));
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
			AuthenticatedUser::establish(&session,identity,"test-credential-salt".to_string(),0).await.unwrap();
			session.save().await.unwrap();
			let authenticatedId = session.id().unwrap();

			AuthenticatedUser::logoutFromSession(&session).await.unwrap();

			assert!(session.id().is_none());
			assert!(store.load(&authenticatedId).await.unwrap().is_none());
		});
	}

	#[test]
	fn accountPreferences_requireAuthenticationAndStayOpaquePerAccount()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let router = ModuleAuthorizationTest::router_get();
			let ciphertext = r#"{"salt":"opaque","nonce":"opaque","content":"opaque"}"#;

			let anonymous = ModuleAuthorizationTest::accountPreferences_set(&router,None,ciphertext).await;
			assert_eq!(anonymous.status(),StatusCode::INTERNAL_SERVER_ERROR);
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<AccountPreferencesError>(anonymous).await,
				AccountPreferencesError::AUTH_REQUIRED,
			);

			let cookieA = ModuleAuthorizationTest::cookie_get(&router,51).await;
			let cookieB = ModuleAuthorizationTest::cookie_get(&router,52).await;
			let saved = ModuleAuthorizationTest::accountPreferences_set(&router,Some(&cookieA),ciphertext).await;
			assert_eq!(saved.status(),StatusCode::OK);
			let accountA = ModuleAuthorizationTest::accountPreferences_get(&router,Some(&cookieA)).await;
			assert_eq!(accountA.status(),StatusCode::OK);
			assert_eq!(
				ModuleAuthorizationTest::responseJson_get::<Option<String>>(accountA).await,
				Some(ciphertext.to_string()),
			);
			let accountB = ModuleAuthorizationTest::accountPreferences_get(&router,Some(&cookieB)).await;
			assert_eq!(accountB.status(),StatusCode::OK);
			assert_eq!(ModuleAuthorizationTest::responseJson_get::<Option<String>>(accountB).await,None);
		});
	}

	#[test]
	fn accountPreferences_rejectEmptyAndOversizedCiphertexts()
	{
		assert_eq!(
			UserBackHelper::accountPreferencesContent_validate(""),
			Err(AccountPreferencesError::CONTENT_INVALID),
		);
		assert!(UserBackHelper::accountPreferencesContent_validate(
			&"x".repeat(UserBackHelper::ACCOUNT_PREFERENCES_MAXIMUM_BYTES)
		).is_ok());
		assert_eq!(
			UserBackHelper::accountPreferencesContent_validate(
				&"x".repeat(UserBackHelper::ACCOUNT_PREFERENCES_MAXIMUM_BYTES + 1)
			),
			Err(AccountPreferencesError::CONTENT_INVALID),
		);
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

			let conditionalId = ModuleID {id: "conditional-module".to_string()};
			let conditionalFirst = ModuleContent {
				id: conditionalId.clone(),
				typeModule: "TEST".to_string(),
				timestamp: 300,
				content: "first-ciphertext".to_string(),
				..Default::default()
			};
			let conditionalCreate = ModuleAuthorizationTest::module_updateIfCurrent(
				&router,Some(&cookieA),conditionalFirst.clone(),i64::MIN,
			).await;
			assert!(matches!(
				ModuleAuthorizationTest::responseJson_get::<ModuleReturnUpdate>(conditionalCreate).await,
				ModuleReturnUpdate::OK,
			));
			let mut competingCreate = conditionalFirst.clone();
			competingCreate.timestamp = 301;
			competingCreate.content = "competing-ciphertext".to_string();
			let conditionalConflict = ModuleAuthorizationTest::module_updateIfCurrent(
				&router,Some(&cookieA),competingCreate,i64::MIN,
			).await;
			let ModuleReturnUpdate::OUTDATED(current) =
				ModuleAuthorizationTest::responseJson_get::<ModuleReturnUpdate>(conditionalConflict).await
			else
			{
				panic!("the second expected-absence write was not rejected");
			};
			assert_eq!(current.timestamp,conditionalFirst.timestamp);
			assert_eq!(current.content,conditionalFirst.content);

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
			assert!(anonymousRss.headers().get(SET_COOKIE).is_none());
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
			assert_eq!(authenticatedRss.status(), StatusCode::INTERNAL_SERVER_ERROR);
			let renewedCookie = authenticatedRss.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
			assert_eq!(renewedCookie.split(';').next().unwrap(), cookie);
			assert!(renewedCookie.contains("Max-Age=86400"));
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
	fn authenticatedUser_legacySessionDefaultsVersionAndRequiresSaltForRotation()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let identity = UserConfigIdentity::fromGeneratedId(&validGeneratedId_get()).unwrap();
			let serialized = serde_json::json!({"identity": identity});
			let legacyUser: AuthenticatedUser = serde_json::from_value(serialized).unwrap();
			assert_eq!(legacyUser.credentialVersion,0);
			assert!(legacyUser.credentialSalt.is_none());

			let mut config = UserBackHelper::getUserConfigFromIdentity(&legacyUser.identity,true).unwrap();
			config.value_set("dateSignUp",JsonValue::String("legacy session".to_string()));
			config.file_save().unwrap();
			let session = Session::new(None,Arc::new(MemoryStore::default()),None);
			session.insert(AuthenticatedUser::SESSION_KEY,legacyUser).await.unwrap();

			assert_eq!(
				UserBackHelper::passwordRotation_prepareFromSession(session).await.err().unwrap(),
				PasswordRotationError::REAUTH_REQUIRED,
			);
		});
	}

	#[test]
	fn passwordRotation_updatesAllContentsKeepsCurrentSessionAndRejectsOthers()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let oldCredential = Base64::encode_string(&[21u8;32]);
			let newCredential = Base64::encode_string(&[22u8;32]);
			let modules = vec![
				ModuleContent {
					id: ModuleID {id: "links".to_string()},
					typeModule: "LINK".to_string(),
					timestamp: 10,
					content: "old-links-ciphertext".to_string(),
					pos: [1,2],
					size: [3,4],
					depth: 5,
				},
				ModuleContent {
					id: ModuleID {id: "unknown-module".to_string()},
					typeModule: "FUTURE_TYPE".to_string(),
					timestamp: 20,
					content: "old-unknown-ciphertext".to_string(),
					pos: [6,7],
					size: [8,9],
					depth: 10,
				},
			];
			let (identity,sessionA,sessionB) = rotationAccount_create(31,oldCredential.clone(),modules.clone()).await;
			let mut config = UserBackHelper::getUserConfigFromIdentity(&identity,false).unwrap();
			config.value_set(UserBackHelper::ACCOUNT_PREFERENCES_FIELD,JsonValue::String("old-preferences-ciphertext".to_string()));
			config.file_save().unwrap();
			drop(config);
			let snapshot = UserBackHelper::passwordRotation_prepareFromSession(sessionA.clone()).await.unwrap();
			assert!(AuthenticatedUser::session_passwordRotationBody_isAllowed(&sessionA).await);
			assert_eq!(snapshot.contents.len(),modules.len());
			assert_eq!(snapshot.preferences.as_deref(),Some("old-preferences-ciphertext"));
			let request = rotationRequest_get(snapshot,oldCredential.clone(),newCredential.clone());
			let rotationId = request.rotationId.clone();

			let mut invalidRequest = request.clone();
			invalidRequest.oldCredential = Base64::encode_string(&[23u8;32]);
			assert_eq!(
				UserBackHelper::passwordRotation_finalizeFromSession(invalidRequest,sessionA.clone()).await.unwrap_err(),
				PasswordRotationError::CURRENT_INVALID,
			);

			UserBackHelper::passwordRotation_finalizeFromSession(request.clone(),sessionA.clone()).await.unwrap();
			let config = UserBackHelper::getUserConfigFromIdentity(&identity,false).unwrap();
			assert_eq!(UserBackHelper::credentialVersion_get(&config),1);
			assert_eq!(
				UserBackHelper::accountPreferences_getFromConfig(&config).unwrap().as_deref(),
				Some("rotated-old-preferences-ciphertext"),
			);
			let stored = ModuleContent::retrieveAll(&config).unwrap();
			assert_eq!(stored.len(),modules.len());
			for (before,after) in modules.iter().zip(stored.iter())
			{
				assert_eq!(after.id,before.id);
				assert_eq!(after.typeModule,before.typeModule);
				assert_eq!(after.timestamp,before.timestamp);
				assert_eq!(after.pos,before.pos);
				assert_eq!(after.size,before.size);
				assert_eq!(after.depth,before.depth);
				assert_eq!(after.content,format!("rotated-{}",before.content));
			}
			let verifier: String = config.value_get(CredentialVerifier::CONFIG_FIELD).unwrap().try_into().unwrap();
			assert_eq!(CredentialVerifier::verify(verifier.clone(),newCredential).await.unwrap(),CredentialVerification::Current);
			assert_eq!(CredentialVerifier::verify(verifier,oldCredential).await.unwrap(),CredentialVerification::Invalid);

			let currentUser = AuthenticatedUser::fromSessionWithConfig(&sessionA).await.unwrap().0;
			assert_eq!(currentUser.credentialVersion,1);
			assert!(currentUser.passwordRotationId.is_none());
			UserBackHelper::passwordRotation_finalizeFromSession(request.clone(),sessionA).await.unwrap();
			let transientSession = Session::new(None,Arc::new(MemoryStore::default()),None);
			let mut transientUser = AuthenticatedUser::new(identity.clone(),"test-credential-salt".to_string(),0);
			transientUser.passwordRotationId = Some(rotationId);
			transientSession.insert(AuthenticatedUser::SESSION_KEY,transientUser).await.unwrap();
			assert!(AuthenticatedUser::session_passwordRotationBody_isAllowed(&transientSession).await);
			assert!(matches!(
				AuthenticatedUser::mutation_beginFromSession(transientSession.clone()).await.err(),
				Some(UserBackHelperError::CredentialRotationInProgress),
			));
			assert!(matches!(
				AuthenticatedUser::fromSessionWithConfig(&transientSession).await,
				Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_DISCONNECTED)),
			));
			assert!(AuthenticatedUser::fromSession(&transientSession).await.is_err());

			assert!(!AuthenticatedUser::session_isAuthenticated(&sessionB).await.unwrap());
			assert!(!AuthenticatedUser::session_passwordRotationBody_isAllowed(&sessionB).await);
			assert_eq!(
				UserBackHelper::passwordRotation_finalizeFromSession(request,sessionB.clone()).await.unwrap_err(),
				PasswordRotationError::AUTH_REQUIRED,
			);
			assert!(AuthenticatedUser::fromSession(&sessionB).await.is_err());
		});
	}

	#[test]
	fn passwordRotation_conflictLeavesCredentialAndConcurrentContentUntouched()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			let _configPath = ConfigPathGuard::new();
			let oldCredential = Base64::encode_string(&[41u8;32]);
			let newCredential = Base64::encode_string(&[42u8;32]);
			let original = ModuleContent {
				id: ModuleID {id: "module".to_string()},
				typeModule: "TEST".to_string(),
				timestamp: 1,
				content: "old-ciphertext".to_string(),
				..Default::default()
			};
			let (identity,session,_) = rotationAccount_create(43,oldCredential.clone(),vec![original]).await;
			let snapshot = UserBackHelper::passwordRotation_prepareFromSession(session.clone()).await.unwrap();
			let request = rotationRequest_get(snapshot,oldCredential.clone(),newCredential);

			let mut config = UserBackHelper::getUserConfigFromIdentity(&identity,false).unwrap();
			let concurrent = ModuleContent {
				id: ModuleID {id: "module".to_string()},
				typeModule: "TEST".to_string(),
				timestamp: 2,
				content: "concurrent-ciphertext".to_string(),
				..Default::default()
			};
			concurrent.update(&mut config,true).unwrap();
			drop(config);

			assert_eq!(
				UserBackHelper::passwordRotation_finalizeFromSession(request,session).await.unwrap_err(),
				PasswordRotationError::CONFLICT,
			);
			let config = UserBackHelper::getUserConfigFromIdentity(&identity,false).unwrap();
			assert_eq!(UserBackHelper::credentialVersion_get(&config),0);
			let verifier: String = config.value_get(CredentialVerifier::CONFIG_FIELD).unwrap().try_into().unwrap();
			assert_eq!(CredentialVerifier::verify(verifier,oldCredential).await.unwrap(),CredentialVerification::Current);
			assert_eq!(ModuleContent::retrieveAll(&config).unwrap()[0].content,"concurrent-ciphertext");
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
			UserBackHelper::loginCheckAndCreateFromSession(
				generatedId,
				credential.clone(),
				"test-credential-salt".to_string(),
				session.clone(),
				1_000,
			).await.unwrap();

			let migratedConfig = UserBackHelper::getUserConfigFromIdentity(&identity, false).unwrap();
			let migratedVerifier: String = migratedConfig.value_get(CredentialVerifier::CONFIG_FIELD).unwrap().try_into().unwrap();
			assert!(migratedVerifier.starts_with(CredentialVerifier::FORMAT_PREFIX));
			assert_eq!(CredentialVerifier::verify(migratedVerifier, credential).await.unwrap(), CredentialVerification::Current);
			assert_eq!(
				AuthenticatedUser::fromSession(&session).await.unwrap(),
				AuthenticatedUser::new(identity,"test-credential-salt".to_string(),0),
			);
		});
	}
}
