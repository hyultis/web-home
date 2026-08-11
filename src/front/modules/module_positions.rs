use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{moduleContent, Cache, ModuleSizeContrainte};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::TranslateText;
use leptos::html::Button;
use leptos::prelude::{AnyView, ClassAttribute, IntoAny, RwSignal};
use leptos::prelude::{
	ArcRwSignal, ElementChild, Get, GetUntracked, NodeRef, NodeRefAttribute, OnAttribute,
	Set, StyleAttribute, Update, ViewFn,
};
use leptos::{component, view, IntoView};
use leptos_use::{use_draggable_with_options, UseDraggableOptions};

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
	let el_move = NodeRef::<Button>::new();
	let el_resize = NodeRef::<Button>::new();

	let moveOffset = pos.clone();
	let movePosition = pos.clone();
	let moveCache = cache.clone();
	let config_move = UseDraggableOptions::default()
		.exact(true)
		.prevent_default(true)
		.target_offset(move |_| {
			let currentPosition = moveOffset.get_untracked();
			(currentPosition[0] as f64,currentPosition[1] as f64)
		})
		.on_move(move |drag| {
			moveCache.update(|cache| cache.update());
			movePosition.update(|position| {
				position[0] = (drag.position.x.round() as i32).max(0);
				position[1] = (drag.position.y.round() as i32).max(0);
			});
		});
	let _moveDraggable = use_draggable_with_options(el_move, config_move);

	let resizeOffset = size.clone();
	let resizeSize = size.clone();
	let resizeCache = cache.clone();
	let xMin = constraints.x_min;
	let xMax = constraints.x_max;
	let yMin = constraints.y_min;
	let yMax = constraints.y_max;
	let config_resize = UseDraggableOptions::default()
		.exact(true)
		.prevent_default(true)
		.target_offset(move |_| {
			let currentSize = resizeOffset.get_untracked();
			(currentSize[0] as f64,currentSize[1] as f64)
		})
		.on_move(move |drag| {
			let mut newWidth = (drag.position.x.round() as i32).max(150);
			let mut newHeight = (drag.position.y.round() as i32).max(150);

			if let Some(max) = xMax
			{
				newWidth = newWidth.min(max as i32);
			}
			if let Some(min) = xMin
			{
				newWidth = newWidth.max(min as i32);
			}
			if let Some(max) = yMax
			{
				newHeight = newHeight.min(max as i32);
			}
			if let Some(min) = yMin
			{
				newHeight = newHeight.max(min as i32);
			}

			resizeCache.update(|cache| cache.update());
			resizeSize.update(|size| {
				size[0] = newWidth as u32;
				size[1] = newHeight as u32;
			});
		});
	let _resizeDraggable = use_draggable_with_options(el_resize, config_resize);

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
					<div class="module_position" style=style>
						<div class="module module--editing">
							<div class="module_header">
								<button type="button" class="module_handle module_move_handle" node_ref=el_move>
									<i class="iconoir-path-arrow-solid" aria-hidden="true"></i>
									<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_MOVE_ACTION"/></span>
								</button>
								<button type="button" class="module_handle module_remove_button" on:click=remove_fn.clone()>
									<i class="iconoir-xmark" aria-hidden="true"></i>
									<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_REMOVE_ACTION"/></span>
								</button>
							</div>
							<div class="module_content">{innerView.run()}</div>
						</div>
						<button type="button" class="module_handle module_resize_handle" node_ref=el_resize>
							<i class="iconoir-arrow-down-right-square" aria-hidden="true"></i>
							<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_RESIZE_ACTION"/></span>
						</button>
					</div>
				}.into_any()
			} else {
				view! {
					<div class="module module_position" style=style>
						<div class="module_content">{innerView.run()}</div>
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
