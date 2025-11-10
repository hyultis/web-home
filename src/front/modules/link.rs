use leptos::prelude::{BindAttribute, GetUntracked};
use leptos::prelude::{use_context, ArcRwSignal, Callback, ClassAttribute, Effect, Get, NodeRef, NodeRefAttribute, OnAttribute, Set, StyleAttribute, Update, Write};
use crate::front::modules::components::{Backable, Cache, Cacheable};
use leptos::prelude::{AnyView, CollectView, ElementChild, IntoAny, Read, RwSignal};
use leptos::{view, IntoView};
use leptos::ev::MouseEvent;
use leptos::html::{Div, I};
use leptos_use::{use_draggable_with_options, use_mouse_in_element, UseDraggableOptions, UseDraggableReturn, UseMouseInElementReturn};
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::front::utils::dialog::DialogManager;
use crate::HWebTrace;

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

	pub fn push(&mut self, new: Link)
	{
		self.content.write().push(new);
		self._update.update(|cache|{
			cache.update();
		});
	}

	/*pub fn getAll(&self) -> &Vec<Link>
	{
		return &self.content.read();
	}*/

	pub fn move_up(&mut self, itemPos: usize)
	{
		if itemPos > 0
		{
			self.content.write().swap(itemPos, itemPos - 1);
		}
		self._update.update(|cache|{
			cache.update();
		});
	}

	pub fn move_down(&mut self, itemPos: usize)
	{
		if itemPos + 1 < self.content.read().len()
		{
			self.content.write().swap(itemPos, itemPos + 1);
		}
		self._update.update(|cache|{
			cache.update();
		});
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
	                      content: ArcRwSignal<Vec<Link>>) -> impl IntoView
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

		let fnRemove = Self::removeLinkPopupFn(content,pos);


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

	fn removeLinkPopupFn(content: ArcRwSignal<Vec<Link>>, pos: usize) -> impl Fn(MouseEvent)
	{
		let dialog = use_context::<DialogManager>().expect("DialogManager missing");
		let content = content.clone();

		return move |_| {
			let content = content.clone();
			dialog.open("Supprimer un lien ?", move || {
				view!{
				<span/>
			}.into_any()
			}, Some(Callback::new(move |_| {
				content.update(|links|{
					links.remove(pos);
				});
			})), None);
		};
	}

	fn addLinkPopupFn(&self) -> impl Fn(MouseEvent)
	{
		let dialog = use_context::<DialogManager>().expect("DialogManager missing");
		let content = self.content.clone();

		return move |_| {
			let label = ArcRwSignal::new("".to_string());
			let url = ArcRwSignal::new("".to_string());

			let labelDialog = label.clone();
			let urlDialog = url.clone();
			let content = content.clone();
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
				content.update(|links|{
					links.push(Link::new(label.clone().get(),url.clone().get()));
				});
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
	fn name() -> String
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
						{return Self::draw_editable_link(&link,key,draggedOriginPosition,draggedTargetPosition,somethingIsDragging, self.content.clone()).into_any();}
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
