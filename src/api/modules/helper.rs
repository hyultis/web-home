use std::collections::HashMap;
use leptos::prelude::ServerFnError;
use crate::api::modules::components::{ModuleContent, ModuleErrors, ModuleID};
use crate::api::modules::ModuleReturnRetrieve;

pub fn helper_retrieveMissingModule(config: &Hconfig::HConfig::HConfig, modules: Vec<ModuleID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ServerFnError>
{
	let missing_module = ModuleContent::retrieveMissingModule(&config, modules);
	let mut returning = HashMap::new();

	for moduleId in missing_module
	{
		let mut content = ModuleContent::newFromName(&moduleId);
		match content.retrieve(&config) {
			Ok(_) => {
				returning.insert(moduleId, ModuleReturnRetrieve::UPDATED(content));
			}
			Err(ModuleErrors::Empty) => {},
			Err(err) => return Err(ServerFnError::new(format!("{:?}",err))),
		}
	};

	return Ok(returning);
}