use leptos::prelude::{RenderHtml, RwSignal};
use leptos::prelude::{ElementChild, IntoAny, Read};
use leptos::{island, view, IntoView};

#[island]
pub fn Links(editMode: RwSignal<bool>) -> impl IntoView {

	view! {
		<div>
			{editMode.read().then(|| view!{<div>i m a link !</div>})}
			<div>i m a link ! {editMode}</div>
		</div>
	}
}