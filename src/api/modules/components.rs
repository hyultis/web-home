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
	// name of the content or child content
	pub typeModule: String,
	// timestamp of the last update of the content or child content
	pub timestamp: i64,
	// content (crypted)
	pub content: String,
	pub pos: [i32; 2],
	pub size: [u32; 2],
	#[serde(default)]
	pub depth: u32,
}

impl Default for ModuleContent
{
	fn default() -> Self {
		Self {
			name: "".to_string(),
			typeModule: "".to_string(),
			timestamp: 0,
			content: "".to_string(),
			pos: [0,0],
			size: [0,0],
			depth: 0,
		}
	}
}

impl ModuleContent
{
	pub fn new(name: String,typeModule: String) -> Self
	{
		Self {
			name,
			typeModule,
			..Default::default()
		}
	}

	pub fn newFromName(name: String) -> Self
	{
		Self {
			name,
			..Default::default()
		}
	}

	#[cfg(feature = "ssr")]
	pub fn update(&self, mut config: Hconfig::HConfig::HConfig) -> Result<(), ModuleErrors>
	{
		use std::collections::HashMap;
		use Hconfig::tinyjson::JsonValue;
		use Htrace::HTrace;

		let configPath = format!("modules/{}", self.name);

		let mut moduleRoot = config.value_get_mut(&configPath);
		let mut lasttimestamp = 0;
		if let Some(JsonValue::Object(content)) = moduleRoot.as_mut()
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
		HTrace!("self.timestamp update for {} : {}",self.name,self.timestamp);
		content.insert("timestamp".to_string(), JsonValue::String(self.timestamp.to_string()));
		content.insert("content".to_string(), JsonValue::String(self.content.clone()));
		content.insert("type".to_string(), JsonValue::String(self.typeModule.clone()));
		content.insert("posX".to_string(), JsonValue::Number(self.pos[0] as f64));
		content.insert("posY".to_string(), JsonValue::Number(self.pos[1] as f64));
		content.insert("sizeX".to_string(), JsonValue::Number(self.size[0] as f64));
		content.insert("sizeY".to_string(), JsonValue::Number(self.size[1] as f64));
		content.insert("depth".to_string(), JsonValue::Number(self.depth as f64));

		config.value_set(&configPath,JsonValue::Object(content));
		config.file_save().map_err(|err| ModuleErrors::ConfigError(err))?;

		return Ok(());
	}

	#[cfg(feature = "ssr")]
	pub fn retrieve(&mut self, config: Hconfig::HConfig::HConfig) -> Result<(), ModuleErrors>
	{
		use Hconfig::tinyjson::JsonValue;

		let Some(JsonValue::Object(ref content)) = config.value_get(&format!("modules/{}", self.name)) else {return Err(ModuleErrors::Empty)};

		if let Some(JsonValue::String(timestampSaved) ) = content.get("timestamp"){
			self.timestamp = timestampSaved.parse::<i64>().unwrap_or(0);
		}
		if let Some(JsonValue::String(content) ) = content.get("content"){
			self.content = content.clone();
		}
		if let Some(JsonValue::String(content) ) = content.get("type"){
			self.typeModule = content.clone();
		}
		self.pos = [0,0];
		if let Some(JsonValue::Number(content) ) = content.get("posX"){
			self.pos[0] = *content as i32;
		}
		if let Some(JsonValue::Number(content) ) = content.get("posY"){
			self.pos[1] = *content as i32;
		}
		self.size = [0,0];
		if let Some(JsonValue::Number(content) ) = content.get("sizeX"){
			self.size[0] = *content as u32;
		}
		if let Some(JsonValue::Number(content) ) = content.get("sizeY"){
			self.size[1] = *content as u32;
		}
		if let Some(JsonValue::Number(content) ) = content.get("depth"){
			self.depth = *content as u32;
		}

		return Ok(());
	}

	#[cfg(feature = "ssr")]
	pub fn remove(mut config: Hconfig::HConfig::HConfig, name: String) -> bool
	{
		use Hconfig::tinyjson::JsonValue;
		let Some(JsonValue::Object(_)) = config.value_get(&format!("modules/{}", name)) else {return false};

		if config.value_remove(&format!("modules/{}", name)) {
			return config.file_save().is_ok();
		}
		return false;
	}

	#[cfg(feature = "ssr")]
	pub fn retrieveMissingModule(config: Hconfig::HConfig::HConfig, modules: Vec<String>) -> Vec<String>
	{
		use Hconfig::tinyjson::JsonValue;

		let Some(JsonValue::Object(ref arrayOfcontent)) = config.value_get("modules") else {return vec![]};

		let mut returning = vec![];
		arrayOfcontent.keys().cloned().for_each( |name|
		{
			if (!modules.contains(&name)) {
				returning.push(name.clone());
			}
		});

		return returning;
	}
}