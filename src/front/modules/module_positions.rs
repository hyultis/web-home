use leptos::html::I;
use leptos::prelude::{ArcRwSignal, Callable, Effect, ElementChild, Get, NodeRef, NodeRefAttribute, OnAttribute, Set, StyleAttribute, Update};
use leptos::prelude::{AnyView, ClassAttribute, IntoAny, RwSignal};
use leptos::{view};
use leptos_use::core::Position;
use leptos_use::{use_draggable_with_options, UseDraggableOptions, UseDraggableReturn};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::{moduleContent};
use crate::front::modules::module_actions::ModuleActionFn;

pub struct ModulePositions<module: moduleContent>
{
	_pos: ArcRwSignal<[i32;2]>,
	_size: ArcRwSignal<[u32;2]>,
	_module: module
}

impl<module: moduleContent> ModulePositions<module>
{
	pub fn new(module: module) -> Self
	{
		Self {
			_pos: ArcRwSignal::new([0,0]),
			_size: ArcRwSignal::new([100,100]),
			_module: module
		}
	}

	pub fn newFromModuleContent(from: ModuleContent, module: module) -> Self
	{
		Self {
			_pos: ArcRwSignal::new(from.pos.clone()),
			_size: ArcRwSignal::new(from.size.clone()),
			_module: module
		}
	}

	pub fn inner(&self) -> &module
	{
		return &self._module;
	}

	fn intoStyle(pos: [i32;2], size: [u32;2]) -> String
	{
		return format!("position: absolute; left: {}px; top: {}px; width: {}px; height: {}px;", pos[0], pos[1], size[0], size[1]);
	}

	pub fn export(&self) -> ModuleContent
	{
		let mut export = self._module.export();
		export.pos = self._pos.get();
		export.size = self._size.get();
		return export;
	}

	pub fn import(&mut self, import: ModuleContent)
	{
		self._pos.update(|pos|{
			pos[0] = import.pos[0];
			pos[1] = import.pos[1];
		});
		self._size.update(|size|{
			size[0] = import.size[0];
			size[1] = import.size[1];
		});
		self._module.import(import);
	}

	pub fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn,currentName:String) -> AnyView
	{
		let innerModuleActions = moduleActions.clone();
		let innerCurrentName = currentName.clone();
		let view = move |module: &module,editMode| {
			return module.draw(editMode,innerModuleActions.clone(),innerCurrentName.clone());
		};

		let pos = self._pos.clone();
		let size = self._size.clone();

		let elMove = self.moveFn();
		let elResize = self.resizeFn();

		let innerModuleActions = moduleActions.clone();
		let innerCurrentName = currentName.clone();
		let removeFn = move |_| {
			innerModuleActions.clone().removeFn.run(innerCurrentName.clone());
		};

		view! {
			{
				if editMode.get()
				{
					view!{<div style={move || Self::intoStyle(pos.clone().get(), size.clone().get())}>
						<div class="module">
						<div class="module_header"><i class="iconoir-path-arrow-solid" node_ref=elMove></i><i class="iconoir-xmark" on:click=removeFn></i></div>
						{view(&self._module,editMode)}
						</div>
						<i class="iconoir-arrow-down-right-square grabbable" node_ref=elResize style="float: right; margin-top: -0.9em; margin-right: -0.1em"></i>
					</div>}.into_any()
				}
				else
				{
					view!{<div class="module" style={move || Self::intoStyle(pos.clone().get(), size.clone().get())}>
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
		let cache = self._module.cache_getUpdate();
		Effect::new(move |_| {
			if !is_dragging.get() {return;}
			let newPosX = position.get().x as i32 -8; // 8 is the half-width of the icon
			let newPosY = position.get().y as i32 -8 -30; // -30 is the header bar height ... TODO : need to automatise that
			cache.update(|data| data.update());
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
		let size = self._module.size();
		let cache = self._module.cache_getUpdate();
		Effect::new(move |_| {
			if !is_dragging.get() {return;}
			let mut newSizeX = position.get().x as i32 - posMove.clone().get()[0] + 8; // 8 is the half-width of the icon
			if let Some(max) = size.x_max {
				if(newSizeX > max as i32) {newSizeX = max as i32}
			}
			if let Some(min) = size.x_min {
				if (newSizeX < min as i32) { newSizeX = min as i32 }
			}

			let mut newSizeY = position.get().y as i32 - posMove.clone().get()[1] + 8 - 30; // -30 is the header bar height ... TODO : need to automatise that
			if let Some(max) = size.y_max {
				if(newSizeY > max as i32) {newSizeY = max as i32}
			}
			if let Some(min) = size.y_min {
				if(newSizeY < min as i32) {newSizeY = min as i32}
			}
			cache.update(|data| data.update());
			posSize.update(|size|{
				size[0] = newSizeX.max(150) as u32;
				size[1] = newSizeY.max(150) as u32;
			});
		});

		return el;
	}
}