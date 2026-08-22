use leptos::prelude::{ClassAttribute, ElementChild};
use leptos::prelude::{AnyView, IntoAny};
use leptos::view;

pub mod translate;
pub mod users_data;
pub mod fluent;
pub mod trace;
pub mod dialog;
pub mod toaster_helpers;
pub mod all_front_enum;
pub mod contentDownloader;
pub(crate) mod browser;
mod external_url;

pub(super) use external_url::SafeExternalUrl;


pub fn draw_title_if_present(title: String) -> AnyView
{
	if(title.is_empty())
	{
		view!{}.into_any()
	}
	else
	{
		view!{<h2 class="module_title">{title}</h2>}.into_any()
	}
}
