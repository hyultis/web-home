use leptos::ev::{ClipboardEvent,KeyboardEvent,MouseEvent};
use leptos::html::Div;
use leptos::prelude::{
	AnyView,ArcRwSignal,AriaAttributes,ClassAttribute,CollectView,Effect,ElementChild,For,Get,
	GetUntracked,GlobalAttributes,IntoAny,NodeRef,NodeRefAttribute,OnAttribute,PropAttribute,Set,
	Update,WithUntracked,
};
use leptos::tachys::html::attribute::custom::CustomAttribute;
use leptos::{component,view,IntoView};
use leptos_use::watch_debounced;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement,InputEvent};
#[cfg(feature="hydrate")]
use web_sys::Node;

use crate::api::modules::components::{ModuleID};
use crate::front::modules::components::Cache;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::TranslateText;

use super::MAX_LENGTH;
use super::document::{TodoBlock,TodoBlockId,TodoBlockKind,TodoEditorDocument,TodoEnterResult,TodoInline};

const SAVE_DELAY_MS: f64 = 5000.0;

#[derive(Clone)]
struct TodoEditorRuntime
{
	document: ArcRwSignal<TodoEditorDocument>,
	source: ArcRwSignal<String>,
	cache: ArcRwSignal<Cache>,
	dirty: ArcRwSignal<bool>,
	revision: ArcRwSignal<u64>,
	linksRevision: ArcRwSignal<u64>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
}

#[derive(Clone,Debug,Eq,PartialEq)]
struct TodoEditorLink
{
	text: String,
	href: String,
}

struct TodoEditorInputResult
{
	text: String,
	focus: Option<(TodoBlockId,usize)>,
}

struct TodoEditorSelection
{
	anchorBlockId: TodoBlockId,
	anchorElement: HtmlElement,
	anchorByte: usize,
	focusBlockId: TodoBlockId,
	focusElement: HtmlElement,
	focusByte: usize,
}

impl TodoEditorSelection
{
	fn single_get(&self) -> Option<(TodoBlockId,HtmlElement,usize,usize)>
	{
		if (self.anchorBlockId!=self.focusBlockId || self.anchorElement!=self.focusElement)
		{
			return None;
		}
		return Some((
			self.anchorBlockId,
			self.anchorElement.clone(),
			self.anchorByte.min(self.focusByte),
			self.anchorByte.max(self.focusByte),
		));
	}
}

impl TodoEditorRuntime
{
	fn new(source: ArcRwSignal<String>,cache: ArcRwSignal<Cache>,moduleActions: ModuleActionFn,moduleId: ModuleID) -> Self
	{
		let document = TodoEditorDocument::source_parse(&source.get_untracked());
		return Self {
			document: ArcRwSignal::new(document),
			source,
			cache,
			dirty: ArcRwSignal::new(false),
			revision: ArcRwSignal::new(0),
			linksRevision: ArcRwSignal::new(0),
			moduleActions,
			moduleId,
		};
	}

	fn source_sync(&self)
	{
		let source = self.document.with_untracked(TodoEditorDocument::source_get);
		if (source==self.source.get_untracked())
		{
			return;
		}
		self.cache.update(|cache| cache.update());
		self.dirty.set(true);
		self.linksRevision.update(|revision| *revision=revision.wrapping_add(1));
		self.source.set(source);
	}

	fn source_externalApply(&self,source: String)
	{
		if (source==self.document.with_untracked(TodoEditorDocument::source_get))
		{
			return;
		}
		self.document.set(TodoEditorDocument::source_parse(&source));
		self.dirty.set(false);
		self.revision.update(|revision| *revision=revision.wrapping_add(1));
	}

	fn save_now(&self)
	{
		if (!self.dirty.get_untracked())
		{
			return;
		}
		self.dirty.set(false);
		(self.moduleActions.updateFn)(self.moduleId.clone());
	}

	fn rebuild_apply(&self)
	{
		self.revision.update(|revision| *revision=revision.wrapping_add(1));
	}

	fn block_textInput(
		&self,
		id: TodoBlockId,
		rawText: String,
		caretByte: usize,
		isComposing: bool,
		shortcutSeparatorInserted: bool,
	) -> TodoEditorInputResult
	{
		let rawText = rawText.replace("\r\n","\n").replace('\r',"\n");
		if (rawText.contains('\n') && !isComposing)
		{
			return self.block_multilineInput(id,rawText,caretByte);
		}

		let (oldText,currentSourceLength) = self.document.with_untracked(|document| {
			let oldText = document.block_get(id).map(|block| block.text_get().to_string()).unwrap_or_default();
			return (oldText,document.source_get().len());
		});
		let baseLength = currentSourceLength.saturating_sub(oldText.len());
		let allowedLength = if (currentSourceLength>MAX_LENGTH)
		{
			oldText.len()
		}
		else
		{
			MAX_LENGTH.saturating_sub(baseLength)
		};
		let normalizedText = text_truncate(rawText,allowedLength);
		let Some((changed,shortcutApplied,text)) = self.document.try_update(|document| {
			let changed = document.block_text_set(id,normalizedText);
			let shortcutApplied = !isComposing
				&& shortcutSeparatorInserted
				&& document.block_shortcut_apply(id,caretByte);
			let text = document.block_get(id).map(|block| block.text_get().to_string()).unwrap_or_default();
			return (changed || shortcutApplied,shortcutApplied,text);
		}) else {
			return TodoEditorInputResult {text: oldText,focus: None};
		};

		if (changed)
		{
			self.source_sync();
		}
		if (shortcutApplied)
		{
			self.rebuild_apply();
			return TodoEditorInputResult {text,focus: Some((id,0))};
		}
		return TodoEditorInputResult {text,focus: None};
	}

	fn block_shortcutSpace(&self,id: TodoBlockId,visibleText: &str,markerEnd: usize) -> bool
	{
		let previousDocument = self.document.get_untracked();
		let applied = self.document.try_update(|document| {
			if (!document.block_shortcutSpace_apply(id,visibleText,markerEnd))
			{
				return false;
			}
			if (document.source_get().len()>MAX_LENGTH)
			{
				*document = previousDocument.clone();
				return false;
			}
			return true;
		}).unwrap_or(false);
		if (!applied)
		{
			return false;
		}
		self.source_sync();
		self.rebuild_apply();
		return true;
	}

	fn block_multilineInput(&self,id: TodoBlockId,rawText: String,caretByte: usize) -> TodoEditorInputResult
	{
		let previousDocument = self.document.get_untracked();
		let result = self.document.try_update(|document| {
			let (blockIds,structureChanged) = document.block_linesReplace(id,&rawText)?;
			if (document.source_get().len()>MAX_LENGTH)
			{
				*document = previousDocument.clone();
				return None;
			}
			let focus = document.blocks_sourceOffsetPosition(&blockIds,caretByte)?;
			return Some((focus,structureChanged));
		}).flatten();
		let Some(((focusId,focusByte),structureChanged)) = result else {
			let text = self.document.with_untracked(|document| document.block_get(id).map(|block| block.text_get().to_string()).unwrap_or_default());
			return TodoEditorInputResult {text,focus: Some((id,caretByte.min(previousDocument.block_get(id).map(|block| block.text_get().len()).unwrap_or_default())))};
		};

		self.source_sync();
		if (!structureChanged)
		{
			let text = self.document.with_untracked(|document| document.block_get(id).map(|block| block.text_get().to_string()).unwrap_or_default());
			return TodoEditorInputResult {text,focus: None};
		}
		self.rebuild_apply();
		let text = self.document.with_untracked(|document| document.block_get(focusId).map(|block| block.text_get().to_string()).unwrap_or_default());
		return TodoEditorInputResult {text,focus: Some((focusId,focusByte))};
	}

	fn block_enter(&self,id: TodoBlockId,byteStart: usize,byteEnd: usize) -> Option<(TodoBlockId,usize)>
	{
		let previousDocument = self.document.get_untracked();
		let result = self.document.try_update(|document| {
			let result = document.block_enterRange(id,byteStart,byteEnd)?;
			if (document.source_get().len()>MAX_LENGTH)
			{
				*document = previousDocument.clone();
				return None;
			}
			return Some(result);
		}).flatten()?;

		self.source_sync();
		self.rebuild_apply();
		return Some(match result {
			TodoEnterResult::Inserted(newId) => (newId,0),
			TodoEnterResult::Unstyled => (id,0),
		});
	}

	fn block_rangeReplace(
		&self,
		firstId: TodoBlockId,
		firstByte: usize,
		secondId: TodoBlockId,
		secondByte: usize,
		replacement: &str,
	) -> Option<(TodoBlockId,usize)>
	{
		let previousDocument = self.document.get_untracked();
		let focus = self.document.try_update(|document| {
			let Some(focus) = document.block_rangeReplace(firstId,firstByte,secondId,secondByte,replacement) else {
				*document = previousDocument.clone();
				return None;
			};
			if (document.source_get().len()>MAX_LENGTH)
			{
				*document = previousDocument.clone();
				return None;
			}
			return Some(focus);
		}).flatten()?;
		self.source_sync();
		self.rebuild_apply();
		return Some(focus);
	}

	fn block_backspaceAtStart(&self,id: TodoBlockId) -> Option<(TodoBlockId,usize)>
	{
		let kind = self.document.with_untracked(|document| document.block_get(id).map(TodoBlock::kind_get))?;
		let focus = self.document.try_update(|document| {
			if (kind==TodoBlockKind::Paragraph)
			{
				return document.block_mergePrevious(id);
			}
			if (document.block_unstyle(id))
			{
				return Some((id,0));
			}
			return None;
		}).flatten()?;

		self.source_sync();
		self.rebuild_apply();
		return Some(focus);
	}

	fn block_deleteAtEnd(&self,id: TodoBlockId) -> Option<(TodoBlockId,usize)>
	{
		let caretIndex = self.document.try_update(|document| document.block_mergeNext(id)).flatten()?;
		self.source_sync();
		self.rebuild_apply();
		return Some((id,caretIndex));
	}

	fn block_taskToggle(&self,id: TodoBlockId) -> bool
	{
		let previousDocument = self.document.get_untracked();
		let toggled = self.document.try_update(|document| {
			if (!document.block_task_toggle(id))
			{
				return false;
			}
			if (document.source_get().len()>MAX_LENGTH)
			{
				*document = previousDocument.clone();
				return false;
			}
			return true;
		}).unwrap_or(false);
		if (!toggled)
		{
			return false;
		}

		self.source_sync();
		self.rebuild_apply();
		return true;
	}

	fn block_links_get(&self,id: TodoBlockId) -> Vec<TodoEditorLink>
	{
		return self.document.with_untracked(|document| {
			document.block_get(id)
				.map(TodoBlock::inlines_get)
				.unwrap_or_default()
				.into_iter()
				.filter_map(|inline| match inline {
					TodoInline::Text(_) => None,
					TodoInline::Link {text,href} => Some(TodoEditorLink {text,href}),
				})
				.collect()
		});
	}

	fn blockTextId_get(&self,id: TodoBlockId) -> String
	{
		return format!("module-todo-{}-block-{}",self.moduleId.id,id.value_get());
	}

	fn editorId_get(&self) -> String
	{
		return format!("module-todo-{}",self.moduleId.id);
	}

	fn blockCheckboxId_get(&self,id: TodoBlockId) -> String
	{
		return format!("{}-checkbox",self.blockTextId_get(id));
	}
}

pub(super) fn draw(
	content: ArcRwSignal<String>,
	cache: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
) -> AnyView
{
	return view!{
		<TodoEditor content=content cache=cache moduleActions=moduleActions moduleId=moduleId/>
	}.into_any();
}

#[component]
fn TodoEditor(
	content: ArcRwSignal<String>,
	cache: ArcRwSignal<Cache>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
) -> impl IntoView
{
	let runtime = TodoEditorRuntime::new(content.clone(),cache,moduleActions,moduleId.clone());
	let contentId = format!("module-todo-{}",moduleId.id);
	let counterId = format!("{}-counter",contentId);
	let helpId = format!("{}-help",contentId);
	let shortcutsId = format!("{}-shortcuts",contentId);
	let labelId = format!("{}-label",contentId);

	let saveRuntime = runtime.clone();
	let contentWatcher = content.clone();
	let _saveWatcher = watch_debounced(
		move || contentWatcher.get(),
		move |_,_,_| saveRuntime.save_now(),
		SAVE_DELAY_MS,
	);

	let externalRuntime = runtime.clone();
	Effect::new(move || {
		externalRuntime.source_externalApply(externalRuntime.source.get());
	});

	let blocksRuntime = runtime.clone();
	let childRuntime = runtime.clone();
	let contentLength = content.clone();
	let editorRef = NodeRef::<Div>::new();
	let inputRuntime = runtime.clone();
	let compositionRuntime = runtime.clone();
	let keyRuntime = runtime.clone();
	let pasteRuntime = runtime.clone();
	return view!{
		<div class="module_todo_layout">
			<span id={labelId.clone()} class="visually_hidden"><TranslateText key="MODULE_TODO_CONTENT"/></span>
			<span id={helpId.clone()} class="visually_hidden"><TranslateText key="MODULE_TODO_EDITOR_HELP"/></span>
			<div
				id={contentId}
				class="module_todo_editor"
				contenteditable="plaintext-only"
				spellcheck="true"
				role="textbox"
				aria-multiline="true"
				aria-labelledby={labelId}
				aria-describedby={format!("{} {}",counterId,helpId)}
				node_ref=editorRef
				on:input=move |event| {
					let inputEvent = event.unchecked_ref::<InputEvent>();
					let shortcutSeparatorInserted = inputEvent.data()
						.map(|data| data==" " || data=="\u{a0}")
						.unwrap_or(false);
					todoEditorInput_apply(
						inputRuntime.clone(),
						editorRef,
						inputEvent.is_composing(),
						shortcutSeparatorInserted,
					);
				}
				on:compositionend=move |_| {
					todoEditorInput_apply(compositionRuntime.clone(),editorRef,false,false);
				}
				on:keydown=move |event: KeyboardEvent| {
					todoEditorKey_apply(keyRuntime.clone(),editorRef,event);
				}
				on:paste=move |event: ClipboardEvent| {
					todoEditorPaste_apply(pasteRuntime.clone(),editorRef,event);
				}>
				<For
					each=move || {
						let revision = blocksRuntime.revision.get();
						return blocksRuntime.document.with_untracked(|document| {
							document.blocks_get().iter().cloned()
								.map(|block| (revision,block))
								.collect::<Vec<_>>()
						});
					}
					key=|(revision,block)| (*revision,block.id_get())
					children=move |(_,block)| view!{
						<TodoEditorBlock block=block runtime=childRuntime.clone()/>
					}
				/>
			</div>
			<div class="module_todo_footer">
				<div class="module_todo_help alttext_upper" tabindex="0" aria-describedby={shortcutsId.clone()}>
					<i class="iconoir-info-circle" aria-hidden="true"></i>
					<span class="visually_hidden"><TranslateText key="MODULE_TODO_COMMANDS"/></span>
					<div id={shortcutsId.clone()} class="alttext module_todo_help_overlay" role="tooltip">
						<span class="module_todo_help_title"><TranslateText key="MODULE_TODO_COMMANDS"/></span>
						<span class="module_todo_help_commands">
							<span class="module_todo_help_command">
								<kbd>"# / ## / ###"</kbd>
								<span><TranslateText key="MODULE_TODO_COMMAND_HEADING"/></span>
							</span>
							<span class="module_todo_help_command">
								<kbd>"-"</kbd>
								<span><TranslateText key="MODULE_TODO_COMMAND_LIST"/></span>
							</span>
							<span class="module_todo_help_command">
								<kbd>"* / *x"</kbd>
								<span><TranslateText key="MODULE_TODO_COMMAND_TASK"/></span>
							</span>
							<span class="module_todo_help_command">
								<kbd>"http(s)://"</kbd>
								<span><TranslateText key="MODULE_TODO_COMMAND_LINK"/></span>
							</span>
						</span>
					</div>
				</div>
				<span id={counterId} class="module_todo_counter">{move || contentLength.get().len()}/{MAX_LENGTH}c</span>
			</div>
		</div>
	}.into_any();
}

#[component]
fn TodoEditorBlock(block: TodoBlock,runtime: TodoEditorRuntime) -> impl IntoView
{
	let blockId = block.id_get();
	let kind = block.kind_get();
	let text = block.text_get().to_string();
	let textId = runtime.blockTextId_get(blockId);
	let className = match kind {
		TodoBlockKind::Paragraph => "module_todo_block module_todo_block--paragraph",
		TodoBlockKind::Heading(1) => "module_todo_block module_todo_block--heading-1",
		TodoBlockKind::Heading(2) => "module_todo_block module_todo_block--heading-2",
		TodoBlockKind::Heading(_) => "module_todo_block module_todo_block--heading-3",
		TodoBlockKind::ListItem => "module_todo_block module_todo_block--list",
		TodoBlockKind::Task(false) => "module_todo_block module_todo_block--task",
		TodoBlockKind::Task(true) => "module_todo_block module_todo_block--task module_todo_block--checked",
	};
	let rowRole = if matches!(kind,TodoBlockKind::Heading(_)) {"heading"} else {"presentation"};
	let headingLevel = match kind {
		TodoBlockKind::Heading(level) => Some(level.to_string()),
		_ => None,
	};

	let marker = match kind {
		TodoBlockKind::Task(checked) => {
			let checkboxId = runtime.blockCheckboxId_get(blockId);
			let toggleRuntime = runtime.clone();
			Some(view!{
				<span class="module_todo_marker module_todo_marker--task" contenteditable="false">
					<input
						id={checkboxId.clone()}
						type="checkbox"
						prop:checked={checked}
						on:change=move |_| {
							if (toggleRuntime.block_taskToggle(blockId))
							{
								todoEditorCheckbox_focusSchedule(toggleRuntime.blockCheckboxId_get(blockId));
							}
						}/>
					<label class="visually_hidden" for={checkboxId}><TranslateText key="MODULE_TODO_TASK_TOGGLE"/></label>
				</span>
			}.into_any())
		},
		TodoBlockKind::ListItem => Some(view!{
			<span class="module_todo_marker module_todo_marker--list" aria-hidden="true"></span>
		}.into_any()),
		TodoBlockKind::Paragraph | TodoBlockKind::Heading(_) => None,
	};

	let linksRuntime = runtime.clone();
	return view!{
		<div class={className} role={rowRole}>
			<div class="module_todo_gutter" contenteditable="false">
				<div class="module_todo_link_actions">
					{move || {
						let _ = linksRuntime.linksRevision.get();
						return linksRuntime.block_links_get(blockId).into_iter().map(|link| {
							let hrefOpen = link.href.clone();
							return view!{
								<a
									class="module_todo_link_action"
									href={link.href}
									target="_blank"
									rel="noopener noreferrer nofollow"
									on:click=move |event: MouseEvent| {
										event.prevent_default();
										event.stop_propagation();
										todoEditorLink_open(&hrefOpen);
									}>
									<i class="iconoir-open-new-window" aria-hidden="true"></i>
									<span class="visually_hidden">{link.text}</span>
									<span class="visually_hidden"><TranslateText key="MODULE_TODO_OPEN_LINK_ACTION"/></span>
								</a>
							};
						}).collect_view();
					}}
				</div>
				{marker}
			</div>
			<div id={textId} class="module_todo_block_text">{text}</div>
		</div>
	}.attr("aria-level",headingLevel)
		.attr("data-todo-block-id",blockId.value_get().to_string())
		.into_any();
}

fn todoEditorInput_apply(
	runtime: TodoEditorRuntime,
	editorRef: NodeRef<Div>,
	isComposing: bool,
	shortcutSeparatorInserted: bool,
)
{
	let Some(editor) = todoEditorElement_get(editorRef) else {return};
	let Some(selection) = todoEditorSelection_get(&editor) else {return};
	let Some((blockId,textElement,_,byteEnd)) = selection.single_get() else {return};
	let rawText = todoEditorText_get(&textElement);
	let result = runtime.block_textInput(blockId,rawText,byteEnd,isComposing,shortcutSeparatorInserted);
	if (result.focus.is_none() && todoEditorText_get(&textElement)!=result.text)
	{
		textElement.set_text_content(Some(&result.text));
		todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(blockId),result.text.len());
	}
	if let Some((focusId,byteIndex)) = result.focus
	{
		todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(focusId),byteIndex);
	}
}

fn todoEditorKey_apply(runtime: TodoEditorRuntime,editorRef: NodeRef<Div>,event: KeyboardEvent)
{
	if (event.is_composing())
	{
		return;
	}
	let Some(editor) = todoEditorElement_get(editorRef) else {return};
	let Some(selection) = todoEditorSelection_get(&editor) else {return};
	if let Some(focus) = todoEditorCrossBlockKey_apply(&runtime,&selection,&event)
	{
		event.prevent_default();
		if let Some((focusId,byteIndex)) = focus
		{
			todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(focusId),byteIndex);
		}
		return;
	}
	let Some((blockId,textElement,byteStart,byteEnd)) = selection.single_get() else {return};
	let text = todoEditorText_get(&textElement);
	if (event.key()==" " && byteStart==byteEnd && runtime.block_shortcutSpace(blockId,&text,byteStart))
	{
		event.prevent_default();
		todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(blockId),0);
		return;
	}

	if (event.key()=="Enter")
	{
		let Some((focusId,byteIndex)) = runtime.block_enter(blockId,byteStart,byteEnd) else {return};
		event.prevent_default();
		todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(focusId),byteIndex);
		return;
	}

	let focus = match event.key().as_str() {
		"Backspace" if byteStart==0 && byteEnd==0 => runtime.block_backspaceAtStart(blockId),
		"Delete" if byteStart==text.len() && byteEnd==text.len() => runtime.block_deleteAtEnd(blockId),
		_ => None,
	};
	let Some((focusId,byteIndex)) = focus else {return};
	event.prevent_default();
	todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(focusId),byteIndex);
}

fn todoEditorCrossBlockKey_apply(
	runtime: &TodoEditorRuntime,
	selection: &TodoEditorSelection,
	event: &KeyboardEvent,
) -> Option<Option<(TodoBlockId,usize)>>
{
	if (selection.anchorBlockId==selection.focusBlockId)
	{
		return None;
	}
	let key = event.key();
	let replacement = match key.as_str() {
		"Backspace" | "Delete" => "",
		"Enter" => "\n",
		key if key.chars().count()==1 && !event.ctrl_key() && !event.meta_key() && !event.alt_key() => key,
		_ => return None,
	};
	return Some(runtime.block_rangeReplace(
		selection.anchorBlockId,
		selection.anchorByte,
		selection.focusBlockId,
		selection.focusByte,
		replacement,
	));
}

fn todoEditorPaste_apply(runtime: TodoEditorRuntime,editorRef: NodeRef<Div>,event: ClipboardEvent)
{
	let Some(clipboard) = event.clipboard_data() else {return};
	let Ok(replacement) = clipboard.get_data("text/plain") else {return};
	let replacement = replacement.replace("\r\n","\n").replace('\r',"\n");
	let Some(editor) = todoEditorElement_get(editorRef) else {return};
	let Some(selection) = todoEditorSelection_get(&editor) else {return};
	event.prevent_default();
	let Some((focusId,byteIndex)) = runtime.block_rangeReplace(
		selection.anchorBlockId,
		selection.anchorByte,
		selection.focusBlockId,
		selection.focusByte,
		&replacement,
	) else {return};
	todoEditorText_focusSchedule(runtime.editorId_get(),runtime.blockTextId_get(focusId),byteIndex);
}

fn text_truncate(mut text: String,maxBytes: usize) -> String
{
	if (text.len()<=maxBytes)
	{
		return text;
	}
	let mut boundary = maxBytes.min(text.len());
	while (boundary>0 && !text.is_char_boundary(boundary))
	{
		boundary -= 1;
	}
	text.truncate(boundary);
	return text;
}

#[cfg(feature="hydrate")]
fn todoEditorElement_get(textRef: NodeRef<Div>) -> Option<HtmlElement>
{
	return textRef.get_untracked().map(|element| element.unchecked_into());
}

#[cfg(not(feature="hydrate"))]
fn todoEditorElement_get(_: NodeRef<Div>) -> Option<HtmlElement>
{
	return None;
}

#[cfg(feature="hydrate")]
fn todoEditorSelection_get(editor: &HtmlElement) -> Option<TodoEditorSelection>
{
	let selection = web_sys::window()?.get_selection().ok()??;
	let anchorNode = selection.anchor_node()?;
	let focusNode = selection.focus_node()?;
	let (anchorElement,anchorBlockId) = todoEditorTextElement_get(editor,&anchorNode)?;
	let (focusElement,focusBlockId) = todoEditorTextElement_get(editor,&focusNode)?;
	let anchorText = todoEditorText_get(&anchorElement);
	let focusText = todoEditorText_get(&focusElement);
	let anchorUtf16 = todoEditorNodeUtf16Offset_get(&anchorElement,&anchorNode,selection.anchor_offset() as usize)?;
	let focusUtf16 = todoEditorNodeUtf16Offset_get(&focusElement,&focusNode,selection.focus_offset() as usize)?;
	let anchor = todoEditorUtf16Offset_toByte(&anchorText,anchorUtf16)?;
	let focus = todoEditorUtf16Offset_toByte(&focusText,focusUtf16)?;
	return Some(TodoEditorSelection {
		anchorBlockId,
		anchorElement,
		anchorByte: anchor,
		focusBlockId,
		focusElement,
		focusByte: focus,
	});
}

#[cfg(not(feature="hydrate"))]
fn todoEditorSelection_get(_: &HtmlElement) -> Option<TodoEditorSelection>
{
	return None;
}

#[cfg(feature="hydrate")]
fn todoEditorTextElement_get(editor: &HtmlElement,node: &Node) -> Option<(HtmlElement,TodoBlockId)>
{
	let editorNode: Node = editor.clone().unchecked_into();
	let mut currentNode = Some(node.clone());
	while let Some(current) = currentNode
	{
		if (current==editorNode)
		{
			return None;
		}
		if let Some(element) = current.dyn_ref::<web_sys::Element>()
		{
			let isTextElement = element.get_attribute("class")
				.map(|classes| classes.split_whitespace().any(|className| className=="module_todo_block_text"))
				.unwrap_or(false);
			if (isTextElement)
			{
				let textElement = element.clone().dyn_into::<HtmlElement>().ok()?;
				let blockElement = element.parent_element()?;
				let blockId = TodoBlockId::value_parse(&blockElement.get_attribute("data-todo-block-id")?)?;
				return Some((textElement,blockId));
			}
		}
		currentNode = current.parent_node();
	}
	return None;
}

#[cfg(feature="hydrate")]
fn todoEditorNodeUtf16Offset_get(element: &HtmlElement,targetNode: &Node,targetOffset: usize) -> Option<usize>
{
	let rootNode: Node = element.clone().unchecked_into();
	let mut utf16Offset = 0;
	if (!todoEditorNodeUtf16Offset_walk(&rootNode,targetNode,targetOffset,&mut utf16Offset))
	{
		return None;
	}
	return Some(utf16Offset);
}

#[cfg(feature="hydrate")]
fn todoEditorNodeUtf16Offset_walk(currentNode: &Node,targetNode: &Node,targetOffset: usize,utf16Offset: &mut usize) -> bool
{
	if (currentNode==targetNode)
	{
		if (currentNode.node_type()==Node::TEXT_NODE)
		{
			let text = currentNode.text_content().unwrap_or_default();
			*utf16Offset += targetOffset.min(text.encode_utf16().count());
			return true;
		}
		let children = currentNode.child_nodes();
		for index in 0..targetOffset.min(children.length() as usize)
		{
			if let Some(child) = children.item(index as u32)
			{
				*utf16Offset += child.text_content().unwrap_or_default().encode_utf16().count();
			}
		}
		return true;
	}

	let children = currentNode.child_nodes();
	for index in 0..children.length()
	{
		let Some(child) = children.item(index) else {continue};
		if (child==*targetNode || child.contains(Some(targetNode)))
		{
			return todoEditorNodeUtf16Offset_walk(&child,targetNode,targetOffset,utf16Offset);
		}
		*utf16Offset += child.text_content().unwrap_or_default().encode_utf16().count();
	}
	return false;
}

fn todoEditorText_get(element: &HtmlElement) -> String
{
	return element.text_content().unwrap_or_default();
}

#[cfg(any(feature="hydrate",test))]
fn todoEditorUtf16Offset_toByte(text: &str,offset: usize) -> Option<usize>
{
	let mut utf16Offset = 0;
	for (byteIndex,character) in text.char_indices()
	{
		if (utf16Offset==offset)
		{
			return Some(byteIndex);
		}
		utf16Offset += character.len_utf16();
		if (utf16Offset>offset)
		{
			return None;
		}
	}
	return (utf16Offset==offset).then_some(text.len());
}

#[cfg(feature="hydrate")]
fn todoEditorText_focusSchedule(editorId: String,elementId: String,byteIndex: usize)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(document) = web_sys::window().and_then(|window| window.document()) else {return};
		let Some(editorElement) = document.get_element_by_id(&editorId) else {return};
		let Ok(editorHtmlElement) = editorElement.dyn_into::<HtmlElement>() else {return};
		let Some(element) = document.get_element_by_id(&elementId) else {return};
		let Ok(htmlElement) = element.clone().dyn_into::<HtmlElement>() else {return};
		let _ = editorHtmlElement.focus();
		let Ok(Some(selection)) = document.get_selection() else {return};
		let elementNode: Node = element.unchecked_into();
		let text = todoEditorText_get(&htmlElement);
		let byteIndex = byteIndex.min(text.len());
		let byteIndex = (0..=byteIndex).rev().find(|index| text.is_char_boundary(*index)).unwrap_or_default();
		let utf16Offset = text[..byteIndex].encode_utf16().count() as u32;
		if let Some(textNode) = elementNode.first_child()
		{
			let _ = selection.collapse_with_offset(Some(&textNode),utf16Offset);
		}
		else
		{
			let _ = selection.collapse_with_offset(Some(&elementNode),0);
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn todoEditorText_focusSchedule(_: String,_: String,_: usize)
{
}

#[cfg(feature="hydrate")]
fn todoEditorCheckbox_focusSchedule(elementId: String)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(document) = web_sys::window().and_then(|window| window.document()) else {return};
		let Some(element) = document.get_element_by_id(&elementId) else {return};
		let Ok(htmlElement) = element.dyn_into::<HtmlElement>() else {return};
		let _ = htmlElement.focus();
	});
}

#[cfg(not(feature="hydrate"))]
fn todoEditorCheckbox_focusSchedule(_: String)
{
}

#[cfg(feature="hydrate")]
fn todoEditorLink_open(href: &str)
{
	let Some(window) = web_sys::window() else {return};
	let _ = window.open_with_url_and_target_and_features(href,"_blank","noopener,noreferrer");
}

#[cfg(not(feature="hydrate"))]
fn todoEditorLink_open(_: &str)
{
}

#[cfg(test)]
mod tests
{
	use super::{text_truncate,todoEditorUtf16Offset_toByte};

	#[test]
	fn truncation_keepsUtf8Boundary()
	{
		assert_eq!(text_truncate("éclair".to_string(),1),"");
		assert_eq!(text_truncate("éclair".to_string(),2),"é");
		assert_eq!(text_truncate("éclair".to_string(),4),"écl");
	}

	#[test]
	fn utf16Offset_mapsEmojiToRustByteBoundary()
	{
		assert_eq!(todoEditorUtf16Offset_toByte("a😀b",0),Some(0));
		assert_eq!(todoEditorUtf16Offset_toByte("a😀b",1),Some(1));
		assert_eq!(todoEditorUtf16Offset_toByte("a😀b",2),None);
		assert_eq!(todoEditorUtf16Offset_toByte("a😀b",3),Some(5));
		assert_eq!(todoEditorUtf16Offset_toByte("a😀b",4),Some(6));
	}
}
