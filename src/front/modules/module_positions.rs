use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{moduleContent, Cache, ModuleSizeContrainte};
use crate::front::modules::module_actions::ModuleActionFn;
use leptos::html::I;
use leptos::logging::log;
use leptos::prelude::{AnyView, ClassAttribute, IntoAny, RwSignal};
use leptos::prelude::{
	ArcRwSignal, Effect, ElementChild, Get, NodeRef, NodeRefAttribute, OnAttribute, Set,
	StyleAttribute, Update, ViewFn,
};
use leptos::{component, view, IntoView};
use leptos_use::core::Position;
use leptos_use::{use_draggable_with_options, UseDraggableOptions, UseDraggableReturn};

pub struct ModulePositions<module: moduleContent>
{
	_pos: ArcRwSignal<[i32; 2]>,
	_size: ArcRwSignal<[u32; 2]>,
	_depth: ArcRwSignal<u32>,
	_module: module,
}

impl<module: moduleContent> ModulePositions<module>
{
	pub fn new(module: module) -> Self
	{
		Self {
			_pos: ArcRwSignal::new([0, 0]),
			_size: ArcRwSignal::new([100, 100]),
			_depth: Default::default(),
			_module: module,
		}
	}

	pub fn newFromModuleContent(from: ModuleContent, module: module) -> Self
	{
		Self {
			_pos: ArcRwSignal::new(from.pos.clone()),
			_size: ArcRwSignal::new(from.size.clone()),
			_depth: ArcRwSignal::new(from.depth.clone()),
			_module: module,
		}
	}

	pub fn depth_set(&self, depth: u32)
	{
		self._depth.set(depth);
	}

	pub fn inner(&self) -> &module
	{
		return &self._module;
	}

	pub fn export(&self) -> ModuleContent
	{
		let mut export = self._module.export();
		export.pos = self._pos.get();
		export.size = self._size.get();
		export.depth = self._depth.get();
		return export;
	}

	pub fn import(&mut self, import: ModuleContent)
	{
		if (!self._module.isOlderThan(&import))
		{
			return;
		}

		self._pos.update(|pos| {
			pos[0] = import.pos[0];
			pos[1] = import.pos[1];
		});
		self._size.update(|size| {
			size[0] = import.size[0];
			size[1] = import.size[1];
		});
		self._depth.set(import.depth);
		self._module.import(import);
	}

	pub fn draw(
		&self,
		editMode: RwSignal<bool>,
		moduleActions: ModuleActionFn,
		moduleId: ModuleID,
	) -> AnyView
	{
		let innerView = self
			._module
			.draw(editMode, moduleActions.clone(), moduleId.clone());
		let cache = self._module.cache_getUpdate();
		let constraints = self._module.size();

		view! {
			<ModulePositionDraw
				pos=self._pos.clone()
				size=self._size.clone()
				depth=self._depth.clone()
				editMode=editMode
				cache=cache
				constraints=constraints
				moduleActions=moduleActions
				moduleId=moduleId
				innerView=innerView
			/>
		}
		.into_any()
	}

	fn moveFn(&self) -> NodeRef<I>
	{
		let startDragPos = RwSignal::new(Position::default());
		let el = NodeRef::<I>::new();
		let mut config = UseDraggableOptions::default();
		config = config.exact(true);
		config = config.on_start(move |d| {
			startDragPos.set(d.position);
			true
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			position,
			is_dragging,
			..
		} = use_draggable_with_options(el, config);

		let posMove = self._pos.clone();
		let cache = self._module.cache_getUpdate();
		Effect::new(move |_| {
			if !is_dragging.get()
			{
				return;
			}
			let newPosX = position.get().x as i32 - 8; // 8 is the half-width of the icon
			let newPosX = newPosX.max(0);
			let newPosY = position.get().y as i32 - 8 - 30; // -30 is the header bar height ... TODO : need to automatise that
			let newPosY = newPosY.max(0);
			cache.update(|data| data.update());
			posMove.update(|size| {
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
		config = config.on_start(move |d| {
			startDragPos.set(d.position);
			true
		});

		// `style` is a helper string "left: {x}px; top: {y}px;"
		let UseDraggableReturn {
			position,
			is_dragging,
			..
		} = use_draggable_with_options(el, config);

		let posMove = self._pos.clone();
		let posSize = self._size.clone();
		let size = self._module.size();
		let cache = self._module.cache_getUpdate();
		Effect::new(move |_| {
			if !is_dragging.get()
			{
				return;
			}
			let mut newSizeX = position.get().x as i32 - posMove.clone().get()[0] + 8; // 8 is the half-width of the icon
			if let Some(max) = size.x_max
			{
				if (newSizeX > max as i32)
				{
					newSizeX = max as i32
				}
			}
			if let Some(min) = size.x_min
			{
				if (newSizeX < min as i32)
				{
					newSizeX = min as i32
				}
			}

			let mut newSizeY = position.get().y as i32 - posMove.clone().get()[1] + 8 - 30; // -30 is the header bar height ... TODO : need to automatise that
			if let Some(max) = size.y_max
			{
				if (newSizeY > max as i32)
				{
					newSizeY = max as i32
				}
			}
			if let Some(min) = size.y_min
			{
				if (newSizeY < min as i32)
				{
					newSizeY = min as i32
				}
			}
			cache.update(|data| data.update());
			posSize.update(|size| {
				size[0] = newSizeX.max(150) as u32;
				size[1] = newSizeY.max(150) as u32;
			});
		});

		return el;
	}
}

#[component]
fn ModulePositionDraw(
	pos: ArcRwSignal<[i32; 2]>,
	size: ArcRwSignal<[u32; 2]>,
	depth: ArcRwSignal<u32>,
	editMode: RwSignal<bool>,
	cache: ArcRwSignal<Cache>,
	constraints: ModuleSizeContrainte,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	innerView: ViewFn,
) -> impl IntoView
{
	let el_move = NodeRef::<I>::new();
	let el_resize = NodeRef::<I>::new();

	let start_drag_pos_move = RwSignal::new(Position::default());
	let mut config_move = UseDraggableOptions::default().exact(true);
	config_move = config_move.on_start(move |d| {
		start_drag_pos_move.set(d.position);
		true
	});

	let UseDraggableReturn {
		position: move_position,
		is_dragging: is_dragging_move,
		..
	} = use_draggable_with_options(el_move, config_move);

	Effect::new({
		let pos = pos.clone();
		let cache = cache.clone();
		move |_| {
			if !is_dragging_move.get()
			{
				return;
			}

			let new_x = (move_position.get().x as i32 - 8).max(0);
			let new_y = (move_position.get().y as i32 - 8 - 30).max(0);

			cache.update(|c| c.update());
			pos.update(|p| {
				p[0] = new_x;
				p[1] = new_y;
			});
		}
	});

	let start_drag_pos_resize = RwSignal::new(Position::default());
	let mut config_resize = UseDraggableOptions::default().exact(true);
	config_resize = config_resize.on_start(move |d| {
		start_drag_pos_resize.set(d.position);
		true
	});

	let UseDraggableReturn {
		position: resize_position,
		is_dragging: is_dragging_resize,
		..
	} = use_draggable_with_options(el_resize, config_resize);

	Effect::new({
		let pos = pos.clone();
		let size = size.clone();
		let cache = cache.clone();

		move |_| {
			if !is_dragging_resize.get()
			{
				return;
			}

			let current_pos = pos.get();

			let mut new_x = resize_position.get().x as i32 - current_pos[0] + 8;
			if let Some(max) = constraints.x_max
			{
				if new_x > max as i32
				{
					new_x = max as i32;
				}
			}
			if let Some(min) = constraints.x_min
			{
				if new_x < min as i32
				{
					new_x = min as i32;
				}
			}

			let mut new_y = resize_position.get().y as i32 - current_pos[1] + 8 - 30;
			if let Some(max) = constraints.y_max
			{
				if new_y > max as i32
				{
					new_y = max as i32;
				}
			}
			if let Some(min) = constraints.y_min
			{
				if new_y < min as i32
				{
					new_y = min as i32;
				}
			}

			cache.update(|c| c.update());
			size.update(|s| {
				s[0] = new_x.max(150) as u32;
				s[1] = new_y.max(150) as u32;
			});
		}
	});

	let remove_fn = {
		let module_actions = moduleActions.clone();
		let module_id = moduleId.clone();
		move |_| {
			(module_actions.removeFn)(module_id.clone());
		}
	};

	view! {
		{move || {
			let style = intoStyle(
				pos.get(),
				size.get(),
				depth.get(),
			);

			if editMode.get() {
				view! {
					<div style=style>
						<div class="module">
							<div class="module_header">
								<i class="iconoir-path-arrow-solid" node_ref=el_move></i>
								<i class="iconoir-xmark" on:click=remove_fn.clone()></i>
							</div>
							{innerView.run()}
						</div>
						<i class="iconoir-arrow-down-right-square grabbable"
						   node_ref=el_resize
						   style="float: right; margin-top: -0.9em; margin-right: -0.1em"></i>
					</div>
				}.into_any()
			} else {
				view! {
					<div class="module" style=style>
						{innerView.run()}
					</div>
				}.into_any()
			}
		}}
	}
}

fn intoStyle(pos: [i32; 2], size: [u32; 2], depth: u32) -> String
{
	return format!(
		"position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; z-index: {}",
		pos[0], pos[1], size[0], size[1], depth
	);
}
