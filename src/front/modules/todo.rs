use serde::{Deserialize, Serialize};
use crate::front::modules::components::Cache;

#[derive(Serialize,Deserialize)]
pub struct Todo
{
	content: String,
	_cache: Cache
}