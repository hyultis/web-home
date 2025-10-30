use Hconfig::HConfigManager::HConfigManager;
use Hconfig::tinyjson::JsonValue;
use crate::global_security::hash;

/// Retrieves a site-specific salt for a given user ID.
///
/// This function fetches a site-wide salt value from the configuration manager and
/// appends it to the provided user-generated ID. It then applies a hash function to
/// generate a unique salted hash for the user.
///
/// # Arguments
///
/// * `generatedId` - A string representing the user-generated ID to be used for creating the salt.
///
/// # Returns
/// `None` if the site configuration or salt value is missing or could not be retrieved.
pub fn getSiteSaltForUser(generatedId: String) -> Option<String>
{
	let Some(config) = HConfigManager::singleton().get("site") else {return None};
	let Some(JsonValue::String(value)) = config.value_get("salt") else {return None};

	return Some(hash(format!("{}{}",value,generatedId)));
}