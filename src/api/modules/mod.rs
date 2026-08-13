use std::collections::HashMap;
use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server;
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};

pub mod components;
#[cfg(feature = "ssr")]
pub mod helper;

#[cfg(feature = "ssr")]
use crate::api::modules::helper::helper_retrieveMissingModule;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ModuleApiError
{
	AUTH_REQUIRED,
	NOT_FOUND,
	SERVER_ERROR,
}

impl ModuleApiError
{
	#[cfg(feature = "ssr")]
	fn fromUserBackError(error: crate::api::login::user_back::UserBackHelperError) -> Self
	{
		use crate::api::login::user_back::UserBackHelperError;
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		return match error
		{
			UserBackHelperError::LoginError(_) => Self::AUTH_REQUIRED,
			UserBackHelperError::CredentialRotationInProgress => Self::SERVER_ERROR,
			error =>
			{
				HTrace!((Level::ERROR) "module API user resolution failed: {:?}", error);
				Self::SERVER_ERROR
			},
		};
	}

	#[cfg(feature = "ssr")]
	fn fromModuleError(error: crate::api::modules::components::ModuleErrors) -> Self
	{
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		HTrace!((Level::ERROR) "module API persistence failed: {:?}", error);
		return Self::SERVER_ERROR;
	}
}

impl FromServerFnError for ModuleApiError
{
	type Encoder = JsonEncoding;

	fn from_server_fn_error(_value: ServerFnErrorErr) -> Self
	{
		return Self::SERVER_ERROR;
	}
}

#[derive(Serialize, Deserialize)]
pub enum ModuleReturnUpdate
{
	OK,
	OUTDATED(ModuleContent)
}


/// api function that update one module content based on ModuleID and their last fetch
#[server]
pub async fn API_module_update(content: ModuleContent, overwrite:bool) -> Result<ModuleReturnUpdate, ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	use crate::api::modules::components::ModuleErrors;
	let mut mutation = AuthenticatedUser::mutation_begin().await.map_err(ModuleApiError::fromUserBackError)?;
	let config = mutation.config_getMut();
	let mut content = content;

	match content.update(config,overwrite) {
		Ok(_) => {}
		Err(ModuleErrors::SavedIsNewer) => { // never send if overwrite is true
			if content.retrieve(config).is_ok() {
				return Ok(ModuleReturnUpdate::OUTDATED(content));
			}
		},
		Err(err) => return Err(ModuleApiError::fromModuleError(err)),
	}

	return Ok(ModuleReturnUpdate::OK);
}

/// api function that updates module content based on ModuleID and their last fetch
#[server]
pub async fn API_modules_update(contents: Vec<ModuleContent>, overwrite:bool) -> Result<HashMap<ModuleID,ModuleReturnUpdate>, ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	use crate::api::modules::components::ModuleErrors;
	let mut mutation = AuthenticatedUser::mutation_begin().await.map_err(ModuleApiError::fromUserBackError)?;
	let config = mutation.config_getMut();
	let mut returning = HashMap::new();

	for mut content in contents {
		match content.update(config,overwrite) {
			Ok(_) => {}
			Err(ModuleErrors::SavedIsNewer) => { // never send if overwrite is true
				if content.retrieve(config).is_ok() {
					returning.insert(content.id.clone(), ModuleReturnUpdate::OUTDATED(content));
				}
			},
			Err(err) => return Err(ModuleApiError::fromModuleError(err)),
		}
	}

	return Ok(returning);
}

#[derive(Serialize, Deserialize)]
pub enum ModuleReturnRetrieve
{
	SAME,
	UPDATED(ModuleContent),
}

/// api function that retrieve one module content based on ModuleID and their last fetch
#[server]
pub async fn API_module_retrieve(moduleData: ApiModulesID) -> Result<ModuleReturnRetrieve, ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	use crate::api::modules::components::ModuleErrors;
	use Htrace::HTrace;

	let (_,config) = AuthenticatedUser::currentWithConfig().await.map_err(ModuleApiError::fromUserBackError)?;

	let mut content = ModuleContent::newFromName(&moduleData.key);
	match content.retrieve(&config) {
		Ok(_) => {
			HTrace!("API_module_retrieve timestamp {} > {} = {}",content.timestamp,moduleData.timestamp,content.timestamp > moduleData.timestamp);
			if(content.timestamp > moduleData.timestamp)
			{
				return Ok(ModuleReturnRetrieve::UPDATED(content));
			}
		}
		Err(ModuleErrors::Empty) => {},
		Err(err) => return Err(ModuleApiError::fromModuleError(err)),
	}

	return Ok(ModuleReturnRetrieve::SAME);
}

/// api function that retrieves module content based on ModuleID and their last fetch
#[server]
pub async fn API_modules_retrieve(modulesData: Vec<ApiModulesID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	use crate::api::modules::components::ModuleErrors;
	let (_,config) = AuthenticatedUser::currentWithConfig().await.map_err(ModuleApiError::fromUserBackError)?;
	let mut returning = HashMap::new();

	for moduleData in modulesData.iter() {
		let mut content = ModuleContent::newFromName(&moduleData.key);
		match content.retrieve(&config) {
			Ok(_) => {
				if(content.timestamp > moduleData.timestamp)
				{
					returning.insert(moduleData.key.clone(), ModuleReturnRetrieve::UPDATED(content));
				}
			}
			Err(ModuleErrors::Empty) => {},
			Err(err) => return Err(ModuleApiError::fromModuleError(err)),
		}
	}

	let missing_module = helper_retrieveMissingModule(&config, modulesData.iter().map(|e| &e.key).cloned().collect::<Vec<_>>())?;
	returning.extend(missing_module);
	return Ok(returning);
}

/// api function that retrieves module that a missing from the `modules` var
#[server]
pub async fn API_module_retrieveMissingModule(#[server(default)] modules: Vec<ModuleID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	let (_,config) = AuthenticatedUser::currentWithConfig().await.map_err(ModuleApiError::fromUserBackError)?;

	let missing_module = helper_retrieveMissingModule(&config,modules)?;
	return Ok(missing_module);
}


/// remove a specific module
#[server]
pub async fn API_module_remove(moduleName: ModuleID) -> Result<(), ModuleApiError>
{
	use crate::api::login::user_back::AuthenticatedUser;
	let mut mutation = AuthenticatedUser::mutation_begin().await.map_err(ModuleApiError::fromUserBackError)?;

	return match ModuleContent::remove(mutation.config_getMut(), moduleName) {
		true => Ok(()),
		false => Err(ModuleApiError::NOT_FOUND)
	};
}
