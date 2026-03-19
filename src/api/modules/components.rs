use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ModuleID
{
	pub id: String,
}

impl ModuleID
{
	#[cfg(feature = "ssr")]
	pub fn new() -> Self
	{
		Self {
			id: "".to_string(),
		}
	}
	#[cfg(not(feature = "ssr"))]
	pub fn new() -> Self
	{
		Self {
			id: uuid::Uuid::new_v4().to_string(),
		}
	}
}

impl Default for ModuleID
{
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiModulesID
{
	pub key: ModuleID,
	pub timestamp: i64,
}

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
	pub name: ModuleID,
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
			name: ModuleID::new(),
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
	pub fn new(name: ModuleID,typeModule: String) -> Self
	{
		Self {
			name,
			typeModule,
			..Default::default()
		}
	}

	pub fn newFromName(name: &ModuleID) -> Self
	{
		Self {
			name: name.clone(),
			..Default::default()
		}
	}

	#[cfg(feature = "ssr")]
	pub fn update(&self, config: &mut Hconfig::HConfig::HConfig,overwrite: bool) -> Result<(), ModuleErrors>
	{
		use std::collections::HashMap;
		use Hconfig::tinyjson::JsonValue;
		use Htrace::HTrace;

		let mut moduleRoot = config.value_get_mut(&self.getModulePath());
		let mut lasttimestamp = 0;
		if let Some(JsonValue::Object(content)) = moduleRoot.as_mut()
		{
			if let Some(JsonValue::Number(timestampSaved) ) = content.get("timestamp"){
				lasttimestamp = *timestampSaved as i64;
			}
		}
		drop(moduleRoot);

		if(!overwrite && lasttimestamp >= self.timestamp)
		{
			return Err(ModuleErrors::SavedIsNewer);
		}

		let mut content = HashMap::new();
		HTrace!("self.timestamp update for {:?} : {}",self.name,self.timestamp);
		content.insert("timestamp".to_string(), JsonValue::String(self.timestamp.to_string()));
		content.insert("content".to_string(), JsonValue::String(self.content.clone()));
		content.insert("type".to_string(), JsonValue::String(self.typeModule.clone()));
		content.insert("posX".to_string(), JsonValue::Number(self.pos[0] as f64));
		content.insert("posY".to_string(), JsonValue::Number(self.pos[1] as f64));
		content.insert("sizeX".to_string(), JsonValue::Number(self.size[0] as f64));
		content.insert("sizeY".to_string(), JsonValue::Number(self.size[1] as f64));
		content.insert("depth".to_string(), JsonValue::Number(self.depth as f64));

		config.value_set(&self.getModulePath(),JsonValue::Object(content));
		config.file_save().map_err(|err| ModuleErrors::ConfigError(err))?;

		return Ok(());
	}

	#[cfg(feature = "ssr")]
	pub fn retrieve(&mut self, config: &Hconfig::HConfig::HConfig) -> Result<(), ModuleErrors>
	{
		use Hconfig::tinyjson::JsonValue;

		let Some(JsonValue::Object(ref content)) = config.value_get(&self.getModulePath()) else {return Err(ModuleErrors::Empty)};

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
	pub fn remove(mut config: Hconfig::HConfig::HConfig, name: ModuleID) -> bool
	{
		use Hconfig::tinyjson::JsonValue;
		let modulePath = Self::getModulePathNamed(&name.id);
		let Some(JsonValue::Object(_)) = config.value_get(&modulePath) else {return false};

		if config.value_remove(&modulePath) {
			return config.file_save().is_ok();
		}
		return false;
	}

	#[cfg(feature = "ssr")]
	pub fn retrieveMissingModule(config: &Hconfig::HConfig::HConfig, modules: Vec<ModuleID>) -> Vec<ModuleID>
	{
		use Hconfig::tinyjson::JsonValue;

		let Some(JsonValue::Object(ref arrayOfcontent)) = config.value_get("modules") else {return vec![]};

		let mut returning = vec![];
		arrayOfcontent.keys().cloned().for_each( |name|
		{
			if (!modules.iter().any(|module| module.id==name)) {
				returning.push(ModuleID{id:name.clone()});
			}
		});

		return returning;
	}

	#[cfg(feature = "ssr")]
	fn getModulePath(&self) -> String
	{
		return Self::getModulePathNamed(&self.name.id);
	}

	#[cfg(feature = "ssr")]
	fn getModulePathNamed(name: &String) -> String
	{
		return format!("modules/{}", name);
	}

}