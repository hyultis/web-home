use super::{AiAutomationHistoryEntry,AiModuleCapabilities};
use crate::front::modules::components::distant_time_simpler;
use crate::front::utils::translate::TranslateText;
use leptos::prelude::{ClassAttribute,CollectView,ElementChild,IntoAny};
use leptos::{component,view,IntoView};

struct AiHistoryArgumentDisplay
{
	id: String,
	translateKey: Option<&'static str>,
	value: String,
}

struct AiHistoryEntryDisplay
{
	appliedAt: i64,
	contextName: String,
	targetModuleType: String,
	action: String,
	actionTranslateKey: Option<&'static str>,
	arguments: Vec<AiHistoryArgumentDisplay>,
}

impl AiHistoryEntryDisplay
{
	fn new(entry: AiAutomationHistoryEntry,modules: &[AiModuleCapabilities]) -> Self
	{
		let capability = modules.iter()
			.find(|module| module.moduleId == entry.targetModuleId)
			.and_then(|module| module.catalog.action_get(&entry.action));
		let arguments = entry.arguments.into_iter().map(|argument| {
			let translateKey = capability.and_then(|capability| capability.arguments.iter()
				.find(|definition| definition.id == argument.id)
				.map(|definition| definition.translateKey));
			return AiHistoryArgumentDisplay {id: argument.id,translateKey,value: argument.value};
		}).collect();
		return Self {
			appliedAt: entry.appliedAt,
			contextName: entry.contextName,
			targetModuleType: entry.targetModuleType,
			action: entry.action,
			actionTranslateKey: capability.map(|capability| capability.translateKey),
			arguments,
		};
	}
}

#[component]
pub(crate) fn AiAutomationHistoryView(
	history: Vec<AiAutomationHistoryEntry>,
	modules: Vec<AiModuleCapabilities>,
) -> impl IntoView
{
	let entries = history.into_iter().rev()
		.map(|entry| AiHistoryEntryDisplay::new(entry,&modules))
		.collect::<Vec<_>>();

	return view! {
		<div class="ai_history">
			<p class="ai_history_help"><TranslateText key="FRONTAI_HISTORY_HELP"/></p>
			{if entries.is_empty()
			{
				view! {<p class="ai_history_empty"><TranslateText key="FRONTAI_HISTORY_EMPTY"/></p>}.into_any()
			}
			else
			{
				view! {
					<ol class="ai_history_list">{entries.into_iter().map(|entry| {
						let actionLabel = match entry.actionTranslateKey
						{
							Some(key) => view! {<TranslateText key/>}.into_any(),
							None => view! {{entry.action}}.into_any(),
						};
						view! {
							<li class="ai_history_entry">
								<div class="ai_history_entry_header">
									<strong>{entry.targetModuleType}{" · "}{actionLabel}</strong>
									<span class="ai_history_time">{distant_time_simpler(entry.appliedAt)}</span>
								</div>
								<p class="ai_history_context">
									<TranslateText key="FRONTAI_HISTORY_CONTEXT"/>{" "}{entry.contextName}
								</p>
								<details>
									<summary><TranslateText key="FRONTAI_HISTORY_DETAILS"/></summary>
									<dl>{entry.arguments.into_iter().map(|argument| {
										let label = match argument.translateKey
										{
											Some(key) => view! {<TranslateText key/>}.into_any(),
											None => view! {{argument.id}}.into_any(),
										};
										return view! {
											<div>
												<dt>{label}</dt>
												<dd>{argument.value}</dd>
											</div>
										};
									}).collect_view()}</dl>
								</details>
							</li>
						}
					}).collect_view()}</ol>
				}.into_any()
			}}
		</div>
	}.into_any();
}
