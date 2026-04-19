use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{
	Backable, BoxFuture, Cache, Cacheable, ModuleName, ModuleSizeContrainte, RefreshTime,
};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::dialog::{DialogData, DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr, toastingParams};
use crate::HWebTrace;
use leptoaster::{expect_toaster, ToastLevel, ToasterContext};
use leptos::ev::MouseEvent;
use leptos::html::{Div, I};
use leptos::prelude::{
	use_context, ArcRwSignal, Callback, ClassAttribute, Effect, Get, NodeRef, NodeRefAttribute,
	OnAttribute, Set, StyleAttribute, Update,
};
use leptos::prelude::{BindAttribute, GetUntracked, ViewFn, With, Write};
use leptos::prelude::{CollectView, ElementChild, IntoAny, RwSignal};
use leptos::task::spawn_local;
use leptos::{component, view, IntoView};
use leptos_use::{
	use_draggable_with_options, use_mouse_in_element, UseDraggableOptions, UseDraggableReturn,
	UseMouseInElementReturn,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::Arc;
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
pub struct Link
{
	label: String,
	url: String,
}

impl Link
{
	pub fn new(label: String, url: String) -> Self
	{
		Self { label, url }
	}
}

pub struct LinksHolder
{
	id: ModuleID,
	content: ArcRwSignal<Vec<Link>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl LinksHolder
{
	pub fn new() -> Self
	{
		Self {
			id: Default::default(),
			content: ArcRwSignal::new(vec![]),
			_update: ArcRwSignal::new(Default::default()),
			_sended: Default::default(),
		}
	}

	pub fn id_get(&self) -> ModuleID
	{
		self.id.clone()
	}

	pub fn id_set(&mut self, name: ModuleID)
	{
		self.id = name;
	}

	fn draw_link(link: &Link) -> impl IntoView
	{
		return view! {
			<a href={link.url.clone()} rel="noopener noreferrer nofollow" target="_blank">{link.label.clone()}</a>
		};
	}

	fn draw_editable_link(
		link: &Link,
		pos: usize,
		draggedOriginPosition: ArcRwSignal<Option<usize>>,
		draggedTargetPosition: ArcRwSignal<Option<usize>>,
		somethingIsDragging: ArcRwSignal<bool>,
		content: ArcRwSignal<Vec<Link>>,
		cache: ArcRwSignal<Cache>,
		dialogManager: DialogManager,
	) -> impl IntoView
	{
		// drop zone
		let target = NodeRef::<Div>::new();
		let UseMouseInElementReturn { is_outside, .. } = use_mouse_in_element(target);

		let draggedOriginPositionInner = draggedOriginPosition.clone();
		Effect::new(move |_| {
			let Some(draggedPosInner) = draggedOriginPositionInner.get()
			else
			{
				return;
			};
			if (!is_outside.get() && pos != draggedPosInner)
			{
				draggedTargetPosition.set(Some(pos));
			}
		});

		let el = NodeRef::<I>::new();
		let mut config = UseDraggableOptions::default();
		let draggedOriginPositionInner = draggedOriginPosition.clone();
		let somethingIsDraggingInner = somethingIsDragging.clone();
		config = config.on_start(move |d| {
			draggedOriginPositionInner.set(Some(pos));
			somethingIsDraggingInner.set(true);
			true
		});
		let somethingIsDraggingInner = somethingIsDragging.clone();
		config = config.on_end(move |d| {
			somethingIsDraggingInner.set(false);
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			style, is_dragging, ..
		} = use_draggable_with_options(el, config);

		let fnRemove = Self::removeLinkPopupFn(dialogManager, content, cache, pos);

		return view! {
			<div class="button ghost" style=move || {
				let mut show = "display: none;";
				if(is_dragging.get()) {show = "display: inline-block;";}
				format!("position: fixed; {} {}", style.get(),show)
			}>
				<i  class="iconoir-arrow-separate grabbable"></i>{link.label.clone()}
			</div>
			<div class={move || {
				let mut classDrop = "";
				let targetPos = draggedOriginPosition.get().map(|x| x as i32).unwrap_or(-1);
				if somethingIsDragging.get() && !is_outside.get() && targetPos != pos as i32 {classDrop = " drop"}
				format!("button{}",classDrop)
			}} node_ref=target>
				<i node_ref=el class="iconoir-arrow-separate grabbable"></i>{link.label.clone()}<i class="iconoir-xmark subbuttonremove" on:click={fnRemove}></i>
			</div>
		};
	}

	fn removeLinkPopupFn(
		dialogManager: DialogManager,
		content: ArcRwSignal<Vec<Link>>,
		cache: ArcRwSignal<Cache>,
		pos: usize,
	) -> impl Fn(MouseEvent)
	{
		let content = content.clone();
		let cache = cache.clone();

		return move |_| {
			let content = content.clone();
			let cache = cache.clone();

			let dialogContent =
				DialogData::new()
					.setTitle("MODULE_RSS_DEL")
					.setOnValidate(Callback::new(move |_| {
						content.update(|links| {
							links.remove(pos);
						});
						cache.update(|cache| {
							cache.update();
						});
						return true;
					}));
			dialogManager.open(dialogContent);
		};
	}

	fn addLinkPopupFn(
		content: ArcRwSignal<Vec<Link>>,
		cache: ArcRwSignal<Cache>,
		dialogManager: DialogManager,
	) -> impl Fn(MouseEvent) + Clone + 'static
	{
		let toaster = expect_toaster();

		return move |_| {
			let label = ArcRwSignal::new("".to_string());
			let url = ArcRwSignal::new("".to_string());

			let labelDialog = label.clone();
			let urlDialog = url.clone();
			let content = content.clone();
			let cache = cache.clone();
			let toaster = toaster.clone();

			let dialogContent = DialogData::new()
				.setTitle("MODULE_RSS_ADD")
				.setBody(move || {
					let innerLabel = RwSignal::new("".to_string());
					let innerUrl = RwSignal::new("".to_string());

					let labelEffect = labelDialog.clone();
					let urlEffect = urlDialog.clone();
					Effect::new(move |_| {
						labelEffect.clone().update(|e| *e = innerLabel.get());
						urlEffect.clone().update(|e| *e = innerUrl.get());
					});

					view! {
						<div>
							<label>
								<span>Label</span>
								<input type="text" placeholder="Label" bind:value=innerLabel/>
							</label>
							<label>
								<span>Url</span>
								<input type="text" placeholder="Url" bind:value=innerUrl/>
							</label>
						</div>
					}
					.into_any()
				})
				.setOnValidate(Callback::new(move |_| {
					let label = label.clone().get();
					let url = url.clone().get();
					let toaster = toaster.clone();

					if (url.is_empty())
					{
						let mut params = HashMap::new();
						params.insert("input".to_string(), "url".to_string());

						spawn_local(async move {
							toastingParams(
								toaster.clone(),
								AllFrontUIEnum::MUST_NOT_EMPTY,
								ToastLevel::Error,
								Arc::new(params),
							)
							.await;
						});
						return false;
					};
					if (label.is_empty())
					{
						let mut params = HashMap::new();
						params.insert("input".to_string(), "label".to_string());

						spawn_local(async move {
							toastingParams(
								toaster.clone(),
								AllFrontUIEnum::MUST_NOT_EMPTY,
								ToastLevel::Error,
								Arc::new(params),
							)
							.await;
						});
						return false;
					};

					if (Url::parse(&url).is_err())
					{
						spawn_local(async move {
							toastingErr(&toaster, AllFrontUIEnum::INVALID_URL).await;
						});
						return false;
					}

					let Some(mut guard) = content.try_write()
					else
					{
						return false;
					};
					let links: &mut Vec<Link> = guard.deref_mut();

					// remove if already exists
					if let Some(pos) = links
						.iter()
						.enumerate()
						.filter(|(_, link)| link.label == label)
						.map(|(pos, link)| pos)
						.next()
					{
						links.remove(pos);
					}

					links.push(Link::new(label, url));
					cache.update(|cache| {
						cache.update();
					});
					return true;
				}));

			dialogManager.open(dialogContent);
		};
	}
}

impl Cacheable for LinksHolder
{
	fn cache_time(&self) -> i64
	{
		return self._update.get_untracked().get();
	}

	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get_untracked().isNewer(&self._sended.get());
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache>
	{
		self._update.clone()
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache>
	{
		self._sended.clone()
	}
}

impl ModuleName for LinksHolder
{
	const MODULE_NAME: &'static str = "links";
}

impl Backable for LinksHolder
{
	fn module_name(&self) -> String
	{
		LinksHolder::MODULE_NAME.to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, _: ModuleActionFn, _: ModuleID) -> ViewFn
	{
		let contentInner = self.content.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			view! {
				<LinksDraw content=contentInner.clone() update=updateInner.clone() editMode=editMode/>
			}
			.into_any()
		})
	}

	fn refresh_time(&self) -> RefreshTime
	{
		RefreshTime::NONE
	}

	fn refresh(
		&self,
		moduleActions: ModuleActionFn,
		moduleId: ModuleID,
		toaster: ToasterContext,
	) -> Option<BoxFuture>
	{
		return None;
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent {
			id: ModuleID::new(),
			typeModule: self.module_name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content).unwrap_or_default(),
			..Default::default()
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(importedContent) = serde_json::from_str(&import.content)
		else
		{
			return;
		};

		self.content.update(|content| {
			*content = importedContent;
		});
		self._update.update(|cache| {
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache| {
			cache.update_from(import.timestamp);
		});
	}

	fn isOlderThan(&self, other: &ModuleContent) -> bool
	{
		return other.timestamp > self._update.get_untracked().get();
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self>
	{
		Some(Self::new())
	}

	fn size(&self) -> ModuleSizeContrainte
	{
		ModuleSizeContrainte::default()
	}
}

#[component]
fn LinksDraw(
	content: ArcRwSignal<Vec<Link>>,
	update: ArcRwSignal<Cache>,
	editMode: RwSignal<bool>,
) -> impl IntoView
{
	let Some(dialogManager) = use_context::<DialogManager>()
	else
	{
		HWebTrace!("cannot get dialogManager in link");
		return view! {}.into_any();
	};

	let addLinkFn =
		LinksHolder::addLinkPopupFn(content.clone(), update.clone(), dialogManager.clone());

	let draggedOriginPosition: ArcRwSignal<Option<usize>> = ArcRwSignal::new(None);
	let draggedTargetPosition: ArcRwSignal<Option<usize>> = ArcRwSignal::new(None);
	let somethingIsDragging: ArcRwSignal<bool> = ArcRwSignal::new(false);

	let contentInner = content.clone();
	let updateInner = update.clone();
	let draggedOriginPositionInner = draggedOriginPosition.clone();
	let draggedTargetPositionInner = draggedTargetPosition.clone();
	let somethingIsDraggingInner = somethingIsDragging.clone();
	Effect::new(move |_| {
		let Some(newTarget) = draggedTargetPositionInner.get()
		else
		{
			return;
		};
		let Some(Origin) = draggedOriginPositionInner.get()
		else
		{
			return;
		};
		if somethingIsDraggingInner.get()
		{
			return;
		};

		draggedOriginPositionInner.set(None);
		draggedTargetPositionInner.set(None);

		contentInner.clone().update(|vec| {
			let isAfter = newTarget > Origin;
			let mut new = vec![];
			let Some(moveLink) = vec.get(Origin).cloned()
			else
			{
				return;
			};
			let last = vec.len() - 1;
			vec.iter().enumerate().for_each(|(pos, link)| {
				if (!isAfter && pos == newTarget)
				{
					//HWebTrace!("newTarget before {}", newTarget);
					new.push(moveLink.clone());
				}

				if (pos != Origin)
				{
					new.push(link.clone());
				}
				if (isAfter && (pos == newTarget || (newTarget > last && pos == last)))
				{
					//HWebTrace!("newTarget after {}", newTarget);
					new.push(moveLink.clone());
				}
			});
			*vec = new;
		});
		updateInner.clone().update(|cache| {
			cache.update();
		});
	});

	let editModeInner = editMode.clone();
	view!{
			<div class="linksheader">
			{move || {
					let editMode = editModeInner.get();
					let draggedOriginPositionInner = draggedOriginPosition.clone();
					let draggedTargetPositionInner = draggedTargetPosition.clone();
					let somethingIsDraggingInner = somethingIsDragging.clone();
					let contentInner = content.clone();
					let updateInner = update.clone();
					let dialogManagerInner = dialogManager.clone();
					content.with(|links|{
						links.iter()
							.enumerate()
							.map(move |(key,link)|
							if editMode
								{return LinksHolder::draw_editable_link(&link,key,draggedOriginPositionInner.clone(),draggedTargetPositionInner.clone(),somethingIsDraggingInner.clone(), contentInner.clone(), updateInner.clone(),dialogManagerInner.clone()).into_any();}
							else
								{return LinksHolder::draw_link(&link).into_any();}
						)
						.collect_view()
					})
				}
			}
			{move || editMode.get().then(|| view!{<div class="button add" on:click=addLinkFn.clone()><i class="iconoir-plus-circle"></i>add</div>})}
			</div>
		}.into_any()
}
