use crate::HWebTrace;
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
							<p>{
								let tmp = data.body.clone();
								tmp()
							}</p>
							<div class="dialog-buttons">
								<button class="validate" on:click=validateFn.clone()><Translate key={AllFrontUIEnum::VALID.to_string()}/></button>
								<button class="close" on:click=closeFn.clone()><Translate key={AllFrontUIEnum::CLOSE.to_string()}/></button>
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
	pub title: String,
	pub body: Arc<dyn Fn() -> AnyView + Send + Sync + 'static>,
	pub on_validate: Option<Callback<(),bool>>,
	pub on_close: Option<Callback<()>>,
	pub is_closing: bool,
	pub is_larger: bool,
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
	pub fn openSimple(
		&self,
		title: impl ToString,
		on_validate: Option<Callback<(),bool>>,
		on_close: Option<Callback<()>>,
	)
	{
		self.dialog.set(Some(DialogData {
			title: title.to_string(),
			body: Arc::new(move || view!{}.into_any()),
			on_validate,
			on_close,
			is_closing: false,
			is_larger: false,
		}));
	}

	/// Ouvre un popup avec le contenu fourni
	/// note pour le titre, s'il commence avec "€", il ne sera pas traduit
	pub fn open(
		&self,
		title: impl ToString,
		body: impl Fn() -> AnyView + Send + Sync + 'static,
		on_validate: Option<Callback<(),bool>>,
		on_close: Option<Callback<()>>,
	)
	{
		self.dialog.set(Some(DialogData {
			title: title.to_string(),
			body: Arc::new(body),
			on_validate,
			on_close,
			is_closing: false,
			is_larger: false,
		}));
	}

	/// Ouvre une grosse popup avec le contenu fourni
	/// note pour le titre, s'il commence avec "€", il ne sera pas traduit
	pub fn openLarger(
		&self,
		title: impl ToString,
		body: impl Fn() -> AnyView + Send + Sync + 'static,
		on_validate: Option<Callback<(),bool>>,
		on_close: Option<Callback<()>>,
	)
	{
		self.dialog.set(Some(DialogData {
			title: title.to_string(),
			body: Arc::new(body),
			on_validate,
			on_close,
			is_closing: false,
			is_larger: true,
		}));
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
		HWebTrace!("isValidated {}",isValidated);
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
