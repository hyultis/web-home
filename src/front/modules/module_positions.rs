use leptos::html::I;
use leptos::prelude::{ArcRwSignal, Effect, ElementChild, Get, NodeRef, NodeRefAttribute, OnAttribute, Set, StyleAttribute, Update};
use leptos::prelude::{AnyView, ClassAttribute, IntoAny, RwSignal};
use leptos::{view};
use leptos_use::core::Position;
use leptos_use::{use_draggable_with_options, UseDraggableOptions, UseDraggableReturn};
use crate::front::modules::components::Backable;
use crate::front::modules::ModuleType;

pub struct ModulePositions<ModuleType>
{
	_pos: ArcRwSignal<[i32;2]>,
	_size: ArcRwSignal<[u32;2]>,
	_module: ModuleType
}

impl ModulePositions<ModuleType>
{
	pub fn new(module: ModuleType) -> Self
	{
		Self {
			_pos: ArcRwSignal::new([0,0]),
			_size: ArcRwSignal::new([100,100]),
			_module: module
		}
	}

	pub fn inner(&self) -> &ModuleType
	{
		return &self._module;
	}

	fn intoStyle(pos: ArcRwSignal<[i32;2]>, size: ArcRwSignal<[u32;2]>) -> String
	{
		let pos = pos.get();
		let size = size.get();
		return format!("left: {}px; top: {}px; width: {}px; height: {}px;", pos[0], pos[1], size[0], size[1]);
	}

	pub fn draw(&self, editMode: RwSignal<bool>) -> AnyView
	{
		let view = move |module: &ModuleType,editMode| {
			match module {
				ModuleType::RSS(_) => view!{<div>{"RSS"}</div>}.into_any(),
				ModuleType::TODO(todo) => todo.draw(editMode)
			}
		};

		let pos = self._pos.clone();
		let size = self._size.clone();

		let elMove = self.moveFn();
		let elResize = self.resizeFn();

		let removeFn = move |ev| {

		};

		view! {
			{
				if editMode.get()
				{
					view!{<div style={move || format!("position: relative;{}",Self::intoStyle(pos.clone(), size.clone()))}>
						<div class="module">
						<div class="module_header"><i class="iconoir-path-arrow-solid" node_ref=elMove></i><i class="iconoir-xmark" on:click=removeFn></i></div>
						{view(&self._module,editMode)}
						</div>
						<i class="iconoir-arrow-down-right-square grabbable" node_ref=elResize style="float: right; margin-top: -0.9em; margin-right: -0.1em"></i>
					</div>}.into_any()
				}
				else
				{
					view!{<div class="module" style={move || Self::intoStyle(pos.clone(), size.clone())}>
					{view(&self._module,editMode)}
					</div>
					}.into_any()
				}
			}
		}.into_any()
	}

	fn moveFn(&self) -> NodeRef<I>
	{
		let startDragPos = RwSignal::new(Position::default());
		let el = NodeRef::<I>::new();
		let mut config = UseDraggableOptions::default();
		config = config.exact(true);
		config = config.on_start( move |d| {
			startDragPos.set(d.position);
			true
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			position,
			is_dragging,
			..
		} = use_draggable_with_options(el,config);

		let posMove = self._pos.clone();
		Effect::new(move |_| {
			if !is_dragging.get() {return;}
			let newPosX = position.get().x as i32;
			let newPosY = position.get().y as i32;
			posMove.update(|size|{
				size[0] = newPosX;
				size[1] = newPosY;
			});
		});

		return el;
	}

	fn resizeFn(&self) -> NodeRef<I>
	{
		let startDragPos = RwSignal::new(Position::default());
		let el = NodeRef::<I>::new();
		let mut config = UseDraggableOptions::default();
		config = config.exact(true);
		config = config.on_start( move |d| {
			startDragPos.set(d.position);
			true
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			position,
			is_dragging,
			..
		} = use_draggable_with_options(el,config);

		let posMove = self._pos.clone();
		let posSize = self._size.clone();
		Effect::new(move |_| {
			if !is_dragging.get() {return;}
			let newSizeX = position.get().x as i32 - posMove.clone().get()[0];
			let newSizeY = position.get().y as i32 - posMove.clone().get()[1];
			posSize.update(|size|{
				size[0] = newSizeX.max(150) as u32;
				size[1] = newSizeY.max(150) as u32;
			});
		});

		return el;
	}
}