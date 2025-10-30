use leptos::prelude::{ArcRwSignal, ClassAttribute, Effect, Get, NodeRef, NodeRefAttribute, OnAttribute, Set, StyleAttribute, Write};
use crate::front::modules::components::{Backable, Cache, Cacheable};
use leptos::prelude::{AnyView, CollectView, ElementChild, IntoAny, Read, RwSignal};
use leptos::{view, IntoView};
use leptos::html::{Div, I};
use leptos_use::{use_draggable_with_options, use_mouse_in_element, UseDraggableOptions, UseDraggableReturn, UseMouseInElementReturn};
use serde::{Deserialize, Serialize};
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
	_cache: Cache,
}

impl LinksHolder
{
	pub fn new() -> Self
	{
		Self {
			content: ArcRwSignal::new(vec![]),
			_cache: Default::default(),
		}
	}

	pub fn push(&mut self, new: Link)
	{
		self.content.write().push(new);
		self._cache.update();
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
		self._cache.update();
	}

	pub fn move_down(&mut self, itemPos: usize)
	{
		if itemPos + 1 < self.content.read().len()
		{
			self.content.write().swap(itemPos, itemPos + 1);
		}
		self._cache.update();
	}

	fn draw_link(link: &Link) -> impl IntoView
	{
		return view! {
			<a href={link.url.clone()} rel="noopener noreferrer nofollow" target="_blank">{link.label.clone()}</a>
		};
	}

	fn draw_editable_link(link: &Link, pos: usize, draggedOriginPosition: RwSignal<Option<usize>>, draggedTargetPosition: RwSignal<Option<usize>>, somethingIsDragging: RwSignal<bool>) -> impl IntoView
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


		return view! {
			<div class="button ghost" style=move || {
				let mut show = "display: none;";
				if(is_dragging.get()) {show = "display: inline-block;";}
				format!("position: fixed; {} {}", style.get(),show)
			}>
				<i  class="iconoir-arrow-separate"></i>{link.label.clone()}
			</div>
			<div class={move || {
				let mut classDrop = "";
				let targetPos = draggedOriginPosition.get().map(|x| x as i32).unwrap_or(-1);
				if somethingIsDragging.get() && !is_outside.get() && targetPos != pos as i32 {classDrop = " drop"}
				format!("button{}",classDrop)
			}} node_ref=target>
				<i node_ref=el class="iconoir-arrow-separate" on:click={move |_| {}}></i>{link.label.clone()}
			</div>
		};
	}
}

impl Cacheable for LinksHolder
{
	fn cache_get(&self) -> &Cache
	{
		&self._cache
	}

	fn cache_get_mut(&mut self) -> &mut Cache
	{
		&mut self._cache
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
		let draggedOriginPosition: RwSignal<Option<usize>> = RwSignal::new(None);
		let draggedTargetPosition: RwSignal<Option<usize>> = RwSignal::new(None);
		let somethingIsDragging: RwSignal<bool> = RwSignal::new(false);

		let content = self.content.clone();
		Effect::new(move |_| {
			let Some(newTarget) = draggedTargetPosition.get() else {return};
			let Some(Origin) = draggedOriginPosition.get() else {return};
			if somethingIsDragging.get() {return};

			draggedTargetPosition.set(None);
			draggedOriginPosition.set(None);

			content.clone().write().swap(Origin, newTarget);

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
						{return Self::draw_editable_link(link,key,draggedOriginPosition,draggedTargetPosition,somethingIsDragging).into_any();}
					else
						{return Self::draw_link(link).into_any();}
				)
				.collect_view()
			}
			{editMode.read().then(|| view!{<div class="button add" on:click={move |_| {}}><i class="iconoir-plus-circle"></i>add</div>})}
			</div>
		}.into_any()
	}

	fn export(&self) -> String
	{
		return serde_json::to_string(&self.content).unwrap();
	}

	fn import(&mut self, importSerial: String)
	{
		if let Ok(content) = serde_json::from_str(&importSerial)
		{
			self.content = content;
		}
	}
}
