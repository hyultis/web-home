use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub enum ModuleErrors
{
	ConfigError(Hconfig::Errors),
	SavedIsNewer,
	Empty
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModuleContent
{
	// name of the content or child content
	pub name: String,
	// timestamp of the last update of the content or child content
	pub timestamp: i64,
	// content (crypted)
	pub content: String
}

impl ModuleContent
{
	pub fn new(name: String, updatetime: i64, content: String) -> Self
	{
		Self {
			name,
			timestamp: updatetime,
			content
		}
	}

	#[cfg(feature = "ssr")]
	pub fn update(&self, mut config: Hconfig::HConfig::HConfig) -> Result<(), ModuleErrors>
	{
		use Htrace::HTrace;
		use std::collections::HashMap;
		use Hconfig::tinyjson::JsonValue;

		HTrace!("update module {}", self.name);
		HTrace!("update module {}", self.timestamp);
		HTrace!("update module {}", self.content);

		let configPath = format!("modules/{}", self.name);

		let mut moduleRoot = config.value_get_mut(configPath.clone());
		let mut lasttimestamp = 0;
		if let Some(JsonValue::Object(ref mut content)) = moduleRoot.as_mut()
		{
			if let Some(JsonValue::Number(timestampSaved) ) = content.get("timestamp"){
				lasttimestamp = *timestampSaved as i64;
			}
		}
		drop(moduleRoot);

		if(lasttimestamp >= self.timestamp)
		{
			return Err(ModuleErrors::SavedIsNewer);
		}

		let mut content = HashMap::new();
		content.insert("timestamp".to_string(), JsonValue::Number(self.timestamp as f64));
		content.insert("content".to_string(), JsonValue::String(self.content.clone()));

		config.value_set(&configPath,JsonValue::Object(content));
		config.file_save().map_err(|err| ModuleErrors::ConfigError(err))?;

		return Ok(());
	}

	#[cfg(feature = "ssr")]
	pub fn retrieve(&mut self, config: Hconfig::HConfig::HConfig) -> Result<(), ModuleErrors>
	{
		use Hconfig::tinyjson::JsonValue;

		let Some(JsonValue::Object(ref content)) = config.value_get(&format!("modules/{}", self.name)) else {return Err(ModuleErrors::Empty)};

		if let Some(JsonValue::Number(timestampSaved) ) = content.get("timestamp"){
			self.timestamp = *timestampSaved as i64;
		}
		if let Some(JsonValue::String(content) ) = content.get("content"){
			self.content = content.clone();
		}

		return Ok(());
	}
}