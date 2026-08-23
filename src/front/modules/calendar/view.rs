use super::{CALENDAR_AI_ACTION_CREATE,CalendarLoadState,CalendarRuntime,browser_today_get};
use crate::front::utils::browser;
use super::caldav::CalDavError;
#[cfg(feature = "hydrate")]
use super::caldav::CalDavClient;
use super::domain::{
	CALENDAR_MAX_COLLECTIONS,CalendarCollection,CalendarConfig,CalendarCreateInput,CalendarCreateMoment,CalendarEditScope,CalendarEvent,
	CalendarConfigError,CalendarHolidayError,CalendarMoment,CalendarRecurrence,CalendarRecurrenceEnd,
	CalendarRecurrenceFrequency,CalendarRejectedReason,CalendarViewMode,
};
use crate::api::modules::components::ModuleID;
use crate::front::modules::components::{Cache,FieldHelper,FieldHelperType,ModuleConfigSession};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::dialog::{DialogActionStyle,DialogData,DialogManager};
use crate::front::utils::toaster_helpers::toastingErr;
#[cfg(feature = "hydrate")]
use crate::front::utils::toaster_helpers::toastingSuccess;
use crate::front::utils::translate::TranslateText;
use crate::HWebTrace;
use leptoaster::expect_toaster;
use leptos::html::Div;
use leptos::prelude::{
	ArcRwSignal,AriaAttributes,BindAttribute,ClassAttribute,CollectView,Effect,ElementChild,Get,GetUntracked,
	GlobalAttributes,IntoAny,NodeRef,NodeRefAttribute,OnAttribute,PropAttribute,RwSignal,Set,StyleAttribute,Update,
	event_target_checked,on_cleanup,use_context,
};
use leptos::{component,view,IntoView};
use std::collections::{HashMap,HashSet};
use time::{Date,Duration,Month,PrimitiveDateTime,Time,Weekday};
#[cfg(not(feature = "hydrate"))]
use time::OffsetDateTime;

#[component]
pub(super) fn CalendarDraw(
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	update: ArcRwSignal<Cache>,
	editMode: RwSignal<bool>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
) -> impl IntoView
{
	let Some(dialogManager) = use_context::<DialogManager>()
	else
	{
		HWebTrace!("cannot get dialogManager in calendar");
		return view!{}.into_any();
	};

	let effectRuntime = runtime.clone();
	let effectConfig = config.clone();
	let effectActions = moduleActions.clone();
	let effectModuleId = moduleId.clone();
	Effect::new(move |previousEditMode: Option<bool>| {
		let currentEditMode = editMode.get();
		if (!currentEditMode && previousEditMode != Some(false))
		{
			let anchor = browser_today_get();
			let viewMode = effectConfig.get_untracked().viewMode;
			let periodChanged = effectRuntime.try_update(|runtime| runtime.period_set(anchor,viewMode))
				.unwrap_or(false);
			if (periodChanged || previousEditMode == Some(true))
			{
				(effectActions.refreshFn)(effectModuleId.clone());
			}
		}
		return currentEditMode;
	});

	view! {
		<CalendarContentDraw
			config=config
			runtime=runtime
			update=update
			moduleActions=moduleActions
			moduleId=moduleId
			dialogManager=dialogManager
		/>
	}.into_any()
}

#[component]
pub(super) fn CalendarConfigDraw(
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	update: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	session: ModuleConfigSession,
) -> impl IntoView
{
	let cleanupRuntime = runtime.clone();
	on_cleanup(move || {
		let _ = cleanupRuntime.try_update(|runtime| runtime.discoveryLoading = false);
	});
	let mut titleField = FieldHelper::new(&config,&update,"MODULE_TITLE_CONF",
		|config| config.get().title,
		|event,config| config.title = event.target().value());
	titleField.setFullSize();
	let runtimeServer = runtime.clone();
	let mut serverField = FieldHelper::new(&config,&update,"MODULE_CALENDAR_SERVER_URL",
		|config| config.get().serverUrl,
		move |event,config| {
			let value = event.target().value();
			if (config.serverUrl != value)
			{
				config.serverUrl = value;
				config.collections.clear();
				runtimeServer.update(|runtime| {
					runtime.discoveredCollections.clear();
					runtime.discoveryError = None;
				});
			}
		});
	serverField.setFullSize();
	let runtimeUsername = runtime.clone();
	let usernameField = FieldHelper::new(&config,&update,"MODULE_CALENDAR_USERNAME",
		|config| config.get().username,
		move |event,config| {
			let value = event.target().value();
			if (config.username != value)
			{
				config.username = value;
				config.collections.clear();
				runtimeUsername.update(|runtime| {
					runtime.discoveredCollections.clear();
					runtime.discoveryError = None;
				});
			}
		});
	let mut passwordField = FieldHelper::new(&config,&update,"MODULE_CALENDAR_PASSWORD",
		|config| config.get().password,
		|event,config| config.password = event.target().value());
	passwordField.setInputType(FieldHelperType::PASSWORD);
	let holidayCountryField = FieldHelper::new(&config,&update,"MODULE_CALENDAR_HOLIDAY_COUNTRY",
		|config| config.get().holidayCountry,
		|event,config| {
			config.holidayCountry = event.target().value().chars()
				.filter(char::is_ascii_alphabetic)
				.take(2)
				.collect::<String>()
				.to_ascii_uppercase();
		});
	let serverInputRef = serverField.inputRef_get();
	let usernameInputRef = usernameField.inputRef_get();
	let passwordInputRef = passwordField.inputRef_get();

	let discoverConfig = config.clone();
	let discoverRuntime = runtime.clone();
	let discoverActions = moduleActions.clone();
	let discoverUpdate = update.clone();
	let discoverSession = session.clone();
	let discover = move |_| {
		if (!discoverSession.isActive()) {return;}
		if let (Some(serverInput),Some(usernameInput),Some(passwordInput)) = (
			serverInputRef.get(),usernameInputRef.get(),passwordInputRef.get(),
		)
		{
			let serverUrl = serverInput.value();
			let username = usernameInput.value();
			let password = passwordInput.value();
			let mut identityChanged = false;
			let changed = discoverConfig.try_update(|config| {
				identityChanged = config.serverUrl != serverUrl || config.username != username;
				let changed = identityChanged || config.password != password;
				config.serverUrl = serverUrl;
				config.username = username;
				config.password = password;
				if (identityChanged)
				{
					config.collections.clear();
				}
				return changed;
			}).unwrap_or(false);
			if (changed)
			{
				discoverUpdate.update(|cache| cache.update());
			}
		}
		calendarCollections_discover(
			discoverConfig.clone(),discoverRuntime.clone(),discoverUpdate.clone(),discoverActions.clone(),
			discoverSession.clone(),
		);
	};
	let selectedConfig = config.clone();
	let discoveredRuntime = runtime.clone();
	let discoveryStatusRuntime = runtime.clone();
	let discoveryButtonRuntime = runtime.clone();
	let selectionUpdate = update.clone();
	let weekendCheckedConfig = config.clone();
	let weekendChangeConfig = config.clone();
	let weekendChangeUpdate = update.clone();
	let aiActionCheckedConfig = config.clone();
	let aiActionChangeConfig = config.clone();
	let aiActionChangeUpdate = update.clone();

	view! {
		<div class="module_config module_calendar_config">
			{titleField.draw()}
			{serverField.draw()}
			<div class="module_calendar_connection_fields">
				{usernameField.draw()}
				{passwordField.draw()}
			</div>
			<p class="module_config_help"><TranslateText key="MODULE_CALENDAR_CONNECTION_HELP"/></p>
			<div class="module_config_actions">
				<button
					type="button"
					disabled=move || discoveryButtonRuntime.get().discoveryLoading
					on:click=discover
				>
					{move || if runtime.get().discoveryLoading
					{
						view!{<TranslateText key="MODULE_CALENDAR_DISCOVERING"/>}.into_any()
					}
					else
					{
						view!{<TranslateText key="MODULE_CALENDAR_DISCOVER"/>}.into_any()
					}}
				</button>
			</div>
			{move || discoveryStatusRuntime.get().discoveryError.map(|error| view! {
				<p class="module_calendar_status module_calendar_status--error" role="alert">
					<TranslateText key={calDavError_key(error)}/>
				</p>
			})}
			<fieldset class="module_calendar_collections">
				<legend><TranslateText key="MODULE_CALENDAR_COLLECTIONS"/></legend>
				{move || {
					let collections = collectionOptions_get(&selectedConfig.get(),&discoveredRuntime.get());
					if (collections.is_empty())
					{
						return view! {
							<p class="module_config_help"><TranslateText key="MODULE_CALENDAR_COLLECTIONS_EMPTY"/></p>
						}.into_any();
					}
					return view! {
						<div class="module_calendar_collection_choices">
							{collections.into_iter().map(|collection| {
								let href = collection.href.clone();
								let label = if (collection.name.is_empty())
								{
									collectionLabel_fallback(&collection.href)
								}
								else
								{
									collection.name.clone()
								};
								let checkedConfig = selectedConfig.clone();
								let changeConfig = selectedConfig.clone();
								let changeUpdate = selectionUpdate.clone();
								let changeCollection = collection.clone();
								view! {
									<label class="module_calendar_collection_choice">
										<input
											type="checkbox"
											prop:checked=move || checkedConfig.get().collections.iter().any(|selected| selected.href == href)
											on:change=move |event| {
												let checked = event_target_checked(&event);
												changeConfig.update(|config| {
													config.collections.retain(|selected| selected.href != changeCollection.href);
													if (checked && config.collections.len() < CALENDAR_MAX_COLLECTIONS)
													{
														config.collections.push(changeCollection.clone());
													}
												});
												changeUpdate.update(|cache| cache.update());
											}
										/>
										<span class="module_calendar_collection_color" style={collectionColor_style(&collection)} aria-hidden="true"></span>
										<span>{label}</span>
									</label>
								}
							}).collect_view()}
						</div>
					}.into_any();
				}}
			</fieldset>
			<fieldset class="module_calendar_display_options">
				<legend><TranslateText key="MODULE_CALENDAR_DISPLAY_OPTIONS"/></legend>
				<label class="module_calendar_checkbox_option">
					<input
						type="checkbox"
						prop:checked=move || weekendCheckedConfig.get().highlightWeekends
						on:change=move |event| {
							weekendChangeConfig.update(|config| config.highlightWeekends = event_target_checked(&event));
							weekendChangeUpdate.update(|cache| cache.update());
						}
					/>
					<span><TranslateText key="MODULE_CALENDAR_HIGHLIGHT_WEEKENDS"/></span>
				</label>
				{holidayCountryField.draw()}
				<p class="module_config_help"><TranslateText key="MODULE_CALENDAR_HOLIDAY_COUNTRY_HELP"/></p>
			</fieldset>
			<fieldset class="module_ai_permissions">
				<legend><TranslateText key="MODULE_AI_PERMISSIONS"/></legend>
				<label class="module_ai_permission">
					<input
						type="checkbox"
						prop:checked=move || aiActionCheckedConfig.get().aiGrant.action_allows(CALENDAR_AI_ACTION_CREATE)
						on:change=move |event| {
							let enabled = event_target_checked(&event);
							aiActionChangeConfig.update(|config| {
								config.aiGrant.actions.retain(|action| action != CALENDAR_AI_ACTION_CREATE);
								if (enabled)
								{
									config.aiGrant.actions.push(CALENDAR_AI_ACTION_CREATE.to_string());
								}
							});
							aiActionChangeUpdate.update(|cache| cache.update());
						}
					/>
					<span><TranslateText key="MODULE_CALENDAR_AI_CREATE_ACTION"/></span>
				</label>
				<p class="module_config_help"><TranslateText key="MODULE_CALENDAR_AI_HELP"/></p>
			</fieldset>
		</div>
	}
}

fn collectionOptions_get(config: &CalendarConfig,runtime: &CalendarRuntime) -> Vec<CalendarCollection>
{
	let mut seen = HashSet::new();
	let mut collections = runtime.discoveredCollections.iter()
		.chain(config.collections.iter())
		.filter(|collection| seen.insert(collection.href.clone()))
		.cloned()
		.collect::<Vec<_>>();
	collections.sort_by(|left,right| left.name.to_lowercase().cmp(&right.name.to_lowercase()).then_with(|| left.href.cmp(&right.href)));
	return collections;
}

fn collectionLabel_fallback(href: &str) -> String
{
	return url::Url::parse(href).ok()
		.and_then(|url| url.path_segments().and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back()).map(str::to_string))
		.unwrap_or_else(|| "—".to_string());
}

#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
struct CalendarScrollState
{
	period: Option<super::domain::CalendarPeriod>,
	top: i32,
	left: i32,
}

impl CalendarScrollState
{
	fn period_apply(&mut self,period: Option<super::domain::CalendarPeriod>)
	{
		if (self.period == period) {return;}
		self.period = period;
		self.top = 0;
		self.left = 0;
	}
}

#[cfg(feature="hydrate")]
fn calendarScroll_capture(
	gridRef: NodeRef<Div>,
	scrollState: RwSignal<CalendarScrollState>,
	period: Option<super::domain::CalendarPeriod>,
)
{
	let Some(grid) = gridRef.try_get_untracked().flatten() else {return};
	scrollState.set(CalendarScrollState {
		period,
		top: grid.scroll_top(),
		left: grid.scroll_left(),
	});
}

#[cfg(not(feature="hydrate"))]
fn calendarScroll_capture(
	_gridRef: NodeRef<Div>,
	_scrollState: RwSignal<CalendarScrollState>,
	_period: Option<super::domain::CalendarPeriod>,
)
{
}

#[cfg(feature="hydrate")]
fn calendarScroll_restore(gridRef: NodeRef<Div>,scrollState: CalendarScrollState)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(grid) = gridRef.try_get_untracked().flatten() else {return};
		grid.set_scroll_top(scrollState.top);
		grid.set_scroll_left(scrollState.left);
	});
}

#[cfg(not(feature="hydrate"))]
fn calendarScroll_restore(_gridRef: NodeRef<Div>,_scrollState: CalendarScrollState)
{
}

#[component]
fn CalendarContentDraw(
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	update: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
) -> impl IntoView
{
	let gridRef = NodeRef::<Div>::new();
	let scrollState = RwSignal::new(CalendarScrollState::default());
	let scrollRuntime = runtime.clone();
	Effect::new(move |_| {
		let period = scrollRuntime.get().period;
		let mut state = scrollState.get_untracked();
		state.period_apply(period);
		scrollState.set(state);
		calendarScroll_restore(gridRef,state);
	});

	let previousRuntime = runtime.clone();
	let previousConfig = config.clone();
	let previousActions = moduleActions.clone();
	let previousId = moduleId.clone();
	let previous = move |_| calendarPeriod_shift(
		-1,previousConfig.clone(),previousRuntime.clone(),previousActions.clone(),previousId.clone(),
	);
	let nextRuntime = runtime.clone();
	let nextConfig = config.clone();
	let nextActions = moduleActions.clone();
	let nextId = moduleId.clone();
	let next = move |_| calendarPeriod_shift(
		1,nextConfig.clone(),nextRuntime.clone(),nextActions.clone(),nextId.clone(),
	);
	let todayRuntime = runtime.clone();
	let todayConfig = config.clone();
	let todayActions = moduleActions.clone();
	let todayId = moduleId.clone();
	let today = move |_| calendarPeriod_set(
		browser_today_get(),todayConfig.clone(),todayRuntime.clone(),todayActions.clone(),todayId.clone(),
	);
	let monthConfig = config.clone();
	let monthRuntime = runtime.clone();
	let monthUpdate = update.clone();
	let monthActions = moduleActions.clone();
	let monthId = moduleId.clone();
	let month = move |_| calendarViewMode_set(
		CalendarViewMode::Month,monthConfig.clone(),monthRuntime.clone(),monthUpdate.clone(),monthActions.clone(),monthId.clone(),
	);
	let weekConfig = config.clone();
	let weekRuntime = runtime.clone();
	let weekUpdate = update.clone();
	let weekActions = moduleActions.clone();
	let weekId = moduleId.clone();
	let week = move |_| calendarViewMode_set(
		CalendarViewMode::Week,weekConfig.clone(),weekRuntime.clone(),weekUpdate.clone(),weekActions.clone(),weekId.clone(),
	);
	let refreshActions = moduleActions.clone();
	let refreshId = moduleId.clone();
	let refresh = move |_| (refreshActions.refreshFn)(refreshId.clone());
	let createConfig = config.clone();
	let createRuntime = runtime.clone();
	let createActions = moduleActions.clone();
	let createId = moduleId.clone();
	let createDialog = dialogManager.clone();
	let create = move |_| calendarCreate_open(
		createConfig.clone(),createRuntime.clone(),createActions.clone(),createId.clone(),createDialog.clone(),
	);
	let gridRuntime = runtime.clone();
	let gridConfig = config.clone();
	let gridActions = moduleActions.clone();
	let gridId = moduleId.clone();
	let gridDialog = dialogManager.clone();
	let titleConfig = config.clone();
	let monthClassConfig = config.clone();
	let monthPressedConfig = config.clone();
	let weekClassConfig = config.clone();
	let weekPressedConfig = config.clone();
	let labelRuntime = runtime.clone();
	let statusRuntime = runtime.clone();
	let gridScrollRuntime = runtime.clone();
	let viewSwitchLabelId = format!("calendar-view-switch-{}",moduleId.id);
	let viewSwitchLabelledBy = viewSwitchLabelId.clone();

	view! {
		<div class="module_calendar">
			<div class="module_titlebar module_calendar_titlebar">
				<h2 class="module_title">{move || {
					let title = titleConfig.get().title;
					if (title.is_empty())
					{
						view!{<TranslateText key="MODULE_TYPE_CALENDAR"/>}.into_any()
					}
					else
					{
						view!{<span>{title}</span>}.into_any()
					}
				}}</h2>
				<div class="module_title_actions module_calendar_primary_actions">
					<button type="button" class="module_title_action" on:click=create>
						<i class="iconoir-plus" aria-hidden="true"></i>
						<span class="visually_hidden"><TranslateText key="MODULE_CALENDAR_CREATE"/></span>
					</button>
					<button type="button" class="module_title_action" on:click=refresh>
						<i class="iconoir-refresh" aria-hidden="true"></i>
						<span class="visually_hidden"><TranslateText key="MODULE_CALENDAR_REFRESH"/></span>
					</button>
				</div>
			</div>
			<div class="module_calendar_toolbar">
				<div class="module_calendar_navigation">
					<button type="button" on:click=previous>
						<i class="iconoir-nav-arrow-left" aria-hidden="true"></i>
						<span class="visually_hidden"><TranslateText key="MODULE_CALENDAR_PREVIOUS"/></span>
					</button>
					<button type="button" on:click=today><TranslateText key="MODULE_CALENDAR_TODAY"/></button>
					<button type="button" on:click=next>
						<i class="iconoir-nav-arrow-right" aria-hidden="true"></i>
						<span class="visually_hidden"><TranslateText key="MODULE_CALENDAR_NEXT"/></span>
					</button>
				</div>
				<div class="module_calendar_period_label" aria-live="polite">{move || calendarPeriodLabel_view(&labelRuntime.get())}</div>
				<div class="module_calendar_view_switch" role="group" aria-labelledby={viewSwitchLabelledBy}>
					<span id={viewSwitchLabelId} class="visually_hidden"><TranslateText key="MODULE_CALENDAR_VIEW"/></span>
					<button
						type="button"
						class:active=move || monthClassConfig.get().viewMode == CalendarViewMode::Month
						aria-pressed=move || monthPressedConfig.get().viewMode == CalendarViewMode::Month
						on:click=month
					><TranslateText key="MODULE_CALENDAR_MONTH"/></button>
					<button
						type="button"
						class:active=move || weekClassConfig.get().viewMode == CalendarViewMode::Week
						aria-pressed=move || weekPressedConfig.get().viewMode == CalendarViewMode::Week
						on:click=week
					><TranslateText key="MODULE_CALENDAR_WEEK"/></button>
				</div>
			</div>
			{move || calendarStatus_view(&statusRuntime.get())}
			<div
				class="module_calendar_grid_container"
				node_ref=gridRef
				on:scroll=move |_| calendarScroll_capture(
					gridRef,scrollState,gridScrollRuntime.get_untracked().period,
				)
			>
				{move || calendarGrid_view(
					gridConfig.clone(),&gridRuntime.get(),gridActions.clone(),gridId.clone(),gridDialog.clone(),
				)}
			</div>
		</div>
	}
}

fn calendarPeriodLabel_view(runtime: &CalendarRuntime) -> leptos::prelude::AnyView
{
	let anchor = runtime.anchor.unwrap_or_else(browser_today_get);
	view! {
		<span><TranslateText key={month_key(anchor.month())}/>{" "}{anchor.year()}</span>
	}.into_any()
}

fn calendarStatus_view(runtime: &CalendarRuntime) -> leptos::prelude::AnyView
{
	let mut notices = Vec::new();
	match runtime.loadState
	{
		CalendarLoadState::Idle => {},
		CalendarLoadState::Loading => notices.push(view! {
			<p class="module_calendar_status" role="status"><TranslateText key="MODULE_CALENDAR_LOADING"/></p>
		}.into_any()),
		CalendarLoadState::ConfigurationRequired => notices.push(view! {
			<p class="module_calendar_status module_calendar_status--warning"><TranslateText key="MODULE_CALENDAR_CONFIGURATION_REQUIRED"/></p>
		}.into_any()),
		CalendarLoadState::Error(error) => notices.push(view! {
			<p class="module_calendar_status module_calendar_status--error" role="alert"><TranslateText key={calDavError_key(error)}/></p>
		}.into_any()),
		CalendarLoadState::Ready if runtime.events.is_empty() => notices.push(view! {
			<p class="module_calendar_status"><TranslateText key="MODULE_CALENDAR_EMPTY"/></p>
		}.into_any()),
		CalendarLoadState::Ready => {},
	}
	if (runtime.stale)
	{
		notices.push(view! {
			<p class="module_calendar_status module_calendar_status--warning"><TranslateText key="MODULE_CALENDAR_STALE"/></p>
		}.into_any());
	}
	if (runtime.partialFailures > 0)
	{
		notices.push(view! {
			<p class="module_calendar_status module_calendar_status--warning"><TranslateText key="MODULE_CALENDAR_PARTIAL"/></p>
		}.into_any());
	}
	if (runtime.rejectedEvents > 0)
	{
		let parameters = HashMap::from([
			("count".to_string(),runtime.rejectedEvents.to_string()),
		]);
		let omitted = runtime.rejectedEvents.saturating_sub(runtime.rejectedSamples.len());
		let samples = runtime.rejectedSamples.clone();
		notices.push(view! {
			<details class="module_calendar_status module_calendar_status--warning module_calendar_rejected">
				<summary><TranslateText key="MODULE_CALENDAR_REJECTED_EVENTS" params=parameters/></summary>
				<ul>
					{samples.into_iter().map(|sample| view! {
						<li>
							<span class="module_calendar_rejected_source">{if (sample.collectionName.is_empty()) {"—".to_string()} else {sample.collectionName}}</span>
							<span class="module_calendar_rejected_title">{if (sample.title.is_empty())
							{
								view!{<TranslateText key="FRONTUI_NOTITLE"/>}.into_any()
							}
							else
							{
								view!{<span>{sample.title}</span>}.into_any()
							}}</span>
							<span><TranslateText key={calendarRejectedReason_key(sample.reason)}/></span>
						</li>
					}).collect_view()}
				</ul>
				{(omitted > 0).then(|| view! {
					<p><TranslateText key="MODULE_CALENDAR_REJECTED_OMITTED" params=HashMap::from([
						("count".to_string(),omitted.to_string()),
					])/></p>
				})}
			</details>
		}.into_any());
	}
	if (runtime.holidayLoading)
	{
		notices.push(view! {
			<p class="module_calendar_status" role="status"><TranslateText key="MODULE_CALENDAR_HOLIDAY_LOADING"/></p>
		}.into_any());
	}
	if let Some(error) = runtime.holidayError
	{
		notices.push(view! {
			<p class="module_calendar_status module_calendar_status--warning" role="alert">
				<TranslateText key={calendarHolidayError_key(error)}/>
			</p>
		}.into_any());
	}
	return view!{<div class="module_calendar_statuses">{notices}</div>}.into_any();
}

fn calendarGrid_view(
	configSignal: ArcRwSignal<CalendarConfig>,
	runtime: &CalendarRuntime,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
) -> leptos::prelude::AnyView
{
	let config = configSignal.get_untracked();
	let Some(period) = runtime.period else {return view!{}.into_any();};
	let anchor = runtime.anchor.unwrap_or_else(browser_today_get);
	let weekdays = [
		"MODULE_CALENDAR_WEEKDAY_MON","MODULE_CALENDAR_WEEKDAY_TUE","MODULE_CALENDAR_WEEKDAY_WED",
		"MODULE_CALENDAR_WEEKDAY_THU","MODULE_CALENDAR_WEEKDAY_FRI","MODULE_CALENDAR_WEEKDAY_SAT",
		"MODULE_CALENDAR_WEEKDAY_SUN",
	];
	let viewClass = if (config.viewMode == CalendarViewMode::Week) {" week"} else {" month"};
	let days = period.days().map(|date| {
		let events = runtime.events.iter()
			.filter(|event| event_overlapsLocalDate(event,date))
			.cloned()
			.collect::<Vec<_>>();
		let outside = config.viewMode == CalendarViewMode::Month && date.month() != anchor.month();
		let today = date == browser_today_get();
		let weekend = config.highlightWeekends && matches!(date.weekday(),Weekday::Saturday | Weekday::Sunday);
		let holidayNames = runtime.holidays.get(&date).cloned().unwrap_or_default();
		let holiday = !holidayNames.is_empty();
		let holidayTitle = holidayNames.join(", ");
		let actions = moduleActions.clone();
		let id = moduleId.clone();
		let dialog = dialogManager.clone();
		let eventConfig = configSignal.clone();
		view! {
			<div class="module_calendar_day" class:outside=outside class:today=today class:weekend=weekend class:holiday=holiday>
				<div class="module_calendar_day_number">
					{holiday.then(|| view! {
						<span class="module_calendar_holiday_name" title={holidayTitle}>{holidayNames.join(", ")}</span>
					})}
					<span>{if (config.viewMode == CalendarViewMode::Week)
						{
							format!("{:02}/{:02}",date.day(),u8::from(date.month()))
						}
						else
						{
							date.day().to_string()
						}}</span>
				</div>
				<div class="module_calendar_day_events">
					{events.into_iter().map(|event| {
						let eventView = event.clone();
						let actions = actions.clone();
						let id = id.clone();
						let dialog = dialog.clone();
						let config = eventConfig.clone();
						let time = eventTime_get(&event,date);
						let style = eventColor_style(&event);
						let title = event.title.clone();
						view! {
							<button
								type="button"
								class="module_calendar_event"
								style={style}
								on:click=move |_| calendarEventDetails_open(eventView.clone(),config.clone(),actions.clone(),id.clone(),dialog.clone())
							>
								<span class="module_calendar_event_time">{
									if let Some(time) = time
									{
										view!{<span>{time}</span>}.into_any()
									}
									else
									{
										view!{<TranslateText key="MODULE_CALENDAR_ALL_DAY_SHORT"/>}.into_any()
									}
								}</span>
								<span class="module_calendar_event_title">{if (title.is_empty())
								{
									view!{<TranslateText key="FRONTUI_NOTITLE"/>}.into_any()
								}
								else
								{
									view!{<span>{title}</span>}.into_any()
								}}</span>
							</button>
						}
					}).collect_view()}
				</div>
			</div>
		}
	}).collect_view();
	return view! {
		<div class={format!("module_calendar_grid{viewClass}")}>
			{weekdays.into_iter().map(|key| view! {
				<div class="module_calendar_weekday"><TranslateText key={key}/></div>
			}).collect_view()}
			{days}
		</div>
	}.into_any();
}

fn calendarPeriod_shift(
	direction: i8,
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
)
{
	let viewMode = config.get_untracked().viewMode;
	let anchor = runtime.get_untracked().anchor.unwrap_or_else(browser_today_get);
	let anchor = match viewMode
	{
		CalendarViewMode::Week => anchor + Duration::days(direction as i64 * 7),
		CalendarViewMode::Month => month_shift(anchor,direction),
	};
	calendarPeriod_set(anchor,config,runtime,moduleActions,moduleId);
}

fn calendarPeriod_set(
	anchor: Date,
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
)
{
	let viewMode = config.get_untracked().viewMode;
	runtime.update(|runtime| {runtime.period_set(anchor,viewMode);});
	(moduleActions.refreshFn)(moduleId);
}

fn calendarViewMode_set(
	viewMode: CalendarViewMode,
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	update: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
)
{
	if (config.get_untracked().viewMode == viewMode)
	{
		return;
	}
	config.update(|config| config.viewMode = viewMode);
	update.update(|cache| cache.update());
	let anchor = runtime.get_untracked().anchor.unwrap_or_else(browser_today_get);
	runtime.update(|runtime| {runtime.period_set(anchor,viewMode);});
	(moduleActions.updateFn)(moduleId.clone());
	(moduleActions.refreshFn)(moduleId);
}

fn month_shift(anchor: Date,direction: i8) -> Date
{
	let (year,month) = if (direction < 0)
	{
		if (anchor.month() == Month::January) {(anchor.year() - 1,Month::December)}
		else {(anchor.year(),anchor.month().previous())}
	}
	else if (anchor.month() == Month::December)
	{
		(anchor.year() + 1,Month::January)
	}
	else
	{
		(anchor.year(),anchor.month().next())
	};
	return Date::from_calendar_date(year,month,1).expect("the first day of a month is valid");
}

fn month_key(month: Month) -> &'static str
{
	return match month
	{
		Month::January => "MODULE_CALENDAR_MONTH_JANUARY",
		Month::February => "MODULE_CALENDAR_MONTH_FEBRUARY",
		Month::March => "MODULE_CALENDAR_MONTH_MARCH",
		Month::April => "MODULE_CALENDAR_MONTH_APRIL",
		Month::May => "MODULE_CALENDAR_MONTH_MAY",
		Month::June => "MODULE_CALENDAR_MONTH_JUNE",
		Month::July => "MODULE_CALENDAR_MONTH_JULY",
		Month::August => "MODULE_CALENDAR_MONTH_AUGUST",
		Month::September => "MODULE_CALENDAR_MONTH_SEPTEMBER",
		Month::October => "MODULE_CALENDAR_MONTH_OCTOBER",
		Month::November => "MODULE_CALENDAR_MONTH_NOVEMBER",
		Month::December => "MODULE_CALENDAR_MONTH_DECEMBER",
	};
}

fn collectionColor_style(collection: &CalendarCollection) -> String
{
	return collection.color_get()
		.map(|color| format!("--calendar-collection-color: {color}"))
		.unwrap_or_default();
}

fn eventColor_style(event: &CalendarEvent) -> String
{
	return CalendarCollection::color_normalize(event.collectionColor.clone())
		.map(|color| format!("--calendar-event-color: {color}"))
		.unwrap_or_default();
}

fn calDavError_key(error: CalDavError) -> &'static str
{
	return match error
	{
		CalDavError::InvalidConfiguration => "MODULE_CALENDAR_ERROR_CONFIGURATION",
		CalDavError::Configuration(CalendarConfigError::MissingServer) => "MODULE_CALENDAR_ERROR_SERVER_REQUIRED",
		CalDavError::Configuration(CalendarConfigError::MissingUsername) => "MODULE_CALENDAR_ERROR_USERNAME_REQUIRED",
		CalDavError::Configuration(CalendarConfigError::MissingPassword) => "MODULE_CALENDAR_ERROR_PASSWORD_REQUIRED",
		CalDavError::Configuration(_) => "MODULE_CALENDAR_ERROR_SERVER_INVALID",
		CalDavError::InsecureTransport => "MODULE_CALENDAR_ERROR_HTTP_CONTEXT",
		CalDavError::InvalidBasicUsername => "MODULE_CALENDAR_ERROR_USERNAME_INVALID",
		CalDavError::Transport => "MODULE_CALENDAR_ERROR_NETWORK",
		CalDavError::Unauthorized => "MODULE_CALENDAR_ERROR_AUTHENTICATION",
		CalDavError::Forbidden => "MODULE_CALENDAR_ERROR_FORBIDDEN",
		CalDavError::NotFound => "MODULE_CALENDAR_ERROR_NOT_FOUND",
		CalDavError::Conflict => "MODULE_CALENDAR_ERROR_CONFLICT",
		CalDavError::ResponseTooLarge | CalDavError::TooManyItems => "MODULE_CALENDAR_ERROR_LIMIT",
		CalDavError::InvalidResponse | CalDavError::InvalidCalendar => "MODULE_CALENDAR_ERROR_RESPONSE",
		CalDavError::MissingEtag => "MODULE_CALENDAR_ERROR_ETAG",
		CalDavError::MoveIncomplete => "MODULE_CALENDAR_ERROR_MOVE_INCOMPLETE",
	};
}

fn calendarRejectedReason_key(reason: CalendarRejectedReason) -> &'static str
{
	return match reason
	{
		CalendarRejectedReason::MissingUid => "MODULE_CALENDAR_REJECTED_MISSING_UID",
		CalendarRejectedReason::MissingStart => "MODULE_CALENDAR_REJECTED_MISSING_START",
		CalendarRejectedReason::UnsupportedDateTime => "MODULE_CALENDAR_REJECTED_DATE_TIME",
		CalendarRejectedReason::InvalidEnd => "MODULE_CALENDAR_REJECTED_END",
		CalendarRejectedReason::FieldTooLarge => "MODULE_CALENDAR_REJECTED_TOO_LARGE",
		CalendarRejectedReason::UnsupportedEvent => "MODULE_CALENDAR_REJECTED_UNSUPPORTED",
	};
}

fn calendarHolidayError_key(error: CalendarHolidayError) -> &'static str
{
	return match error
	{
		CalendarHolidayError::InvalidCountry => "MODULE_CALENDAR_HOLIDAY_ERROR_COUNTRY",
		CalendarHolidayError::Transport | CalendarHolidayError::Unavailable => "MODULE_CALENDAR_HOLIDAY_ERROR_NETWORK",
		CalendarHolidayError::ResponseTooLarge | CalendarHolidayError::TooManyItems => "MODULE_CALENDAR_HOLIDAY_ERROR_LIMIT",
		CalendarHolidayError::InvalidResponse => "MODULE_CALENDAR_HOLIDAY_ERROR_RESPONSE",
	};
}

#[cfg(feature = "hydrate")]
fn timestampLocalDate_get(timestamp: i64) -> Option<Date>
{
	let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp as f64 * 1_000.0));
	let month = u8::try_from(date.get_month() + 1).ok()?.try_into().ok()?;
	return Date::from_calendar_date(date.get_full_year() as i32,month,date.get_date() as u8).ok();
}

#[cfg(not(feature = "hydrate"))]
fn timestampLocalDate_get(timestamp: i64) -> Option<Date>
{
	OffsetDateTime::from_unix_timestamp(timestamp).ok().map(|dateTime| dateTime.date())
}

fn event_overlapsLocalDate(event: &CalendarEvent,date: Date) -> bool
{
	return match (&event.start,&event.end)
	{
		(CalendarMoment::AllDay(start),CalendarMoment::AllDay(end)) => date >= *start && date < *end,
		(CalendarMoment::Timed(start),CalendarMoment::Timed(end)) =>
		{
			let startDate = timestampLocalDate_get(*start);
			let finalTimestamp = if (end > start) {end.saturating_sub(1)} else {*start};
			let endDate = timestampLocalDate_get(finalTimestamp);
			matches!((startDate,endDate),(Some(start),Some(end)) if date >= start && date <= end)
		},
		_ => false,
	};
}

#[cfg(feature = "hydrate")]
fn eventTime_get(event: &CalendarEvent,date: Date) -> Option<String>
{
	let CalendarMoment::Timed(timestamp) = event.start else {return None;};
	if (timestampLocalDate_get(timestamp) != Some(date))
	{
		return Some("↳".to_string());
	}
	let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp as f64 * 1_000.0));
	return Some(format!("{:02}:{:02}",date.get_hours(),date.get_minutes()));
}

#[cfg(not(feature = "hydrate"))]
fn eventTime_get(event: &CalendarEvent,date: Date) -> Option<String>
{
	let CalendarMoment::Timed(timestamp) = event.start else {return None;};
	let dateTime = OffsetDateTime::from_unix_timestamp(timestamp).ok()?;
	if (dateTime.date() != date) {return Some("↳".to_string());}
	return Some(format!("{:02}:{:02}",dateTime.hour(),dateTime.minute()));
}

#[cfg(feature = "hydrate")]
fn calendarCollections_discover(
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	update: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	session: ModuleConfigSession,
)
{
	if (!session.isActive() || runtime.get_untracked().discoveryLoading) {return;}
	runtime.update(|runtime| {
		runtime.discoveryLoading = true;
		runtime.discoveryError = None;
		runtime.discoveredCollections.clear();
	});
	let configSignal = config.clone();
	let configSnapshot = config.get_untracked();
	let runtimeResult = runtime.clone();
	let toaster = expect_toaster();
	let taskActions = moduleActions.clone();
	let taskSession = session.clone();
	moduleActions.task_spawn(async move {
		let result = match CalDavClient::new(&configSnapshot)
		{
			Ok(client) => client.collections_discover().await,
			Err(error) => Err(error),
		};
		if (!taskActions.lifecycle_isActive() || !taskSession.isActive()) {return;}
		if let Ok(collections) = &result
		{
			let metadataChanged = configSignal.try_update(|config| {
				let mut changed = false;
				for selected in &mut config.collections
				{
					let Some(discovered) = collections.iter().find(|collection| collection.href == selected.href)
					else {continue;};
					if (selected.name != discovered.name || selected.color != discovered.color)
					{
						selected.name = discovered.name.clone();
						selected.color = discovered.color.clone();
						changed = true;
					}
				}
				return changed;
			}).unwrap_or(false);
			if (metadataChanged)
			{
				update.update(|cache| cache.update());
			}
		}
		runtimeResult.update(|runtime| {
			runtime.discoveryLoading = false;
			match &result
			{
				Ok(collections) => runtime.discoveredCollections = collections.clone(),
				Err(error) => runtime.discoveryError = Some(*error),
			}
		});
		match result
		{
			Ok(_) => toastingSuccess(&toaster,"MODULE_CALENDAR_DISCOVERY_SUCCESS").await,
			Err(error) => toastingErr(&toaster,calDavError_key(error)).await,
		}
	});
}

#[cfg(not(feature = "hydrate"))]
fn calendarCollections_discover(
	_config: ArcRwSignal<CalendarConfig>,
	_runtime: ArcRwSignal<CalendarRuntime>,
	_update: ArcRwSignal<Cache>,
	_moduleActions: ModuleActionFn,
	_session: ModuleConfigSession,
)
{
}

fn calendarCreate_open(
	config: ArcRwSignal<CalendarConfig>,
	_runtime: ArcRwSignal<CalendarRuntime>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let configSnapshot = config.get_untracked();
	if (configSnapshot.collections.is_empty())
	{
		dialogManager.open(
			DialogData::new()
				.setTitle("MODULE_CALENDAR_CREATE")
				.setBody(|| view! {
					<p class="module_calendar_dialog_notice"><TranslateText key="MODULE_CALENDAR_CONFIGURATION_REQUIRED"/></p>
				}.into_any())
				.setButtonValidateTitle(None::<String>),
		);
		return;
	}

	let today = browser_today_get();
	let title = RwSignal::new(String::new());
	let description = RwSignal::new(String::new());
	let location = RwSignal::new(String::new());
	let allDay = RwSignal::new(false);
	let allDayStart = RwSignal::new(dateInput_format(today));
	let allDayEnd = RwSignal::new(dateInput_format(today));
	let timedStart = RwSignal::new(format!("{}T09:00",dateInput_format(today)));
	let timedEnd = RwSignal::new(format!("{}T10:00",dateInput_format(today)));
	let collectionHref = RwSignal::new(configSnapshot.collections[0].href.clone());
	let recurrenceEnabled = RwSignal::new(false);
	let recurrenceFrequency = RwSignal::new("WEEKLY".to_string());
	let recurrenceInterval = RwSignal::new("1".to_string());
	let recurrenceEnd = RwSignal::new("NEVER".to_string());
	let recurrenceUntil = RwSignal::new(dateInput_format(today));
	let recurrenceCount = RwSignal::new("5".to_string());
	let pending = RwSignal::new(false);
	let collections = configSnapshot.collections.clone();

	let bodyCollections = collections.clone();
	let dialog = DialogData::new()
		.setTitle("MODULE_CALENDAR_CREATE")
		.setIsLarger(true)
		.setBody(move || calendarCreateForm_view(
			title,description,location,allDay,allDayStart,allDayEnd,timedStart,timedEnd,
			collectionHref,bodyCollections.clone(),recurrenceEnabled,recurrenceFrequency,
			recurrenceInterval,recurrenceEnd,recurrenceUntil,recurrenceCount,pending,
		))
		.setButtonValidateTitle(Some("MODULE_CALENDAR_CREATE_ACTION"))
		.setOnValidate({
			let config = config.clone();
			let moduleActions = moduleActions.clone();
			let moduleId = moduleId.clone();
			let dialogManager = dialogManager.clone();
			move |_| {
				if (pending.get_untracked()) {return false;}
				let Some(input) = calendarCreateInput_get(
					title.get_untracked(),description.get_untracked(),location.get_untracked(),
					allDay.get_untracked(),allDayStart.get_untracked(),allDayEnd.get_untracked(),
					timedStart.get_untracked(),timedEnd.get_untracked(),
					recurrenceEnabled.get_untracked(),recurrenceFrequency.get_untracked(),
					recurrenceInterval.get_untracked(),recurrenceEnd.get_untracked(),
					recurrenceUntil.get_untracked(),recurrenceCount.get_untracked(),
				)
				else
				{
					let toaster = expect_toaster();
					moduleActions.task_spawn(async move {
						toastingErr(&toaster,"MODULE_CALENDAR_CREATE_INVALID").await;
					});
					return false;
				};
				pending.set(true);
				calendarEvent_create(
					config.clone(),collectionHref.get_untracked(),input,pending,moduleActions.clone(),
					moduleId.clone(),dialogManager.clone(),
				);
				return false;
			}
		})
		.setCanClose(move || !pending.get());
	dialogManager.open(dialog);
}

fn calendarCreateForm_view(
	title: RwSignal<String>,
	description: RwSignal<String>,
	location: RwSignal<String>,
	allDay: RwSignal<bool>,
	allDayStart: RwSignal<String>,
	allDayEnd: RwSignal<String>,
	timedStart: RwSignal<String>,
	timedEnd: RwSignal<String>,
	collectionHref: RwSignal<String>,
	collections: Vec<CalendarCollection>,
	recurrenceEnabled: RwSignal<bool>,
	recurrenceFrequency: RwSignal<String>,
	recurrenceInterval: RwSignal<String>,
	recurrenceEnd: RwSignal<String>,
	recurrenceUntil: RwSignal<String>,
	recurrenceCount: RwSignal<String>,
	pending: RwSignal<bool>,
) -> leptos::prelude::AnyView
{
	view! {
		<div class="module_calendar_create_form">
			{calendarCreateIdentity_view(title,collectionHref,collections)}
			{calendarCreatePeriod_view(allDay,allDayStart,allDayEnd,timedStart,timedEnd)}
			{calendarCreateDetails_view(location,description)}
			{calendarCreateRecurrence_view(
				recurrenceEnabled,recurrenceFrequency,recurrenceInterval,recurrenceEnd,
				recurrenceUntil,recurrenceCount,
			)}
			{move || pending.get().then(|| view! {
				<p class="module_calendar_dialog_notice" role="status"><TranslateText key="MODULE_CALENDAR_SAVING"/></p>
			})}
		</div>
	}.into_any()
}

fn calendarCreateIdentity_view(
	title: RwSignal<String>,
	collectionHref: RwSignal<String>,
	collections: Vec<CalendarCollection>,
) -> leptos::prelude::AnyView
{
	let collectionView = if (collections.len() > 1)
	{
		view! {
			<label class="module_calendar_form_field" for="calendar-create-collection">
				<span><TranslateText key="MODULE_CALENDAR_COLLECTION"/></span>
				<select id="calendar-create-collection" bind:value=collectionHref>
					{collections.iter().map(|collection| {
						let label = if (collection.name.is_empty()) {collectionLabel_fallback(&collection.href)} else {collection.name.clone()};
						view!{<option value={collection.href.clone()}>{label}</option>}
					}).collect_view()}
				</select>
			</label>
		}.into_any()
	}
	else
	{
		let collection = collections.first().cloned();
		view! {
			<div class="module_calendar_form_field">
				<span><TranslateText key="MODULE_CALENDAR_COLLECTION"/></span>
				<strong>{collection.map(|collection| if (collection.name.is_empty()) {collectionLabel_fallback(&collection.href)} else {collection.name}).unwrap_or_default()}</strong>
			</div>
		}.into_any()
	};
	return view! {
		<label class="module_calendar_form_field" for="calendar-create-title">
			<span><TranslateText key="MODULE_CALENDAR_EVENT_TITLE"/></span>
			<input id="calendar-create-title" type="text" maxlength="4096" required bind:value=title/>
		</label>
		{collectionView}
	}.into_any();
}

fn calendarCreatePeriod_view(
	allDay: RwSignal<bool>,
	allDayStart: RwSignal<String>,
	allDayEnd: RwSignal<String>,
	timedStart: RwSignal<String>,
	timedEnd: RwSignal<String>,
) -> leptos::prelude::AnyView
{
	view! {
		<label class="module_calendar_checkbox">
			<input type="checkbox" prop:checked=move || allDay.get() on:change=move |event| allDay.set(event_target_checked(&event))/>
			<span><TranslateText key="MODULE_CALENDAR_ALL_DAY"/></span>
		</label>
		{move || if (allDay.get())
		{
			view! {
				<div class="module_calendar_form_row">
					<label class="module_calendar_form_field" for="calendar-create-start-date">
						<span><TranslateText key="MODULE_CALENDAR_START_DATE"/></span>
						<input id="calendar-create-start-date" type="date" required bind:value=allDayStart/>
					</label>
					<label class="module_calendar_form_field" for="calendar-create-end-date">
						<span><TranslateText key="MODULE_CALENDAR_END_DATE_INCLUSIVE"/></span>
						<input id="calendar-create-end-date" type="date" required bind:value=allDayEnd/>
					</label>
				</div>
			}.into_any()
		}
		else
		{
			view! {
				<div class="module_calendar_form_row">
					<label class="module_calendar_form_field" for="calendar-create-start-time">
						<span><TranslateText key="MODULE_CALENDAR_START"/></span>
						<input id="calendar-create-start-time" type="datetime-local" required bind:value=timedStart/>
					</label>
					<label class="module_calendar_form_field" for="calendar-create-end-time">
						<span><TranslateText key="MODULE_CALENDAR_END"/></span>
						<input id="calendar-create-end-time" type="datetime-local" required bind:value=timedEnd/>
					</label>
				</div>
			}.into_any()
		}}
	}.into_any()
}

fn calendarCreateDetails_view(
	location: RwSignal<String>,
	description: RwSignal<String>,
) -> leptos::prelude::AnyView
{
	view! {
		<label class="module_calendar_form_field" for="calendar-create-location">
			<span><TranslateText key="MODULE_CALENDAR_LOCATION"/></span>
			<input id="calendar-create-location" type="text" maxlength="8192" bind:value=location/>
		</label>
		<label class="module_calendar_form_field" for="calendar-create-description">
			<span><TranslateText key="MODULE_CALENDAR_DESCRIPTION"/></span>
			<textarea id="calendar-create-description" maxlength="131072" rows="4" bind:value=description></textarea>
		</label>
	}.into_any()
}

fn calendarCreateRecurrence_view(
	recurrenceEnabled: RwSignal<bool>,
	recurrenceFrequency: RwSignal<String>,
	recurrenceInterval: RwSignal<String>,
	recurrenceEnd: RwSignal<String>,
	recurrenceUntil: RwSignal<String>,
	recurrenceCount: RwSignal<String>,
) -> leptos::prelude::AnyView
{
	view! {
		<fieldset class="module_calendar_recurrence">
			<legend><TranslateText key="MODULE_CALENDAR_RECURRENCE"/></legend>
			<label class="module_calendar_checkbox">
				<input type="checkbox" prop:checked=move || recurrenceEnabled.get() on:change=move |event| recurrenceEnabled.set(event_target_checked(&event))/>
				<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_ENABLE"/></span>
			</label>
			{move || recurrenceEnabled.get().then(|| calendarCreateRecurrenceSettings_view(
				recurrenceFrequency,recurrenceInterval,recurrenceEnd,recurrenceUntil,recurrenceCount,
			))}
		</fieldset>
	}.into_any()
}

fn calendarCreateRecurrenceSettings_view(
	recurrenceFrequency: RwSignal<String>,
	recurrenceInterval: RwSignal<String>,
	recurrenceEnd: RwSignal<String>,
	recurrenceUntil: RwSignal<String>,
	recurrenceCount: RwSignal<String>,
) -> leptos::prelude::AnyView
{
	view! {
		<div class="module_calendar_form_row">
			<label class="module_calendar_form_field" for="calendar-create-frequency">
				<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_FREQUENCY"/></span>
				<select id="calendar-create-frequency" bind:value=recurrenceFrequency>
					<option value="DAILY"><TranslateText key="MODULE_CALENDAR_RECURRENCE_DAILY"/></option>
					<option value="WEEKLY"><TranslateText key="MODULE_CALENDAR_RECURRENCE_WEEKLY"/></option>
					<option value="MONTHLY"><TranslateText key="MODULE_CALENDAR_RECURRENCE_MONTHLY"/></option>
					<option value="YEARLY"><TranslateText key="MODULE_CALENDAR_RECURRENCE_YEARLY"/></option>
				</select>
			</label>
			<label class="module_calendar_form_field" for="calendar-create-interval">
				<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_INTERVAL"/></span>
				<input id="calendar-create-interval" type="number" min="1" max="365" required bind:value=recurrenceInterval/>
			</label>
		</div>
		<label class="module_calendar_form_field" for="calendar-create-recurrence-end">
			<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_END"/></span>
			<select id="calendar-create-recurrence-end" bind:value=recurrenceEnd>
				<option value="NEVER"><TranslateText key="MODULE_CALENDAR_RECURRENCE_NEVER"/></option>
				<option value="UNTIL"><TranslateText key="MODULE_CALENDAR_RECURRENCE_UNTIL"/></option>
				<option value="COUNT"><TranslateText key="MODULE_CALENDAR_RECURRENCE_COUNT"/></option>
			</select>
		</label>
		{move || calendarCreateRecurrenceEnd_view(recurrenceEnd.get(),recurrenceUntil,recurrenceCount)}
	}.into_any()
}

fn calendarCreateRecurrenceEnd_view(
	recurrenceEnd: String,
	recurrenceUntil: RwSignal<String>,
	recurrenceCount: RwSignal<String>,
) -> leptos::prelude::AnyView
{
	return match recurrenceEnd.as_str()
	{
		"UNTIL" => view! {
			<label class="module_calendar_form_field" for="calendar-create-until">
				<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_UNTIL_DATE"/></span>
				<input id="calendar-create-until" type="date" required bind:value=recurrenceUntil/>
			</label>
		}.into_any(),
		"COUNT" => view! {
			<label class="module_calendar_form_field" for="calendar-create-count">
				<span><TranslateText key="MODULE_CALENDAR_RECURRENCE_OCCURRENCES"/></span>
				<input id="calendar-create-count" type="number" min="1" max="1000" required bind:value=recurrenceCount/>
			</label>
		}.into_any(),
		_ => view!{}.into_any(),
	};
}

fn calendarEventDetails_open(
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let recurrenceState = ArcRwSignal::new(CalendarEventRecurrenceState::Loading);
	let bodyRecurrenceState = recurrenceState.clone();
	let resolveRecurrenceState = recurrenceState.clone();
	let recurrenceEvent = event.clone();
	let recurrenceConfig = config.clone();
	let recurrenceActions = moduleActions.clone();
	let bodyEvent = event.clone();
	let bodyConfig = config.clone();
	let bodyActions = moduleActions.clone();
	let bodyModuleId = moduleId.clone();
	let bodyDialog = dialogManager.clone();
	let dialog = DialogData::new()
		.setTitle("MODULE_CALENDAR_EVENT_DETAILS")
		.setBody(move || {
			let actionsRecurrenceState = bodyRecurrenceState.clone();
			let actionsEvent = bodyEvent.clone();
			let actionsConfig = bodyConfig.clone();
			let actions = bodyActions.clone();
			let actionsId = bodyModuleId.clone();
			let actionsDialog = bodyDialog.clone();
			let collectionName = if (bodyEvent.collectionName.is_empty())
			{
				collectionLabel_fallback(&bodyEvent.identity.collectionHref)
			}
			else
			{
				bodyEvent.collectionName.clone()
			};
			view! {
				<div class="module_calendar_event_details">
					<h3>{if (bodyEvent.title.is_empty()) {"—".to_string()} else {bodyEvent.title.clone()}}</h3>
					<dl>
						<div><dt><TranslateText key="MODULE_CALENDAR_COLLECTION"/></dt><dd>{collectionName}</dd></div>
						<div><dt><TranslateText key="MODULE_CALENDAR_START"/></dt><dd>{eventMoment_label(&bodyEvent.start)}</dd></div>
						<div><dt><TranslateText key="MODULE_CALENDAR_END"/></dt><dd>{eventEnd_label(&bodyEvent)}</dd></div>
						{(!bodyEvent.location.is_empty()).then(|| view! {
							<div><dt><TranslateText key="MODULE_CALENDAR_LOCATION"/></dt><dd>{bodyEvent.location.clone()}</dd></div>
						})}
					</dl>
					{(!bodyEvent.description.is_empty()).then(|| view! {
						<div class="module_calendar_event_description">
							<h4><TranslateText key="MODULE_CALENDAR_DESCRIPTION"/></h4>
							<p>{bodyEvent.description.clone()}</p>
						</div>
					})}
					<div class="module_calendar_event_actions">
						{move || calendarEventActions_view(
							actionsRecurrenceState.clone(),actionsEvent.clone(),actionsConfig.clone(),actions.clone(),
							actionsId.clone(),actionsDialog.clone(),
						)}
					</div>
				</div>
			}.into_any()
		})
		.setButtonValidateTitle(None::<String>);
	dialogManager.open(dialog);
	calendarEventRecurrence_resolve(recurrenceEvent,recurrenceConfig,resolveRecurrenceState,recurrenceActions);
}

#[derive(Clone)]
#[cfg_attr(not(feature = "hydrate"),allow(dead_code))]
enum CalendarEventRecurrenceState
{
	Loading,
	Single,
	Series(CalendarEvent),
	Error(CalDavError),
}

fn calendarEventActions_view(
	recurrenceState: ArcRwSignal<CalendarEventRecurrenceState>,
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
) -> leptos::prelude::AnyView
{
	return match recurrenceState.get()
	{
		CalendarEventRecurrenceState::Loading => view! {
			<p class="module_calendar_dialog_notice" role="status"><TranslateText key="MODULE_CALENDAR_EVENT_TYPE_LOADING"/></p>
		}.into_any(),
		CalendarEventRecurrenceState::Error(error) => view! {
			<p class="module_calendar_dialog_notice module_calendar_status--error" role="alert"><TranslateText key={calDavError_key(error)}/></p>
		}.into_any(),
		CalendarEventRecurrenceState::Single => view! {
			<button type="button" on:click={
				let event = event.clone();
				let config = config.clone();
				let moduleActions = moduleActions.clone();
				let moduleId = moduleId.clone();
				let dialogManager = dialogManager.clone();
				move |_| calendarEventEdit_open(
					CalendarEditScope::Event,event.clone(),config.clone(),moduleActions.clone(),moduleId.clone(),dialogManager.clone(),
				)
			}><TranslateText key="MODULE_CALENDAR_EDIT_EVENT"/></button>
			<button type="button" class="danger" on:click=move |_| calendarDeleteConfirmation_open(
				CalendarDeleteScope::Event,event.clone(),config.clone(),moduleActions.clone(),moduleId.clone(),dialogManager.clone(),
			)><TranslateText key="MODULE_CALENDAR_DELETE_EVENT"/></button>
		}.into_any(),
		CalendarEventRecurrenceState::Series(masterEvent) =>
		{
			let editOccurrenceEvent = event.clone();
			let editOccurrenceConfig = config.clone();
			let editOccurrenceActions = moduleActions.clone();
			let editOccurrenceId = moduleId.clone();
			let editOccurrenceDialog = dialogManager.clone();
			let editSeriesConfig = config.clone();
			let editSeriesActions = moduleActions.clone();
			let editSeriesId = moduleId.clone();
			let editSeriesDialog = dialogManager.clone();
			let occurrenceEvent = event.clone();
			let occurrenceConfig = config.clone();
			let occurrenceActions = moduleActions.clone();
			let occurrenceId = moduleId.clone();
			let occurrenceDialog = dialogManager.clone();
			view! {
				<button type="button" on:click=move |_| calendarEventEdit_open(
					CalendarEditScope::Occurrence,editOccurrenceEvent.clone(),editOccurrenceConfig.clone(),
					editOccurrenceActions.clone(),editOccurrenceId.clone(),editOccurrenceDialog.clone(),
				)><TranslateText key="MODULE_CALENDAR_EDIT_OCCURRENCE"/></button>
				<button type="button" on:click=move |_| calendarEventEdit_open(
					CalendarEditScope::Series,masterEvent.clone(),editSeriesConfig.clone(),
					editSeriesActions.clone(),editSeriesId.clone(),editSeriesDialog.clone(),
				)><TranslateText key="MODULE_CALENDAR_EDIT_SERIES"/></button>
				<button type="button" class="danger" on:click=move |_| calendarDeleteConfirmation_open(
					CalendarDeleteScope::Occurrence,occurrenceEvent.clone(),occurrenceConfig.clone(),occurrenceActions.clone(),occurrenceId.clone(),occurrenceDialog.clone(),
				)><TranslateText key="MODULE_CALENDAR_DELETE_OCCURRENCE"/></button>
				<button type="button" class="danger" on:click=move |_| calendarDeleteConfirmation_open(
					CalendarDeleteScope::Series,event.clone(),config.clone(),moduleActions.clone(),moduleId.clone(),dialogManager.clone(),
				)><TranslateText key="MODULE_CALENDAR_DELETE_SERIES"/></button>
			}.into_any()
		},
	};
}

#[derive(Clone)]
struct CalendarEditFormSignals
{
	title: ArcRwSignal<String>,
	description: ArcRwSignal<String>,
	location: ArcRwSignal<String>,
	allDay: ArcRwSignal<bool>,
	allDayStart: ArcRwSignal<String>,
	allDayEnd: ArcRwSignal<String>,
	timedStart: ArcRwSignal<String>,
	timedEnd: ArcRwSignal<String>,
	collectionHref: ArcRwSignal<String>,
}

impl CalendarEditFormSignals
{
	fn new(event: &CalendarEvent) -> Self
	{
		let today = browser_today_get();
		let (allDay,allDayStart,allDayEnd,timedStart,timedEnd) = match (&event.start,&event.end)
		{
			(CalendarMoment::AllDay(start),CalendarMoment::AllDay(end)) =>
			{
				let inclusiveEnd = end.previous_day().filter(|end| end >= start).unwrap_or(*start);
				(
					true,dateInput_format(*start),dateInput_format(inclusiveEnd),
					format!("{}T09:00",dateInput_format(*start)),format!("{}T10:00",dateInput_format(*start)),
				)
			},
			(CalendarMoment::Timed(start),CalendarMoment::Timed(end)) =>
			{
				(
					false,dateInput_format(today),dateInput_format(today),
					calendarTimedInput_format(*start),calendarTimedInput_format(*end),
				)
			},
			_ =>
			{
				(
					false,dateInput_format(today),dateInput_format(today),
					format!("{}T09:00",dateInput_format(today)),format!("{}T10:00",dateInput_format(today)),
				)
			},
		};
		return Self {
			title: ArcRwSignal::new(event.title.clone()),
			description: ArcRwSignal::new(event.description.clone()),
			location: ArcRwSignal::new(event.location.clone()),
			allDay: ArcRwSignal::new(allDay),
			allDayStart: ArcRwSignal::new(allDayStart),
			allDayEnd: ArcRwSignal::new(allDayEnd),
			timedStart: ArcRwSignal::new(timedStart),
			timedEnd: ArcRwSignal::new(timedEnd),
			collectionHref: ArcRwSignal::new(event.identity.collectionHref.clone()),
		};
	}

	fn input_get(&self) -> Option<CalendarCreateInput>
	{
		return calendarCreateInput_get(
			self.title.get_untracked(),self.description.get_untracked(),self.location.get_untracked(),
			self.allDay.get_untracked(),self.allDayStart.get_untracked(),self.allDayEnd.get_untracked(),
			self.timedStart.get_untracked(),self.timedEnd.get_untracked(),
			false,"WEEKLY".to_string(),"1".to_string(),"NEVER".to_string(),
			self.allDayStart.get_untracked(),"1".to_string(),
		);
	}
}

fn calendarEventEdit_open(
	editScope: CalendarEditScope,
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let configSnapshot = config.get_untracked();
	let signals = CalendarEditFormSignals::new(&event);
	let collections = configSnapshot.collections.clone();
	let pending = ArcRwSignal::new(false);
	let bodySignals = signals.clone();
	let bodyCollections = collections.clone();
	let bodyPending = pending.clone();
	let validateSignals = signals.clone();
	let validatePending = pending.clone();
	let closePending = pending.clone();
	let dialog = DialogData::new()
		.setTitle("MODULE_CALENDAR_EDIT_TITLE")
		.setIsLarger(true)
		.setBody(move || calendarEventEditForm_view(
			bodySignals.clone(),bodyCollections.clone(),bodyPending.clone(),editScope,
		))
		.setButtonValidateTitle(Some("MODULE_CALENDAR_UPDATE_ACTION"))
		.setOnValidate({
			let config = config.clone();
			let moduleActions = moduleActions.clone();
			let moduleId = moduleId.clone();
			let dialogManager = dialogManager.clone();
			move |_| {
				if (validatePending.get_untracked()) {return false;}
				let Some(input) = validateSignals.input_get()
				else
				{
					let toaster = expect_toaster();
					moduleActions.task_spawn(async move {
						toastingErr(&toaster,"MODULE_CALENDAR_UPDATE_INVALID").await;
					});
					return false;
				};
				validatePending.set(true);
				calendarEvent_update(
					editScope,event.clone(),config.clone(),validateSignals.collectionHref.get_untracked(),input,validatePending.clone(),
					moduleActions.clone(),moduleId.clone(),dialogManager.clone(),
				);
				return false;
			}
		})
		.setCanClose(move || !closePending.get());
	dialogManager.open(dialog);
}

fn calendarEventEditForm_view(
	signals: CalendarEditFormSignals,
	collections: Vec<CalendarCollection>,
	pending: ArcRwSignal<bool>,
	editScope: CalendarEditScope,
) -> leptos::prelude::AnyView
{
	// Arena handles belong to the edit dialog while the Arc signals remain owned by DialogData.
	let title = RwSignal::from(&signals.title);
	let description = RwSignal::from(&signals.description);
	let location = RwSignal::from(&signals.location);
	let allDay = RwSignal::from(&signals.allDay);
	let allDayStart = RwSignal::from(&signals.allDayStart);
	let allDayEnd = RwSignal::from(&signals.allDayEnd);
	let timedStart = RwSignal::from(&signals.timedStart);
	let timedEnd = RwSignal::from(&signals.timedEnd);
	let collectionHref = RwSignal::from(&signals.collectionHref);
	let scopeKey = match editScope
	{
		CalendarEditScope::Event => "MODULE_CALENDAR_EDIT_EVENT_SCOPE",
		CalendarEditScope::Occurrence => "MODULE_CALENDAR_EDIT_OCCURRENCE_SCOPE",
		CalendarEditScope::Series => "MODULE_CALENDAR_EDIT_SERIES_SCOPE",
	};
	return view! {
		<div class="module_calendar_create_form">
			<p class="module_calendar_dialog_notice"><TranslateText key={scopeKey}/></p>
			{calendarCreateIdentity_view(title,collectionHref,collections)}
			{calendarCreatePeriod_view(
				allDay,allDayStart,allDayEnd,timedStart,timedEnd,
			)}
			{calendarCreateDetails_view(location,description)}
			{move || pending.get().then(|| view! {
				<p class="module_calendar_dialog_notice" role="status"><TranslateText key="MODULE_CALENDAR_UPDATING"/></p>
			})}
		</div>
	}.into_any();
}

#[derive(Clone,Copy)]
enum CalendarDeleteScope
{
	Event,
	Occurrence,
	Series,
}

fn calendarDeleteConfirmation_open(
	scope: CalendarDeleteScope,
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let pending = ArcRwSignal::new(false);
	let messageKey = match scope
	{
		CalendarDeleteScope::Event => "MODULE_CALENDAR_DELETE_EVENT_CONFIRM",
		CalendarDeleteScope::Occurrence => "MODULE_CALENDAR_DELETE_OCCURRENCE_CONFIRM",
		CalendarDeleteScope::Series => "MODULE_CALENDAR_DELETE_SERIES_CONFIRM",
	};
	let validateDialog = dialogManager.clone();
	let validateActions = moduleActions.clone();
	let validateId = moduleId.clone();
	let bodyPending = pending.clone();
	let validatePending = pending.clone();
	let closePending = pending.clone();
	let dialog = DialogData::new()
		.setTitle("MODULE_CALENDAR_DELETE_CONFIRM_TITLE")
		.setBody(move || {
			let pending = bodyPending.clone();
			view! {
				<div class="module_calendar_delete_confirmation">
					<p><TranslateText key={messageKey}/></p>
					{move || pending.get().then(|| view! {
						<p class="module_calendar_dialog_notice" role="status"><TranslateText key="MODULE_CALENDAR_DELETING"/></p>
					})}
				</div>
			}.into_any()
		})
		.setButtonValidateTitle(Some("MODULE_CALENDAR_DELETE_ACTION"))
		.setValidateStyle(DialogActionStyle::Danger)
		.setOnValidate(move |_| {
			if (validatePending.get_untracked()) {return false;}
			validatePending.set(true);
			calendarEvent_delete(
				scope,event.clone(),config.clone(),validatePending.clone(),validateActions.clone(),validateId.clone(),validateDialog.clone(),
			);
			return false;
		})
		.setCanClose(move || !closePending.get());
	dialogManager.open(dialog);
}

fn calendarCreateInput_get(
	title: String,
	description: String,
	location: String,
	allDay: bool,
	allDayStart: String,
	allDayEnd: String,
	timedStart: String,
	timedEnd: String,
	recurrenceEnabled: bool,
	recurrenceFrequency: String,
	recurrenceInterval: String,
	recurrenceEnd: String,
	recurrenceUntil: String,
	recurrenceCount: String,
) -> Option<CalendarCreateInput>
{
	let start = if (allDay)
	{
		CalendarCreateMoment::AllDay(dateInput_parse(&allDayStart)?)
	}
	else
	{
		CalendarCreateMoment::Local(dateTimeInput_parse(&timedStart)?)
	};
	let end = if (allDay)
	{
		CalendarCreateMoment::AllDay(dateInput_parse(&allDayEnd)?.next_day()?)
	}
	else
	{
		CalendarCreateMoment::Local(dateTimeInput_parse(&timedEnd)?)
	};
	let recurrence = if (recurrenceEnabled)
	{
		let frequency = match recurrenceFrequency.as_str()
		{
			"DAILY" => CalendarRecurrenceFrequency::Daily,
			"WEEKLY" => CalendarRecurrenceFrequency::Weekly,
			"MONTHLY" => CalendarRecurrenceFrequency::Monthly,
			"YEARLY" => CalendarRecurrenceFrequency::Yearly,
			_ => return None,
		};
		let endRule = match recurrenceEnd.as_str()
		{
			"NEVER" => CalendarRecurrenceEnd::Never,
			"UNTIL" =>
			{
				let date = dateInput_parse(&recurrenceUntil)?;
				CalendarRecurrenceEnd::Until {
					date,
					utcEndTimestamp: browserLocalDayEnd_timestamp(date)?,
				}
			},
			"COUNT" => CalendarRecurrenceEnd::Count(recurrenceCount.parse().ok()?),
			_ => return None,
		};
		Some(CalendarRecurrence {
			frequency,
			interval: recurrenceInterval.parse().ok()?,
			end: endRule,
		})
	}
	else
	{
		None
	};
	let input = CalendarCreateInput {
		title,
		description,
		location,
		start,
		end,
		timezone: browser::timezone_get(),
		recurrence,
	};
	input.validate().ok()?;
	return Some(input);
}

fn dateInput_parse(value: &str) -> Option<Date>
{
	let mut parts = value.split('-');
	let year = parts.next()?;
	let month = parts.next()?;
	let day = parts.next()?;
	if (parts.next().is_some()
		|| year.len() != 4 || month.len() != 2 || day.len() != 2
		|| !year.bytes().all(|value| value.is_ascii_digit())
		|| !month.bytes().all(|value| value.is_ascii_digit())
		|| !day.bytes().all(|value| value.is_ascii_digit()))
	{
		return None;
	}
	let year = year.parse().ok()?;
	let month: Month = month.parse::<u8>().ok()?.try_into().ok()?;
	let day = day.parse().ok()?;
	return Date::from_calendar_date(year,month,day).ok();
}

fn dateTimeInput_parse(value: &str) -> Option<PrimitiveDateTime>
{
	let (date,time) = value.split_once('T')?;
	let date = dateInput_parse(date)?;
	let mut values = time.split(':');
	let hour = values.next()?.parse().ok()?;
	let minute = values.next()?.parse().ok()?;
	let second = values.next().and_then(|value| value.parse().ok()).unwrap_or(0);
	if (values.next().is_some()) {return None;}
	return Time::from_hms(hour,minute,second).ok().map(|time| PrimitiveDateTime::new(date,time));
}

fn dateInput_format(date: Date) -> String
{
	format!("{:04}-{:02}-{:02}",date.year(),u8::from(date.month()),date.day())
}

#[cfg(feature = "hydrate")]
fn calendarTimedInput_format(timestamp: i64) -> String
{
	let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp as f64 * 1_000.0));
	return format!(
		"{:04}-{:02}-{:02}T{:02}:{:02}",
		date.get_full_year(),date.get_month() + 1,date.get_date(),date.get_hours(),date.get_minutes(),
	);
}

#[cfg(not(feature = "hydrate"))]
fn calendarTimedInput_format(timestamp: i64) -> String
{
	let Ok(dateTime) = OffsetDateTime::from_unix_timestamp(timestamp) else {return String::new()};
	return format!(
		"{:04}-{:02}-{:02}T{:02}:{:02}",
		dateTime.year(),u8::from(dateTime.month()),dateTime.day(),dateTime.hour(),dateTime.minute(),
	);
}

#[cfg(feature = "hydrate")]
fn browserLocalDayEnd_timestamp(date: Date) -> Option<i64>
{
	let date = js_sys::Date::new_with_year_month_day_hr_min_sec(
		date.year() as u32,u8::from(date.month()) as i32 - 1,date.day() as i32,23,59,59,
	);
	let timestamp = date.get_time();
	return timestamp.is_finite().then_some((timestamp / 1_000.0) as i64);
}

#[cfg(not(feature = "hydrate"))]
fn browserLocalDayEnd_timestamp(date: Date) -> Option<i64>
{
	Some(PrimitiveDateTime::new(date,Time::from_hms(23,59,59).ok()?).assume_utc().unix_timestamp())
}

#[cfg(feature = "hydrate")]
fn calendarEventRecurrence_resolve(
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	recurrenceState: ArcRwSignal<CalendarEventRecurrenceState>,
	moduleActions: ModuleActionFn,
)
{
	let config = config.get_untracked();
	let taskActions = moduleActions.clone();
	moduleActions.task_spawn(async move {
		let result = match CalDavClient::new(&config)
		{
			Ok(client) => client.event_master_get(&event,&browser::timezone_get()).await,
			Err(error) => Err(error),
		};
		if (!taskActions.lifecycle_isActive()) {return;}
		recurrenceState.set(match result
		{
			Ok(masterEvent) if masterEvent.recurrent => CalendarEventRecurrenceState::Series(masterEvent),
			Ok(_) => CalendarEventRecurrenceState::Single,
			Err(error) => CalendarEventRecurrenceState::Error(error),
		});
	});
}

#[cfg(not(feature = "hydrate"))]
fn calendarEventRecurrence_resolve(
	_event: CalendarEvent,
	_config: ArcRwSignal<CalendarConfig>,
	recurrenceState: ArcRwSignal<CalendarEventRecurrenceState>,
	_moduleActions: ModuleActionFn,
)
{
	recurrenceState.set(CalendarEventRecurrenceState::Single);
}

#[cfg(feature = "hydrate")]
fn calendarEvent_create(
	config: ArcRwSignal<CalendarConfig>,
	collectionHref: String,
	input: CalendarCreateInput,
	pending: RwSignal<bool>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let config = config.get_untracked();
	let collection = config.collections.iter().find(|collection| collection.href == collectionHref).cloned();
	let toaster = expect_toaster();
	let taskActions = moduleActions.clone();
	moduleActions.task_spawn(async move {
		let result = match (CalDavClient::new(&config),collection)
		{
			(Ok(client),Some(collection)) => client.event_create(&collection,&input).await,
			(Err(error),_) => Err(error),
			(_,None) => Err(CalDavError::InvalidConfiguration),
		};
		if (!taskActions.lifecycle_isActive()) {return;}
		pending.set(false);
		match result
		{
			Ok(()) =>
			{
				dialogManager.clear();
				(taskActions.refreshFn)(moduleId);
				toastingSuccess(&toaster,"MODULE_CALENDAR_CREATE_SUCCESS").await;
			},
			Err(error) => toastingErr(&toaster,calDavError_key(error)).await,
		}
	});
}

#[cfg(not(feature = "hydrate"))]
fn calendarEvent_create(
	_config: ArcRwSignal<CalendarConfig>,
	_collectionHref: String,
	_input: CalendarCreateInput,
	pending: RwSignal<bool>,
	_moduleActions: ModuleActionFn,
	_moduleId: ModuleID,
	_dialogManager: DialogManager,
)
{
	pending.set(false);
}

#[cfg(feature = "hydrate")]
fn calendarEvent_update(
	editScope: CalendarEditScope,
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	collectionHref: String,
	input: CalendarCreateInput,
	pending: ArcRwSignal<bool>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let config = config.get_untracked();
	let collection = config.collections.iter().find(|collection| collection.href == collectionHref).cloned();
	let toaster = expect_toaster();
	let taskActions = moduleActions.clone();
	moduleActions.task_spawn(async move {
		let result = match (CalDavClient::new(&config),collection)
		{
			(Ok(client),Some(collection)) => client.event_update(&event,&collection,&input,editScope).await,
			(Err(error),_) => Err(error),
			(_,None) => Err(CalDavError::InvalidConfiguration),
		};
		pending.set(false);
		if (!taskActions.lifecycle_isActive()) {return;}
		match result
		{
			Ok(()) =>
			{
				dialogManager.clear();
				(taskActions.refreshFn)(moduleId);
				toastingSuccess(&toaster,"MODULE_CALENDAR_UPDATE_SUCCESS").await;
			},
			Err(error) => toastingErr(&toaster,calDavError_key(error)).await,
		}
	});
}

#[cfg(not(feature = "hydrate"))]
fn calendarEvent_update(
	_editScope: CalendarEditScope,
	_event: CalendarEvent,
	_config: ArcRwSignal<CalendarConfig>,
	_collectionHref: String,
	_input: CalendarCreateInput,
	pending: ArcRwSignal<bool>,
	_moduleActions: ModuleActionFn,
	_moduleId: ModuleID,
	_dialogManager: DialogManager,
)
{
	pending.set(false);
}

#[cfg(feature = "hydrate")]
fn calendarEvent_delete(
	scope: CalendarDeleteScope,
	event: CalendarEvent,
	config: ArcRwSignal<CalendarConfig>,
	pending: ArcRwSignal<bool>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
	dialogManager: DialogManager,
)
{
	let config = config.get_untracked();
	let toaster = expect_toaster();
	let taskActions = moduleActions.clone();
	moduleActions.task_spawn(async move {
		let result = match CalDavClient::new(&config)
		{
			Ok(client) => match scope
			{
				CalendarDeleteScope::Occurrence => client.event_delete_occurrence(&event).await,
				CalendarDeleteScope::Event | CalendarDeleteScope::Series => client.event_delete_series(&event).await,
			},
			Err(error) => Err(error),
		};
		pending.set(false);
		if (!taskActions.lifecycle_isActive()) {return;}
		match result
		{
			Ok(()) =>
			{
				dialogManager.clear();
				(taskActions.refreshFn)(moduleId);
				toastingSuccess(&toaster,"MODULE_CALENDAR_DELETE_SUCCESS").await;
			},
			Err(error) => toastingErr(&toaster,calDavError_key(error)).await,
		}
	});
}

#[cfg(not(feature = "hydrate"))]
fn calendarEvent_delete(
	_scope: CalendarDeleteScope,
	_event: CalendarEvent,
	_config: ArcRwSignal<CalendarConfig>,
	pending: ArcRwSignal<bool>,
	_moduleActions: ModuleActionFn,
	_moduleId: ModuleID,
	_dialogManager: DialogManager,
)
{
	pending.set(false);
}

#[cfg(feature = "hydrate")]
fn eventMoment_label(moment: &CalendarMoment) -> String
{
	return match moment
	{
		CalendarMoment::AllDay(date) => format!("{:02}/{:02}/{:04}",date.day(),u8::from(date.month()),date.year()),
		CalendarMoment::Timed(timestamp) =>
		{
			let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(*timestamp as f64 * 1_000.0));
			format!("{:02}/{:02}/{:04} {:02}:{:02}",date.get_date(),date.get_month() + 1,date.get_full_year(),date.get_hours(),date.get_minutes())
		},
	};
}

#[cfg(test)]
mod tests
{
	use super::{CalendarEditFormSignals,CalendarScrollState,event_overlapsLocalDate};
	use crate::front::modules::calendar::domain::{
		CalendarEvent,CalendarEventIdentity,CalendarMoment,CalendarPeriod,CalendarViewMode,
	};
	use leptos::prelude::{GetUntracked,Owner};
	use time::{Date,Month,PrimitiveDateTime,Time};

	#[test]
	fn timedEvent_remainsVisibleAfterItsEndTime()
	{
		let date = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let timestamp = |hour| PrimitiveDateTime::new(date,Time::from_hms(hour,0,0).unwrap())
			.assume_utc().unix_timestamp();
		let event = CalendarEvent {
			identity: CalendarEventIdentity {
				collectionHref: "https://calendar.invalid/test/".to_string(),
				resourceHref: "https://calendar.invalid/test/past.ics".to_string(),
				uid: "past".to_string(),
				occurrenceId: None,
			},
			collectionName: "Test".to_string(),
			collectionColor: None,
			title: "Past event".to_string(),
			description: String::new(),
			location: String::new(),
			start: CalendarMoment::Timed(timestamp(8)),
			end: CalendarMoment::Timed(timestamp(9)),
			occurrence: None,
			recurrent: false,
			etag: None,
		};

		assert!(event_overlapsLocalDate(&event,date));
	}

	#[test]
	fn calendarScrollState_preservesScrollOnlyForTheSamePeriod()
	{
		let anchor = Date::from_calendar_date(2026,Month::August,23).unwrap();
		let month = CalendarPeriod::from_anchor(anchor,CalendarViewMode::Month);
		let week = CalendarPeriod::from_anchor(anchor,CalendarViewMode::Week);
		let mut state = CalendarScrollState::default();

		state.period_apply(Some(month));
		state.top = 120;
		state.left = 35;
		state.period_apply(Some(month));
		assert_eq!((state.top,state.left),(120,35));

		state.period_apply(Some(week));
		assert_eq!((state.top,state.left),(0,0));
	}

	#[test]
	fn editFormSignals_surviveTheDetailsDialogOwnerCleanup()
	{
		let owner = Owner::new();
		let event = CalendarEvent {
			identity: CalendarEventIdentity {
				collectionHref: "https://calendar.invalid/test/".to_string(),
				resourceHref: "https://calendar.invalid/test/recurrent.ics".to_string(),
				uid: "recurrent".to_string(),
				occurrenceId: Some("20260823T100000".to_string()),
			},
			collectionName: "Test".to_string(),
			collectionColor: None,
			title: "Occurrence".to_string(),
			description: String::new(),
			location: String::new(),
			start: CalendarMoment::Timed(1_777_197_600),
			end: CalendarMoment::Timed(1_777_201_200),
			occurrence: None,
			recurrent: true,
			etag: None,
		};
		let signals = owner.with(|| CalendarEditFormSignals::new(&event));

		owner.cleanup();

		assert_eq!(signals.collectionHref.get_untracked(),event.identity.collectionHref);
		assert_eq!(signals.title.get_untracked(),"Occurrence");
	}
}

fn eventEnd_label(event: &CalendarEvent) -> String
{
	if let (CalendarMoment::AllDay(start),CalendarMoment::AllDay(end)) = (&event.start,&event.end)
	{
		if let Some(inclusiveEnd) = end.previous_day().filter(|inclusiveEnd| inclusiveEnd >= start)
		{
			return eventMoment_label(&CalendarMoment::AllDay(inclusiveEnd));
		}
	}
	return eventMoment_label(&event.end);
}

#[cfg(not(feature = "hydrate"))]
fn eventMoment_label(moment: &CalendarMoment) -> String
{
	return match moment
	{
		CalendarMoment::AllDay(date) => format!("{:02}/{:02}/{:04}",date.day(),u8::from(date.month()),date.year()),
		CalendarMoment::Timed(timestamp) => OffsetDateTime::from_unix_timestamp(*timestamp).ok()
			.map(|date| format!("{:02}/{:02}/{:04} {:02}:{:02}",date.day(),u8::from(date.month()),date.year(),date.hour(),date.minute()))
			.unwrap_or_default(),
	};
}
