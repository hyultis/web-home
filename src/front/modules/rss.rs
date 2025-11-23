use leptos::prelude::ArcRwSignal;
use serde::{Deserialize, Serialize};
use crate::front::modules::components::Cache;

#[derive(Serialize,Deserialize)]
struct RssContent
{
	pub title: String,
	pub link: String,
}

#[derive(Serialize,Deserialize)]
pub struct Rss
{
	content: ArcRwSignal<RssContent>,
	_update: ArcRwSignal<Cache>,
}