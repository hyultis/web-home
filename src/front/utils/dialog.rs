use leptos::ev::KeyboardEvent;
use leptos::html::Div;
use leptos::prelude::{AriaAttributes, ElementChild, GlobalAttributes, IntoAny, NodeRef, NodeRefAttribute};
use leptos::prelude::OnAttribute;
use leptos::prelude::{AnyView, ClassAttribute, Effect, Signal, Update};
use leptos::prelude::{Get, GetUntracked, RwSignal, Set};
use leptos::{component, view, IntoView};
use leptos_use::{use_css_var, use_timeout_fn, UseTimeoutFnReturn};
use std::sync::Arc;
#[cfg(feature="hydrate")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature="hydrate")]
use wasm_bindgen::JsCast;
#[cfg(feature="hydrate")]
use web_sys::HtmlElement;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::translate::TranslateText;

#[component]
pub fn DialogHost(manager: DialogManager) -> impl IntoView
{
	let (color, _) = use_css_var("--motion-overlay");
	let duration = Signal::derive(move || {
		let value = color.get();
		parse_css_time_to_secs(&value)
	});
	let dialogRef = NodeRef::<Div>::new();
	let focusManager = manager.clone();
	Effect::new(move |wasOpen: Option<bool>| {
		let isOpen = focusManager.dialog.get().is_some();
		if (dialogFocus_initialMustApply(isOpen,wasOpen))
		{
			dialogFocus_initial(dialogRef);
		}
		return isOpen;
	});

	let fnManager = manager.clone();
	let UseTimeoutFnReturn {
		start,
		stop: _,
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
	let keyboardStart = startfn.clone();
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
	let fnManager = manager.clone();
	let keyboardCloseFn = move |event: KeyboardEvent| {
		if (event.key() == "Tab")
		{
			dialogFocus_trap(&event,dialogRef);
			return;
		}
		if (event.key() != "Escape" || is_pending.get())
		{
			return;
		}
		event.prevent_default();
		fnManager.close(keyboardStart.clone());
	};

	view! {
		{move || {
			manager.dialog.get().map(|data| {
				let closeEnabledData = data.clone();
				let workspaceCloseEnabledData = data.clone();
				let validateEnabledData = data.clone();
				let isWorkspace = data.is_workspace;
				let title = data.title.clone();
				let workspaceHeaderStart = data.header_start.clone();
				let workspaceCloseTitle = data.button_close_title.clone();
				view! {
					<div class={move || {
							let mut closing = "";
							if data.is_closing {closing = " closing";}
							let mut larger = "";
							if data.is_larger {larger = " larger";}
							let mut workspace = "";
							if data.is_workspace {workspace = " workspace";}
							format!("dialog-backdrop{}{}{}",closing,larger,workspace)
						}} on:click=closeFn.clone() on:keydown=keyboardCloseFn.clone()>
						<div
							class="dialog-window"
							node_ref=dialogRef
							role="dialog"
							aria-modal="true"
							aria-labelledby="webhome-dialog-title"
							tabindex="-1"
							on:click=|e| e.stop_propagation()
						>
							{if isWorkspace
							{
								view! {
									<div class="dialog-workspace-header">
										<div class="dialog-workspace-header-start">{
											workspaceHeaderStart.map(|header| header())
										}</div>
										<h2 id="webhome-dialog-title">{dialogTitle_view(title.clone())}</h2>
										<div class="dialog-workspace-header-end">{
											workspaceCloseTitle.map(|button| view! {
												<button
													type="button"
													class="dialog-workspace-close icon_button"
													disabled=move || !workspaceCloseEnabledData.canClose()
													on:click=closeFn.clone()
												>
													<i class="iconoir-xmark" aria-hidden="true"></i>
													<span class="visually_hidden"><TranslateText key=button/></span>
												</button>
											})
										}</div>
									</div>
								}.into_any()
							}
							else
							{
								view! {<h2 id="webhome-dialog-title">{dialogTitle_view(title)}</h2>}.into_any()
							}}
							<div class="dialog-content">{
								let tmp = data.body.clone();
								tmp()
							}</div>
							{(!isWorkspace).then(|| view! {<div class="dialog-buttons">
								{
									if let Some(button) = data.button_close_title.clone()
									{
										view!{
											<button
												type="button"
												class="close"
												disabled=move || !closeEnabledData.canClose()
												on:click=closeFn.clone()
											><TranslateText key={button}/></button>
										}.into_any()
									}
									else {view!{}.into_any()}
								}
								{
									if let Some(button) = data.button_validate_title.clone()
									{
										view!{
											<button
												type="button"
												class={data.validate_style.class_get()}
												disabled=move || !validateEnabledData.canClose()
												on:click=validateFn.clone()
											><TranslateText key={button}/></button>
										}.into_any()
									}
									else {view!{}.into_any()}
								}
							</div>})}
						</div>
					</div>
				}
			})
		}}
	}
}

fn dialogTitle_view(title: String) -> AnyView
{
	if let Some(title) = title.strip_prefix('€')
	{
		return view!{{title.to_string()}}.into_any();
	}
	return view!{<TranslateText key=title/>}.into_any();
}

#[derive(Clone)]
pub struct DialogData
{
	title: String,
	header_start: Option<Arc<dyn Fn() -> AnyView + Send + Sync + 'static>>,
	body: Arc<dyn Fn() -> AnyView + Send + Sync + 'static>,
	// A dialog can outlive the reactive Owner that created it, so it must own its actions.
	on_validate: Option<Arc<dyn Fn(()) -> bool + Send + Sync + 'static>>,
	on_close: Option<Arc<dyn Fn(()) + Send + Sync + 'static>>,
	can_close: Arc<dyn Fn() -> bool + Send + Sync + 'static>,
	is_closing: bool,
	is_larger: bool,
	is_workspace: bool,
	button_validate_title: Option<String>,
	button_close_title: Option<String>,
	validate_style: DialogActionStyle,
}

#[derive(Clone, Copy)]
pub(crate) enum DialogActionStyle
{
	Success,
	Warning,
	Danger,
}

impl DialogActionStyle
{
	fn class_get(self) -> &'static str
	{
		return match self
		{
			Self::Success => "validate",
			Self::Warning => "validate validate_warning",
			Self::Danger => "validate validate_danger",
		};
	}
}

impl DialogData
{
	pub fn new() -> Self
	{
		Self {
			title: AllFrontUIEnum::NOTITLE.to_string(),
			header_start: None,
			body: Arc::new(move || view!{}.into_any()),
			on_validate: None,
			on_close: None,
			can_close: Arc::new(|| true),
			is_closing: false,
			is_larger: false,
			is_workspace: false,
			button_validate_title: Some(AllFrontUIEnum::VALID.to_string()),
			button_close_title: Some(AllFrontUIEnum::CLOSE.to_string()),
			validate_style: DialogActionStyle::Success,
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

	/// Adds content before the centered title in an application-like workspace header.
	pub(crate) fn setHeaderStart(mut self,header_start: impl Fn() -> AnyView + Send + Sync + 'static) -> Self
	{
		self.header_start = Some(Arc::new(header_start));
		self
	}

	/// Defines an action for the valid button before the popup is closed. If the callback returns false, the popup is not closed.
	pub fn setOnValidate(mut self, on_validate: impl Fn(()) -> bool + Send + Sync + 'static) -> Self
	{
		self.on_validate = Some(Arc::new(on_validate));
		self
	}

	/// Defines an action for the close button before the popup is closed.
	pub fn setOnClose(mut self, on_close: impl Fn(()) + Send + Sync + 'static) -> Self
	{
		self.on_close = Some(Arc::new(on_close));
		self
	}

	/// Prevents user-driven validation and closing while a dialog-owned operation is incomplete.
	pub fn setCanClose(mut self, can_close: impl Fn() -> bool + Send + Sync + 'static) -> Self
	{
		self.can_close = Arc::new(can_close);
		self
	}

	fn canClose(&self) -> bool
	{
		return (self.can_close)();
	}

	fn run_validate(&self) -> bool
	{
		if (!self.canClose())
		{
			return false;
		}
		return self.on_validate.as_ref().map(|callback| callback(())).unwrap_or(true);
	}

	fn run_close(&self) -> bool
	{
		if (!self.canClose())
		{
			return false;
		}
		self.run_closeForced();
		return true;
	}

	fn run_closeForced(&self)
	{
		if let Some(callback) = &self.on_close
		{
			callback(());
		}
	}

	/// "Large" tells the popup to use the maximum available screen size instead of the content’s minimum size.
	pub fn setIsLarger(mut self, is_larger: bool) -> Self
	{
		self.is_larger = is_larger;
		self
	}

	/// "Workspace" reserves almost the complete viewport for an application-like surface.
	pub fn setIsWorkspace(mut self,is_workspace: bool) -> Self
	{
		self.is_workspace = is_workspace;
		self
	}

	/// Change the label of the valid button (or hide it if `NONE`).
	pub fn setButtonValidateTitle(mut self, button_validate_title: Option<impl ToString>) -> Self
	{
		self.button_validate_title = button_validate_title.map(|s| s.to_string());
		self
	}

	pub(crate) fn setValidateStyle(mut self, validate_style: DialogActionStyle) -> Self
	{
		self.validate_style = validate_style;
		self
	}

	/// Change the label of the close button (or hide it if `NONE`).
	pub fn setButtonCloseTitle(mut self, button_close_title: Option<impl ToString>)
		-> Self
	{
		self.button_close_title = button_close_title.map(|s| s.to_string());
		self
	}
}

#[derive(Clone)]
pub struct DialogManager
{
	dialog: RwSignal<Option<DialogData>>,
	focusReturn: RwSignal<Option<DialogFocusReturn>>,
}

#[cfg(feature="hydrate")]
#[derive(Clone)]
struct DialogFocusReturn
{
	id: String,
	removeId: bool,
}

#[cfg(not(feature="hydrate"))]
#[derive(Clone)]
struct DialogFocusReturn;

impl DialogManager
{
	pub fn new() -> Self
	{
		Self {
			dialog: RwSignal::new(None),
			focusReturn: RwSignal::new(None),
		}
	}

	/// Ouvre un popup sans body
	/// note pour le titre, s'il commence avec "€", il ne sera pas traduit
	pub fn open(
		&self,
		dialog: DialogData
	)
	{
		if (self.dialog.get_untracked().is_none())
		{
			self.focusReturn.set(dialogFocus_capture());
		}
		self.dialog.set(Some(dialog));
	}

	pub(crate) fn clear(&self)
	{
		if let Some(dialog) = self.dialog.get_untracked()
		{
			dialog.run_closeForced();
		}
		self.innerClose();
	}

	/// Ferme la popup courante
	pub fn close(&self, start: impl Fn(()) + Clone + Send + Sync)
	{
		if let Some(dialog) = self.dialog.get_untracked()
		{
			if (!dialog.run_close())
			{
				return;
			}
		}
		self.innerAnimateClose(start);
	}

	/// Valide la popup
	pub fn validate(&self, start: impl Fn(()) + Clone + Send + Sync)
	{
		let mut isValidated = true;
		if let Some(dialog) = self.dialog.get_untracked()
		{
			isValidated = dialog.run_validate();
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
		if let Some(focusReturn) = self.focusReturn.get_untracked()
		{
			dialogFocus_restore(&focusReturn);
			self.focusReturn.set(None);
		}
	}
}

#[cfg(feature="hydrate")]
fn dialogFocus_capture() -> Option<DialogFocusReturn>
{
	static FOCUS_ID: AtomicU64 = AtomicU64::new(1);

	let document = web_sys::window()?.document()?;
	let activeElement = document.active_element()?;
	let currentId = activeElement.id();
	if (!currentId.is_empty())
	{
		return Some(DialogFocusReturn {id: currentId,removeId: false});
	}

	let focusId = format!("webhome-dialog-focus-return-{}",FOCUS_ID.fetch_add(1,Ordering::Relaxed));
	activeElement.set_id(&focusId);
	return Some(DialogFocusReturn {id: focusId,removeId: true});
}

#[cfg(not(feature="hydrate"))]
fn dialogFocus_capture() -> Option<DialogFocusReturn>
{
	return None;
}

#[cfg(feature="hydrate")]
fn dialogFocus_restore(focusReturn: &DialogFocusReturn)
{
	let Some(document) = web_sys::window().and_then(|window| window.document()) else {return};
	let Some(element) = document.get_element_by_id(&focusReturn.id) else {return};
	if let Ok(htmlElement) = element.clone().dyn_into::<HtmlElement>()
	{
		let _ = htmlElement.focus();
	}
	if (focusReturn.removeId)
	{
		let _ = element.remove_attribute("id");
	}
}

#[cfg(not(feature="hydrate"))]
fn dialogFocus_restore(_: &DialogFocusReturn)
{
}

fn dialogFocus_initialMustApply(isOpen: bool, wasOpen: Option<bool>) -> bool
{
	return isOpen && wasOpen != Some(true);
}

#[cfg(feature="hydrate")]
fn dialogFocusableElements_get(dialog: &HtmlElement) -> Vec<HtmlElement>
{
	let Ok(nodes) = dialog.query_selector_all(concat!(
		"a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),",
		"textarea:not([disabled]),[tabindex]:not([tabindex=\"-1\"]):not([aria-disabled=\"true\"])"
	)) else {return Vec::new()};
	let mut focusableElements = Vec::new();
	for index in 0..nodes.length()
	{
		if let Some(node) = nodes.item(index) && let Ok(element) = node.dyn_into::<HtmlElement>()
		{
			if (element.closest("[hidden]").ok().flatten().is_some())
			{
				continue;
			}
			focusableElements.push(element);
		}
	}
	return focusableElements;
}

#[cfg(feature="hydrate")]
fn dialogFocus_initial(dialogRef: NodeRef<Div>)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(dialog) = dialogRef.get_untracked() else {return};
		let dialogElement: HtmlElement = dialog.unchecked_into();
		let focusableElements = dialogFocusableElements_get(&dialogElement);
		let target = focusableElements.first().unwrap_or(&dialogElement);
		let _ = target.focus();
	});
}

#[cfg(not(feature="hydrate"))]
fn dialogFocus_initial(_: NodeRef<Div>)
{
}

#[cfg(feature="hydrate")]
fn dialogFocus_trap(event: &KeyboardEvent, dialogRef: NodeRef<Div>)
{
	let Some(dialog) = dialogRef.get_untracked() else {return};
	let dialogElement: HtmlElement = dialog.unchecked_into();
	let focusRoot = dialogElement.query_selector(concat!(
		"[role=\"dialog\"][aria-modal=\"true\"],",
		"[role=\"alertdialog\"][aria-modal=\"true\"]"
	)).ok().flatten()
		.and_then(|element| element.dyn_into::<HtmlElement>().ok())
		.unwrap_or_else(|| dialogElement.clone());
	let focusableElements = dialogFocusableElements_get(&focusRoot);
	let activeElement = web_sys::window()
		.and_then(|window| window.document())
		.and_then(|document| document.active_element())
		.and_then(|element| element.dyn_into::<HtmlElement>().ok());
	let Some(firstElement) = focusableElements.first() else {
		event.prevent_default();
		let _ = focusRoot.focus();
		return;
	};
	let Some(lastElement) = focusableElements.last() else {return};
	let dialogHasFocus = activeElement.as_ref().is_none_or(|element| element == &focusRoot);
	let mustWrap = if (event.shift_key())
	{
		dialogHasFocus || activeElement.as_ref().is_some_and(|element| element == firstElement)
	}
	else
	{
		activeElement.as_ref().is_some_and(|element| element == lastElement)
	};
	if (!mustWrap)
	{
		return;
	}

	event.prevent_default();
	let target = if (event.shift_key()) {lastElement} else {firstElement};
	let _ = target.focus();
}

#[cfg(not(feature="hydrate"))]
fn dialogFocus_trap(_: &KeyboardEvent, _: NodeRef<Div>)
{
}

#[cfg(test)]
mod tests
{
	use super::{dialogFocus_initialMustApply, DialogActionStyle, DialogData, DialogManager};
	use leptos::prelude::{GetUntracked, Owner};
	use std::sync::Arc;
	use std::sync::atomic::{AtomicBool, Ordering};

	#[test]
	fn dialog_validate_action_survives_origin_owner_cleanup()
	{
		let originOwner = Owner::new();
		let wasCalled = Arc::new(AtomicBool::new(false));
		let wasCalledInner = wasCalled.clone();

		let dialog = originOwner.with(|| {
			DialogData::new().setOnValidate(move |_| {
				wasCalledInner.store(true,Ordering::Relaxed);
				return true;
			})
		});

		originOwner.cleanup();

		assert!(dialog.run_validate());
		assert!(wasCalled.load(Ordering::Relaxed));
	}

	#[test]
	fn dialog_clear_dropsAccountScopedContentImmediately()
	{
		let owner = Owner::new();
		let wasClosed = Arc::new(AtomicBool::new(false));
		let wasClosedInner = wasClosed.clone();
		owner.with(|| {
			let manager = DialogManager::new();
			manager.open(DialogData::new()
				.setTitle("account-a-content")
				.setOnClose(move |_| wasClosedInner.store(true,Ordering::Relaxed)));
			assert!(manager.dialog.get_untracked().is_some());

			manager.clear();
			assert!(manager.dialog.get_untracked().is_none());
		});
		owner.cleanup();
		assert!(wasClosed.load(Ordering::Relaxed));
	}

	#[test]
	fn dialogCloseGuard_blocksUserActionsButNotLifecycleClear()
	{
		let owner = Owner::new();
		let wasClosed = Arc::new(AtomicBool::new(false));
		let wasClosedInner = wasClosed.clone();
		owner.with(|| {
			let dialog = DialogData::new()
				.setCanClose(|| false)
				.setOnClose(move |_| wasClosedInner.store(true,Ordering::Relaxed));

			assert!(!dialog.run_validate());
			assert!(!dialog.run_close());
			assert!(!wasClosed.load(Ordering::Relaxed));

			let manager = DialogManager::new();
			manager.open(dialog);
			manager.clear();
			assert!(manager.dialog.get_untracked().is_none());
		});
		owner.cleanup();
		assert!(wasClosed.load(Ordering::Relaxed));
	}

	#[test]
	fn dialogActionStyle_mapsSemanticClasses()
	{
		assert_eq!(DialogActionStyle::Success.class_get(),"validate");
		assert_eq!(DialogActionStyle::Warning.class_get(),"validate validate_warning");
		assert_eq!(DialogActionStyle::Danger.class_get(),"validate validate_danger");
	}

	#[test]
	fn dialogInitialFocus_runsOnlyWhenDialogBecomesOpen()
	{
		assert!(!dialogFocus_initialMustApply(false,None));
		assert!(dialogFocus_initialMustApply(true,None));
		assert!(dialogFocus_initialMustApply(true,Some(false)));
		assert!(!dialogFocus_initialMustApply(true,Some(true)));
		assert!(!dialogFocus_initialMustApply(false,Some(true)));
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
