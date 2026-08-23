use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{moduleContent, Cache, ModuleConfigSession, ModuleConfigViewFn, ModuleSizeContrainte};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::dialog::{DialogData,DialogManager};
use crate::front::utils::translate::TranslateText;
use leptos::html::Button;
use leptos::prelude::{AnyView, ClassAttribute, IntoAny, RwSignal};
use leptos::prelude::{
	ArcRwSignal, ElementChild, Get, GetUntracked, GlobalAttributes, NodeRef, NodeRefAttribute, OnAttribute,
	Set, StyleAttribute, Update, ViewFn, expect_context,
};
use leptos::{component, view, IntoView};
use leptos_use::{use_draggable_with_options, UseDraggableOptions};
use std::collections::{HashSet,VecDeque};

const MODULE_LAYOUT_GAP: i32 = 12;
const MODULE_LAYOUT_SEARCH_LIMIT: usize = 4_096;
const MODULE_LAYOUT_DEFAULT_MINIMUM_SIZE: u32 = 150;

#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
pub(super) struct ModuleRect
{
	pub x: i32,
	pub y: i32,
	pub width: u32,
	pub height: u32,
}

impl ModuleRect
{
	pub fn new(position: [i32;2],size: [u32;2]) -> Self
	{
		return Self {
			x: position[0].max(0),
			y: position[1].max(0),
			width: size[0],
			height: size[1],
		};
	}

	pub fn position_get(self) -> [i32;2]
	{
		return [self.x,self.y];
	}

	fn right_get(self) -> i64
	{
		return i64::from(self.x) + i64::from(self.width);
	}

	fn bottom_get(self) -> i64
	{
		return i64::from(self.y) + i64::from(self.height);
	}

	pub fn intersects(self,other: Self) -> bool
	{
		return i64::from(self.x) < other.right_get()
			&& self.right_get() > i64::from(other.x)
			&& i64::from(self.y) < other.bottom_get()
			&& self.bottom_get() > i64::from(other.y);
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(super) struct ModulePlacement
{
	pub position: [i32;2],
	pub depth: u32,
}

#[derive(Clone,Copy)]
pub(super) enum ModulePlacementMode
{
	Fixed,
	Nearest,
}

pub(super) fn modulePosition_freeFind(requested: ModuleRect,occupied: &[ModuleRect]) -> Option<[i32;2]>
{
	let requested = ModuleRect::new(requested.position_get(),[requested.width,requested.height]);
	let mut occupied = occupied.to_vec();
	occupied.sort_by_key(|rect| (rect.y,rect.x,rect.height,rect.width));
	let mut pending = VecDeque::from([requested.position_get()]);
	let mut visited = HashSet::from([requested.position_get()]);
	for _ in 0..MODULE_LAYOUT_SEARCH_LIMIT
	{
		let position = pending.pop_front()?;
		let candidate = ModuleRect::new(position,[requested.width,requested.height]);
		let collisions = occupied.iter().copied()
			.filter(|occupied| candidate.intersects(*occupied))
			.collect::<Vec<_>>();
		if (collisions.is_empty())
		{
			return Some(position);
		}

		for collision in collisions.iter().rev()
		{
			let nextY = collision.bottom_get() + i64::from(MODULE_LAYOUT_GAP);
			if let Ok(nextY) = i32::try_from(nextY)
			{
				let next = [position[0],nextY.max(requested.y)];
				if (visited.insert(next)) {pending.push_front(next);}
			}
		}
		for collision in &collisions
		{
			let nextX = collision.right_get() + i64::from(MODULE_LAYOUT_GAP);
			if let Ok(nextX) = i32::try_from(nextX)
			{
				let next = [nextX.max(requested.x),position[1]];
				if (visited.insert(next)) {pending.push_back(next);}
			}
		}
	}
	return None;
}

pub(super) fn modulePosition_nearestFreeFind(requested: ModuleRect,occupied: &[ModuleRect]) -> Option<[i32;2]>
{
	let requested = ModuleRect::new(requested.position_get(),[requested.width,requested.height]);
	let mut xCoordinates = HashSet::from([requested.x,0]);
	let mut yCoordinates = HashSet::from([requested.y,0]);
	for occupied in occupied
	{
		let left = i64::from(occupied.x) - i64::from(requested.width) - i64::from(MODULE_LAYOUT_GAP);
		if let Ok(left) = i32::try_from(left)
		{
			if (left >= 0) {xCoordinates.insert(left);}
		}
		let right = occupied.right_get() + i64::from(MODULE_LAYOUT_GAP);
		if let Ok(right) = i32::try_from(right)
		{
			xCoordinates.insert(right);
		}

		let above = i64::from(occupied.y) - i64::from(requested.height) - i64::from(MODULE_LAYOUT_GAP);
		if let Ok(above) = i32::try_from(above)
		{
			if (above >= 0) {yCoordinates.insert(above);}
		}
		let below = occupied.bottom_get() + i64::from(MODULE_LAYOUT_GAP);
		if let Ok(below) = i32::try_from(below)
		{
			yCoordinates.insert(below);
		}
	}

	let mut candidates = xCoordinates.into_iter()
		.flat_map(|x| yCoordinates.iter().copied().map(move |y| [x,y]))
		.collect::<Vec<_>>();
	candidates.sort_by_key(|position| modulePosition_distanceKey(*position,requested.position_get()));
	for position in candidates.into_iter().take(MODULE_LAYOUT_SEARCH_LIMIT)
	{
		let candidate = ModuleRect::new(position,[requested.width,requested.height]);
		if (!occupied.iter().any(|occupied| candidate.intersects(*occupied)))
		{
			return Some(position);
		}
	}
	return None;
}

fn modulePosition_distanceKey(position: [i32;2],requested: [i32;2]) -> (i128,i64,i32,i32)
{
	let xDistance = i64::from(position[0]) - i64::from(requested[0]);
	let yDistance = i64::from(position[1]) - i64::from(requested[1]);
	return (
		i128::from(xDistance) * i128::from(xDistance) + i128::from(yDistance) * i128::from(yDistance),
		xDistance.abs().saturating_add(yDistance.abs()),position[1],position[0],
	);
}

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
		let constraints = module.size();
		Self {
			_pos: ArcRwSignal::new([0, 0]),
			_size: ArcRwSignal::new([
				constraints.x_min.unwrap_or(MODULE_LAYOUT_DEFAULT_MINIMUM_SIZE),
				constraints.y_min.unwrap_or(MODULE_LAYOUT_DEFAULT_MINIMUM_SIZE),
			]),
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

	pub(super) fn rect_get(&self) -> ModuleRect
	{
		return ModuleRect::new(self._pos.get_untracked(),self._size.get_untracked());
	}

	pub(super) fn position_set(&self,position: [i32;2])
	{
		self._pos.set([position[0].max(0),position[1].max(0)]);
	}

	pub(super) fn depth_get(&self) -> u32
	{
		return self._depth.get_untracked();
	}

	pub(super) fn visual_order_get(&self) -> (i32,i32,u32)
	{
		let position = self._pos.get();
		return (position[1],position[0],self._depth.get());
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
		let configView = self._module.draw_config(moduleActions.clone(),moduleId.clone());
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
				configView=configView
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
	configView: Option<ModuleConfigViewFn>,
) -> impl IntoView
{
	let dialogManager = expect_context::<DialogManager>();
	let el_move = NodeRef::<Button>::new();
	let el_resize = NodeRef::<Button>::new();

	let moveOffset = pos.clone();
	let movePosition = pos.clone();
	let moveSize = size.clone();
	let moveDepth = depth.clone();
	let moveCache = cache.clone();
	let moveResolve = moduleActions.layoutResolveFn.clone();
	let moveModuleId = moduleId.clone();
	let config_move = UseDraggableOptions::default()
		.exact(true)
		.prevent_default(true)
		.target_offset(move |_| {
			let currentPosition = moveOffset.get_untracked();
			(currentPosition[0] as f64,currentPosition[1] as f64)
		})
		.on_move(move |drag| {
			let requested = [
				(drag.position.x.round() as i32).max(0),
				(drag.position.y.round() as i32).max(0),
			];
			let Some(placement) = moveResolve(
				moveModuleId.clone(),requested,moveSize.get_untracked(),ModulePlacementMode::Nearest,
			)
			else {return};
			moveCache.update(|cache| cache.update());
			movePosition.set(placement.position);
			moveDepth.set(placement.depth);
		});
	let _moveDraggable = use_draggable_with_options(el_move, config_move);

	let resizeOffset = size.clone();
	let resizeSize = size.clone();
	let resizePosition = pos.clone();
	let resizeCache = cache.clone();
	let resizeResolve = moduleActions.layoutResolveFn.clone();
	let resizeModuleId = moduleId.clone();
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
			let mut newWidth = (drag.position.x.round() as i32).max(MODULE_LAYOUT_DEFAULT_MINIMUM_SIZE as i32);
			let mut newHeight = (drag.position.y.round() as i32).max(MODULE_LAYOUT_DEFAULT_MINIMUM_SIZE as i32);

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

			let newSize = [newWidth as u32,newHeight as u32];
			if (resizeResolve(
				resizeModuleId.clone(),resizePosition.get_untracked(),newSize,ModulePlacementMode::Fixed,
			).is_none()) {return;}
			resizeCache.update(|cache| cache.update());
			resizeSize.set(newSize);
		});
	let _resizeDraggable = use_draggable_with_options(el_resize, config_resize);

	let remove_fn = {
		let module_actions = moduleActions.clone();
		let module_id = moduleId.clone();
		move |_| {
			(module_actions.removeFn)(module_id.clone());
		}
	};
	let configDialogManager = dialogManager.clone();
	let configDialogActions = moduleActions.clone();
	let configDialogId = moduleId.clone();
	let configViewOpen = configView.clone();

	view! {
		{move || {
			let style = intoStyle(
				pos.get(),
				size.get(),
				depth.get(),
			);

			if editMode.get() {
				let configButton = configViewOpen.clone().map(|configView| {
					let dialogManager = configDialogManager.clone();
					let moduleActions = configDialogActions.clone();
					let moduleId = configDialogId.clone();
					view! {
						<button type="button" class="module_handle module_config_button" on:click=move |_| {
							let session = ModuleConfigSession::new(moduleActions.clone(),moduleId.clone());
							let bodySession = session.clone();
							let closeSession = session.clone();
							let bodyView = configView.clone();
							let dialog = DialogData::new()
								.setTitle("FRONTUI_MODULE_CONFIGURATION")
								.setBody(move || bodyView(bodySession.clone()))
								.setButtonValidateTitle(None::<String>)
								.setButtonCloseTitle(Some("FRONTUI_UPDATE"))
								.setOnClose(move |_| closeSession.close());
							dialogManager.open(dialog);
						}>
							<i class="iconoir-settings" aria-hidden="true"></i>
							<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_CONFIGURE_ACTION"/></span>
						</button>
					}
				});
				view! {
					<div class="module_position" style=style>
						<div class="module module--editing">
							<div class="module_header">
								<button type="button" class="module_handle module_move_handle" node_ref=el_move>
									<i class="iconoir-path-arrow-solid" aria-hidden="true"></i>
									<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_MOVE_ACTION"/></span>
								</button>
								<div class="module_header_actions">
									{configButton}
									<button type="button" class="module_handle module_remove_button" on:click=remove_fn.clone()>
										<i class="iconoir-xmark" aria-hidden="true"></i>
										<span class="visually_hidden"><TranslateText key="FRONTUI_MODULE_REMOVE_ACTION"/></span>
									</button>
								</div>
							</div>
							<div class="module_content module_content--preview" inert=true>{innerView.run()}</div>
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

#[cfg(test)]
mod tests
{
	use super::{ModuleRect,modulePosition_freeFind,modulePosition_nearestFreeFind};

	#[test]
	fn rectanglesTouchingAtTheEdge_doNotOverlap()
	{
		let left = ModuleRect::new([0,0],[100,100]);
		let right = ModuleRect::new([100,0],[100,100]);

		assert!(!left.intersects(right));
	}

	#[test]
	fn freePosition_prefersBelowBeforeRight()
	{
		let occupied = [ModuleRect::new([0,0],[100,100])];

		assert_eq!(
			modulePosition_freeFind(ModuleRect::new([0,0],[100,100]),&occupied),
			Some([0,112]),
		);
	}

	#[test]
	fn freePosition_neverMovesAboveOrLeftOfTheRequest()
	{
		let occupied = [
			ModuleRect::new([50,50],[100,100]),
			ModuleRect::new([50,162],[100,100]),
		];
		let resolved = modulePosition_freeFind(ModuleRect::new([50,50],[100,100]),&occupied).unwrap();

		assert!(resolved[0] >= 50);
		assert!(resolved[1] >= 50);
		assert_eq!(resolved,[50,274]);
	}

	#[test]
	fn nearestFreePosition_usesTheShortestDirectionIncludingUp()
	{
		let occupied = [ModuleRect::new([100,300],[100,100])];
		let requested = ModuleRect::new([140,285],[20,20]);

		assert_eq!(modulePosition_nearestFreeFind(requested,&occupied),Some([140,268]));
	}

	#[test]
	fn nearestFreePosition_keepsAnAlreadyFreePosition()
	{
		let occupied = [ModuleRect::new([300,300],[100,100])];
		let requested = ModuleRect::new([40,50],[100,100]);

		assert_eq!(modulePosition_nearestFreeFind(requested,&occupied),Some([40,50]));
	}
}
