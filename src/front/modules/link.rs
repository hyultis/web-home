use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::{expect_toaster, ToastLevel};
use leptos::prelude::{BindAttribute, GetUntracked, Write};
use leptos::prelude::{use_context, ArcRwSignal, Callback, ClassAttribute, Effect, Get, NodeRef, NodeRefAttribute, OnAttribute, Set, StyleAttribute, Update};
use crate::front::modules::components::{Backable, Cache, Cacheable};
use leptos::prelude::{AnyView, CollectView, ElementChild, IntoAny, Read, RwSignal};
use leptos::{view, IntoView};
use leptos::ev::MouseEvent;
use leptos::html::{Div, I};
use leptos_use::{use_draggable_with_options, use_mouse_in_element, UseDraggableOptions, UseDraggableReturn, UseMouseInElementReturn};
use serde::{Deserialize, Serialize};
use url::Url;
use crate::api::modules::components::ModuleContent;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::dialog::DialogManager;
use crate::front::utils::toaster_helpers::{toastingErr, toastingParams};
use crate::HWebTrace;
use std::ops::DerefMut;
use leptos::__reexports::wasm_bindgen_futures::spawn_local;

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
	content: ArcRwSignal<Vec<Link>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl LinksHolder
{
	pub fn new() -> Self
	{
		Self {
			content: ArcRwSignal::new(vec![]),
			_update: ArcRwSignal::new(Default::default()),
			_sended: Default::default(),
		}
	}

	fn draw_link(link: &Link) -> impl IntoView
	{
		return view! {
			<a href={link.url.clone()} rel="noopener noreferrer nofollow" target="_blank">{link.label.clone()}</a>
		};
	}

	fn draw_editable_link(link: &Link, pos: usize,
	                      draggedOriginPosition: RwSignal<Option<usize>>,
	                      draggedTargetPosition: RwSignal<Option<usize>>,
	                      somethingIsDragging: RwSignal<bool>,
	                      content: ArcRwSignal<Vec<Link>>, cache: ArcRwSignal<Cache>) -> impl IntoView
	{
		// drop zone
		let target = NodeRef::<Div>::new();
		let UseMouseInElementReturn {
			is_outside, ..
		} = use_mouse_in_element(target);

		Effect::new(move |_| {
			let Some(draggedPosInner) = draggedOriginPosition.get() else {return};
			if(!is_outside.get() && pos!=draggedPosInner)
			{
				draggedTargetPosition.set(Some(pos));
			}
		});

		let el = NodeRef::<I>::new();
		let mut config = UseDraggableOptions::default();
		config = config.on_start( move |d| {
			draggedOriginPosition.set(Some(pos));
			somethingIsDragging.set(true);
			true
		});
		config = config.on_end( move |d| {
			somethingIsDragging.set(false);
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			style,
			is_dragging,
			..
		} = use_draggable_with_options(el,config);

		let fnRemove = Self::removeLinkPopupFn(content,cache,pos);


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

	fn removeLinkPopupFn(content: ArcRwSignal<Vec<Link>>, cache: ArcRwSignal<Cache>, pos: usize) -> impl Fn(MouseEvent)
	{
		let dialog = use_context::<DialogManager>().expect("DialogManager missing");
		let content = content.clone();
		let cache = cache.clone();

		return move |_| {
			let content = content.clone();
			let cache = cache.clone();
			dialog.openSimple("Supprimer un lien ?", Some(Callback::new(move |_| {
				content.update(|links|{
					links.remove(pos);
				});
				cache.update(|cache|{
					cache.update();
				});
				return true;
			})), None);
		};
	}

	fn addLinkPopupFn(&self) -> impl Fn(MouseEvent)
	{
		let dialog = use_context::<DialogManager>().expect("DialogManager missing");
		let content = self.content.clone();
		let cache = self._update.clone();
		let toaster = expect_toaster();

		return move |_| {
			let label = ArcRwSignal::new("".to_string());
			let url = ArcRwSignal::new("".to_string());

			let labelDialog = label.clone();
			let urlDialog = url.clone();
			let content = content.clone();
			let cache = cache.clone();
			let toaster = toaster.clone();
			dialog.open("Ajouter un lien", move || {

				let innerLabel = RwSignal::new("".to_string());
				let innerUrl = RwSignal::new("".to_string());

				let labelEffect = labelDialog.clone();
				let urlEffect = urlDialog.clone();
				Effect::new(move |_| {
					labelEffect.clone().update(|e| *e = innerLabel.get());
					urlEffect.clone().update(|e| *e = innerUrl.get());
				});

				view!{
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
			}.into_any()
			}, Some(Callback::new(move |_| {
				let label = label.clone().get();
				let url = url.clone().get();
				let toaster = toaster.clone();

				if(url.is_empty()) {
					let mut params= HashMap::new();
					params.insert("input".to_string(), "url".to_string());

					spawn_local(async move {
						toastingParams(toaster.clone(), AllFrontUIEnum::MUST_NOT_EMPTY, ToastLevel::Error, Arc::new(params)).await;
					});
					return false;
				};
				if(label.is_empty()) {
					let mut params= HashMap::new();
					params.insert("input".to_string(), "label".to_string());

					spawn_local(async move {
						toastingParams(toaster.clone(), AllFrontUIEnum::MUST_NOT_EMPTY, ToastLevel::Error, Arc::new(params)).await;
					});
					return false;
				};

				if(Url::parse(&url).is_err())
				{
					spawn_local(async move {
						toastingErr(toaster.clone(), AllFrontUIEnum::INVALID_URL).await;
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
				if let Some(pos) = links.iter().enumerate().filter(|(_,link)|link.label==label).map(|(pos,link)|pos).next()
				{
					links.remove(pos);
				}

				links.push(Link::new(label,url));
				cache.update(|cache|{
					cache.update();
				});
				return true;
			})), None);
		};
	}
}

impl Cacheable for LinksHolder
{
	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get().isNewer(&self._sended.get());
	}
}

impl Backable for LinksHolder
{
	fn name(&self) -> String
	{
		"links".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>) -> AnyView
	{
		let addLinkFn = self.addLinkPopupFn();

		let draggedOriginPosition: RwSignal<Option<usize>> = RwSignal::new(None);
		let draggedTargetPosition: RwSignal<Option<usize>> = RwSignal::new(None);
		let somethingIsDragging: RwSignal<bool> = RwSignal::new(false);

		let content = self.content.clone();
		let cache = self._update.clone();
		Effect::new(move |_| {
			let Some(newTarget) = draggedTargetPosition.get() else {return};
			let Some(Origin) = draggedOriginPosition.get() else {return};
			if somethingIsDragging.get() {return};

			draggedTargetPosition.set(None);
			draggedOriginPosition.set(None);

			content.clone().update(|vec|{
				let isAfter = newTarget > Origin;
				let mut new = vec![];
				let Some(moveLink) = vec.get(Origin).cloned() else {return};
				let last = vec.len() -1;
				vec.iter().enumerate().for_each(|(pos,link)|{
					if(!isAfter && pos==newTarget)
					{
						//HWebTrace!("newTarget before {}", newTarget);
						new.push(moveLink.clone());
					}

					if(pos!=Origin) {
						new.push(link.clone());
					}
					if(isAfter && (pos==newTarget || (newTarget>last && pos==last))){
						//HWebTrace!("newTarget after {}", newTarget);
						new.push(moveLink.clone());
					}
				});

				/*if(!isAfter)
				{new.remove(Origin+1);}
				else
				{new.remove(Origin);}*/
				*vec = new;
			});
			cache.clone().update(|cache|{
				cache.update();
			});

			HWebTrace!("need to move origin {} to target {}", Origin, newTarget);
		});
		/*
			<span>{match &*draggedPosition.read(){
				Some(pos) => view!{pos}.into_any(),
				_ => view!{<span>fgdfg</span>}.into_any()
			}}</span>
		 */
		view!{
			<div class="linksheader">
			{self.content.read()
				.iter()
				.enumerate()
				.map(|(key,link)|
					if editMode.get()
						{return Self::draw_editable_link(&link,key,draggedOriginPosition,draggedTargetPosition,somethingIsDragging, self.content.clone(), self._update.clone()).into_any();}
					else
						{return Self::draw_link(&link).into_any();}
				)
				.collect_view()
			}
			{editMode.read().then(|| view!{<div class="button add" on:click=addLinkFn><i class="iconoir-plus-circle"></i>add</div>})}
			</div>
		}.into_any()
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			name: "links".to_string(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content).unwrap(),
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(content) = serde_json::from_str(&import.content) else {return};

		self.content = content;
		self._update.update(|cache|{
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache|{
			cache.update_from(import.timestamp);
		});
	}
}
