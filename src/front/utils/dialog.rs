use leptos::prelude::{ElementChild, IntoAny};
use leptos::prelude::OnAttribute;
use leptos::prelude::{AnyView, ClassAttribute, Signal, Update};
use leptos::prelude::{Callable, Callback, Get, GetUntracked, RwSignal, Set};
use leptos::{component, view, IntoView};
use leptos_use::{use_css_var, use_timeout_fn, UseTimeoutFnReturn};
use std::sync::Arc;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::translate::Translate;

#[component]
pub fn DialogHost(manager: DialogManager) -> impl IntoView
{
	let (color, set_color) = use_css_var("--animationduration");
	let duration = Signal::derive(move || {
		let value = color.get();
		parse_css_time_to_secs(&value)
	});

	let fnManager = manager.clone();
	let UseTimeoutFnReturn {
		start,
		stop,
		is_pending,
		..
	} = use_timeout_fn(
		move |_| {
			fnManager.innerClose();
		},
		duration,
	);

	let fnManager = manager.clone();
	let startfn = start.clone();
	let closeFn = move |_| {
		if (is_pending.get())
		{
			return;
		}
		fnManager.close(startfn.clone());
	};
	let fnManager = manager.clone();
	let validateFn = move |_| {
		if (is_pending.get())
		{
			return;
		}
		fnManager.validate(start.clone());
	};

	view! {
		{move || {
			manager.dialog.get().map(|data| {
				view! {
					<div class={move || {
							let mut closing = "";
							if data.is_closing {closing = " closing";}
							let mut larger = "";
							if data.is_larger {larger = " larger";}
							format!("dialog-backdrop{}{}",closing,larger)
						}} on:click=closeFn.clone()>
						<div class="dialog-window" on:click=|e| e.stop_propagation()>
							<h2>{
								if(data.title.starts_with("€")) {
									view!({data.title.chars().next().map(|c| &data.title[c.len_utf8()..]).unwrap_or("MODULE_MAIL_NO_SUBJECT")}).into_any()
								}
								else {
									view!(<Translate key={data.title}/>).into_any()
								}
					}</h2>
							<p class="dialog-content">{
								let tmp = data.body.clone();
								tmp()
							}</p>
							<div class="dialog-buttons">
								{
									if let Some(button) = data.button_validate_title.clone()
									{
										view!{<button class="validate" on:click=validateFn.clone()><Translate key={button}/></button>}.into_any()
									}
									else {view!{}.into_any()}
								}
								{
									if let Some(button) = data.button_close_title.clone()
									{
										view!{<button class="close" on:click=closeFn.clone()><Translate key={button}/></button>}.into_any()
									}
									else {view!{}.into_any()}
								}
							</div>
						</div>
					</div>
				}
			})
		}}
	}
}

#[derive(Clone)]
pub struct DialogData
{
	title: String,
	body: Arc<dyn Fn() -> AnyView + Send + Sync + 'static>,
	on_validate: Option<Callback<(),bool>>,
	on_close: Option<Callback<()>>,
	is_closing: bool,
	is_larger: bool,
	button_validate_title: Option<String>,
	button_close_title: Option<String>,
}

impl DialogData
{
	pub fn new() -> Self
	{
		Self {
			title: AllFrontUIEnum::NOTITLE.to_string(),
			body: Arc::new(move || view!{}.into_any()),
			on_validate: None,
			on_close: None,
			is_closing: false,
			is_larger: false,
			button_validate_title: Some(AllFrontUIEnum::VALID.to_string()),
			button_close_title: Some(AllFrontUIEnum::CLOSE.to_string()),
		}
	}

	/// note: si le titre commence avec "€", il ne sera pas traduit
	pub fn setTitle(mut self, title: impl ToString) -> Self
	{
		self.title = title.to_string();
		self
	}

	pub fn setBody(mut self, body: impl Fn() -> AnyView + Send + Sync + 'static) -> Self
	{
		self.body = Arc::new(body);
		self
	}

	/// Defines an action for the valid button before the popup is closed. If the callback returns false, the popup is not closed.
	pub fn setOnValidate(mut self, on_validate: Callback<(),bool>) -> Self
	{
		self.on_validate = Some(on_validate);
		self
	}

	/// Defines an action for the close button before the popup is closed.
	pub fn setOnClose(mut self, on_close: Callback<()>) -> Self
	{
		self.on_close = Some(on_close);
		self
	}

	/// "Large" tells the popup to use the maximum available screen size instead of the content’s minimum size.
	pub fn setIsLarger(mut self, is_larger: bool) -> Self
	{
		self.is_larger = is_larger;
		self
	}

	/// Change the label of the valid button (or hide it if `NONE`).
	pub fn setButtonValidateTitle(mut self, button_validate_title: Option<impl ToString>) -> Self
	{
		self.button_validate_title = button_validate_title.map(|s| s.to_string());
		self
	}

	/// Change the label of the close button (or hide it if `NONE`).
	pub fn setButtonCloseTitle(mut self, button_close_title: Option<impl ToString>)
	{
		self.button_close_title = button_close_title.map(|s| s.to_string());
	}
}

#[derive(Clone)]
pub struct DialogManager
{
	dialog: RwSignal<Option<DialogData>>,
}

impl DialogManager
{
	pub fn new() -> Self
	{
		Self {
			dialog: RwSignal::new(None),
		}
	}

	/// Ouvre un popup sans body
	/// note pour le titre, s'il commence avec "€", il ne sera pas traduit
	pub fn open(
		&self,
		dialog: DialogData
	)
	{
		self.dialog.set(Some(dialog));
	}

	/// Ferme la popup courante
	pub fn close(&self, start: impl Fn(()) + Clone + Send + Sync)
	{
		if let Some(d) = self.dialog.get_untracked()
		{
			if let Some(cb) = d.on_close
			{
				cb.run(());
			}
		}
		self.innerAnimateClose(start);
	}

	/// Valide la popup
	pub fn validate(&self, start: impl Fn(()) + Clone + Send + Sync)
	{
		let mut isValidated = true;
		if let Some(d) = self.dialog.get_untracked()
		{
			if let Some(cb) = d.on_validate
			{
				isValidated = cb.run(());
			}
		}
		if(isValidated)
		{
			self.innerAnimateClose(start);
		}
	}

	/// internal
	fn innerAnimateClose(&self, start: impl Fn(()) + Clone + Send + Sync)
	{
		self.dialog.update(|d| {
			if let Some(d) = d
			{
				d.is_closing = true;
				start(());
			}
		});
	}

	fn innerClose(&self)
	{
		self.dialog.set(None);
	}
}

/// convert transition-duration css value to seconds f64
fn parse_css_time_to_secs(s: &str) -> f64
{
	let trimmed = s.trim();
	let mut result = 0.0;

	if let Some(stripped) = trimmed.strip_suffix("ms")
	{
		result = stripped.trim().parse::<f64>().unwrap_or(0.0);
	}
	else if let Some(stripped) = trimmed.strip_suffix('s')
	{
		result = stripped.trim().parse::<f64>().unwrap_or(0.0) * 1000.0;
	}

	return result;
}
