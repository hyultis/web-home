use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ModuleContent};

pub mod components;

#[derive(Serialize, Deserialize)]
pub enum ModuleReturnUpdate
{
	OK,
	OUTDATED
}

#[server]
pub async fn API_module_update(generatedId: String, content: ModuleContent) -> Result<ModuleReturnUpdate, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	match content.update(config) {
		Ok(_) => {}
		Err(ModuleErrors::SavedIsNewer) => return Ok(ModuleReturnUpdate::OUTDATED),
		Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
	}

	return Ok(ModuleReturnUpdate::OK);
}

#[derive(Serialize, Deserialize)]
pub enum ModuleReturnRetrieve
{
	UPDATED(ModuleContent),
	EMPTY,
}

#[server]
pub async fn API_module_retrieve(generatedId: String, moduleName: String) -> Result<ModuleReturnRetrieve, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	use crate::api::modules::components::ModuleErrors;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	let mut content = ModuleContent::newFromName(moduleName);
	match content.retrieve(config) {
		Ok(_) => {}
		Err(ModuleErrors::Empty) => return Ok(ModuleReturnRetrieve::EMPTY),
		Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
	}

	return Ok(ModuleReturnRetrieve::UPDATED(content));
}
#[server]
pub async fn API_module_retrieveMissingModule(generatedId: String, #[server(default)] modules: Vec<String>) -> Result<Vec<String>, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	let missing_module = match ModuleContent::retrieveMissingModule(config,modules) {
		Ok(result) => {result}
		Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
	};

	return Ok(missing_module);
}
