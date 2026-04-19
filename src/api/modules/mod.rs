use std::collections::HashMap;
use leptos::prelude::{FromServerFnError, ServerFnError, ServerFnErrorErr};
use leptos::server;
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};

pub mod components;
#[cfg(feature = "ssr")]
pub mod helper;

#[cfg(feature = "ssr")]
use crate::api::modules::helper::helper_retrieveMissingModule;

#[derive(Serialize, Deserialize)]
pub enum ModuleReturnUpdate
{
	OK,
	OUTDATED(ModuleContent)
}


/// api function that update one module content based on ModuleID and their last fetch
#[server]
pub async fn API_module_update(generatedId: String, content: ModuleContent, overwrite:bool) -> Result<ModuleReturnUpdate, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	let mut config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;
	let mut content = content;

	match content.update(&mut config,overwrite) {
		Ok(_) => {}
		Err(ModuleErrors::SavedIsNewer) => { // never send if overwrite is true
			if content.retrieve(&config).is_ok() {
				return Ok(ModuleReturnUpdate::OUTDATED(content));
			}
		},
		Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
	}

	return Ok(ModuleReturnUpdate::OK);
}

/// api function that updates module content based on ModuleID and their last fetch
#[server]
pub async fn API_modules_update(generatedId: String, contents: Vec<ModuleContent>, overwrite:bool) -> Result<HashMap<ModuleID,ModuleReturnUpdate>, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	let mut config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;
	let mut returning = HashMap::new();

	for mut content in contents {
		match content.update(&mut config,overwrite) {
			Ok(_) => {}
			Err(ModuleErrors::SavedIsNewer) => { // never send if overwrite is true
				if content.retrieve(&config).is_ok() {
					returning.insert(content.id.clone(), ModuleReturnUpdate::OUTDATED(content));
				}
			},
			Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
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
pub async fn API_module_retrieve(generatedId: String, moduleData: ApiModulesID) -> Result<ModuleReturnRetrieve, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	use Htrace::HTrace;

	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

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
		Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
	}

	return Ok(ModuleReturnRetrieve::SAME);
}

/// api function that retrieves module content based on ModuleID and their last fetch
#[server]
pub async fn API_modules_retrieve(generatedId: String, modulesData: Vec<ApiModulesID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;
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
			Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
		}
	}

	let missing_module = helper_retrieveMissingModule(&config, modulesData.iter().map(|e| &e.key).cloned().collect::<Vec<_>>())?;
	returning.extend(missing_module);
	return Ok(returning);
}

/// api function that retrieves module that a missing from the `modules` var
#[server]
pub async fn API_module_retrieveMissingModule(generatedId: String, #[server(default)] modules: Vec<ModuleID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	let missing_module = helper_retrieveMissingModule(&config,modules)?;
	return Ok(missing_module);
}


#[derive(Serialize, Deserialize, Debug)]
pub enum ModuleReturnRemove
{
	NOTFOUND,
	SERVER_ERROR
}

impl FromServerFnError for ModuleReturnRemove {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		ModuleReturnRemove::SERVER_ERROR
	}
}

/// remove a specific module
#[server]
pub async fn API_module_remove(generatedId: String, moduleName: ModuleID) -> Result<(), ModuleReturnRemove>
{
	use crate::api::login::user_back::UserBackHelper;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ModuleReturnRemove::SERVER_ERROR)?;

	return match ModuleContent::remove(config, moduleName) {
		true => Ok(()),
		false => Err(ModuleReturnRemove::NOTFOUND)
	};
}