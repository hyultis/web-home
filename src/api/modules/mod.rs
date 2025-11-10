use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;

pub mod components;

#[derive(Serialize, Deserialize)]
pub enum ModuleReturn
{
	OK,
	UPDATED(ModuleContent),
	OUTDATED
}

#[server]
pub async fn API_module_update(generatedId: String, content: ModuleContent) -> Result<ModuleReturn, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	content.update(config).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	return Ok(ModuleReturn::OK);
}

#[server]
pub async fn API_module_retrieve(generatedId: String, moduleName: String) -> Result<ModuleReturn, ServerFnError>
{
	use crate::api::login::user_back::UserBackHelper;
	let config = UserBackHelper::getUserConfig(generatedId,false).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	let mut content = ModuleContent::newFromName(moduleName);
	content.retrieve(config).map_err(|err| ServerFnError::new(format!("{:?}",err)))?;

	return Ok(ModuleReturn::UPDATED(content));
}