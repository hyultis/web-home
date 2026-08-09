use std::collections::HashMap;
use crate::api::modules::components::{ModuleContent, ModuleErrors, ModuleID};
use crate::api::modules::{ModuleApiError, ModuleReturnRetrieve};

pub fn helper_retrieveMissingModule(config: &Hconfig::HConfig::HConfig, modules: Vec<ModuleID>) -> Result<HashMap<ModuleID,ModuleReturnRetrieve>, ModuleApiError>
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
			Err(err) => return Err(ModuleApiError::fromModuleError(err)),
		}
	};

	return Ok(returning);
}
