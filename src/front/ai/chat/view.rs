use super::domain::{
	ChatCompletionContext,ChatConversation,ChatDocument,ChatError,ChatMessageRole,
	CHAT_MESSAGE_MAXIMUM_BYTES,
};
use super::{ChatFeedback,ChatFeedbackKind,ChatRuntime};
use crate::front::ai::{AiAllowedOrigins,AiProfile};
use crate::front::ai::provider::{
	AiCompletionRequest,AiMessage,AiMessageRole,AiProviderClient,
};
#[cfg(test)]
use crate::front::ai::provider::AiTransportError;
use crate::front::modules::module_holder::{ModuleHolder,ModuleHolderEpoch};
use crate::front::utils::translate::TranslateText;
use crate::HWebTrace;
use leptos::ev::{KeyboardEvent,SubmitEvent};
use leptos::html::{Button,Div,Input,Textarea};
use leptos::prelude::{
	ArcRwSignal,AriaAttributes,BindAttribute,ClassAttribute,CollectView,Effect,ElementChild,Get,
	GetUntracked,GlobalAttributes,IntoAny,NodeRef,NodeRefAttribute,OnAttribute,RwSignal,Set,Update,
	WithUntracked,use_context,
};
use leptos::{component,view,IntoView};
#[cfg(feature="hydrate")]
use wasm_bindgen::JsCast;

#[component]
pub(in crate::front::ai) fn AiChatView(
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
) -> impl IntoView
{
	let Some(allowedOrigins) = use_context::<AiAllowedOrigins>()
	else
	{
		HWebTrace!("cannot get allowedOrigins in AI chat");
		return view!{}.into_any();
	};

	let textareaRef = NodeRef::<Textarea>::new();
	let messagesRef = NodeRef::<Div>::new();
	let renameConversationId = RwSignal::new(None::<String>);
	let renameTitle = RwSignal::new(String::new());
	let renameError = RwSignal::new(None::<&'static str>);
	let deleteConversationId = RwSignal::new(None::<String>);

	let scrollDocument = document.clone();
	let scrollRuntime = runtime.clone();
	Effect::new(move |_| {
		let runtime = scrollRuntime.get();
		let selected = runtime.selectedConversationId;
		let pending = runtime.pending.map(|pending| pending.generation);
		let lastMessage = selected.as_deref().and_then(|conversationId| {
			scrollDocument.get().conversation_get(conversationId)
				.and_then(|conversation| conversation.messages.last())
				.map(|message| message.id.clone())
		});
		let _ = (selected,pending,lastMessage);
		messageList_scrollSchedule(messagesRef);
	});

	let createDocument = document.clone();
	let createRuntime = runtime.clone();
	let createTextareaRef = textareaRef;
	let conversationCreate = move |_| {
		let result = createDocument.try_update(|document| document.conversation_create(ChatDocument::now_get()))
			.unwrap_or(Err(ChatError::InvalidDocument));
		match result
		{
			Ok(conversationId) => {
				createRuntime.update(|runtime| {
					runtime.selectedConversationId = Some(conversationId);
					runtime.feedback = None;
				});
				chatDocument_changed(lifecycleEpoch);
				textarea_focus(createTextareaRef);
			},
			Err(error) => chatFeedback_set(&createRuntime,None,error.translateKey_get(),ChatFeedbackKind::Error),
		}
	};

	let sendDocument = document.clone();
	let sendRuntime = runtime.clone();
	let sendAllowedOrigins = allowedOrigins.clone();
	let sendTextareaRef = textareaRef;
	let messageSubmit = move |event: SubmitEvent| {
		event.prevent_default();
		chatMessage_send(
			sendDocument.clone(),sendRuntime.clone(),lifecycleEpoch,
			sendAllowedOrigins.clone(),sendTextareaRef,
		);
	};
	let keyboardDocument = document.clone();
	let keyboardRuntime = runtime.clone();
	let keyboardAllowedOrigins = allowedOrigins.clone();
	let keyboardTextareaRef = textareaRef;
	let messageKeyboard = move |event: KeyboardEvent| {
		if (event.key() != "Enter" || event.shift_key() || event.is_composing())
		{
			return;
		}
		event.prevent_default();
		chatMessage_send(
			keyboardDocument.clone(),keyboardRuntime.clone(),lifecycleEpoch,
			keyboardAllowedOrigins.clone(),keyboardTextareaRef,
		);
	};

	let retryDocument = document.clone();
	let retryRuntime = runtime.clone();
	let retryAllowedOrigins = allowedOrigins.clone();
	let requestRetry = move |_| chatRequest_retry(
		retryDocument.clone(),retryRuntime.clone(),lifecycleEpoch,retryAllowedOrigins.clone(),
	);
	let cancelRuntime = runtime.clone();
	let requestCancel = move |_| {
		let wasPending = cancelRuntime.get_untracked().pending.is_some();
		cancelRuntime.update(|runtime| runtime.request_cancel());
		if (wasPending)
		{
			chatDocument_changed(lifecycleEpoch);
		}
	};

	let listDocument = document.clone();
	let listRuntime = runtime.clone();
	let selectedDocument = document.clone();
	let selectedRuntime = runtime.clone();
	let messagesDocument = document.clone();
	let messagesRuntime = runtime.clone();
	let feedbackRuntime = runtime.clone();
	let truncatedRuntime = runtime.clone();
	let textareaDocument = document.clone();
	let textareaRuntime = runtime.clone();
	let actionRuntime = runtime.clone();
	let retryVisibleDocument = document.clone();
	let retryVisibleRuntime = runtime.clone();
	let submitDocument = document.clone();
	let submitRuntime = runtime.clone();
	let renameOverlayDocument = document.clone();
	let renameOverlayRuntime = runtime.clone();
	let deleteOverlayDocument = document.clone();
	let deleteOverlayRuntime = runtime.clone();

	view! {
		<div class="ai_chat">
			<div class="ai_chat_titlebar">
				<h2><TranslateText key="MODULE_CHAT_CONVERSATIONS"/></h2>
				<button id="ai-chat-new-conversation-action" type="button" class="ai_chat_new" on:click=conversationCreate>
					<i class="iconoir-plus" aria-hidden="true"></i>
					<span><TranslateText key="MODULE_CHAT_NEW_CONVERSATION"/></span>
				</button>
			</div>
			<div class="ai_chat_layout">
				<aside class="ai_chat_sidebar" aria-labelledby="ai-chat-conversations-title">
					<h3 id="ai-chat-conversations-title" class="visually_hidden"><TranslateText key="MODULE_CHAT_CONVERSATIONS"/></h3>
					{move || chatConversationLists_view(
						&listDocument.get(),listRuntime.clone(),listDocument.clone(),lifecycleEpoch,textareaRef,
					)}
				</aside>
				<section class="ai_chat_main">
					{move || chatSelectedToolbar_view(
						&selectedDocument.get(),selectedRuntime.clone(),selectedDocument.clone(),lifecycleEpoch,
						renameConversationId,renameTitle,renameError,deleteConversationId,
					)}
					<div class="ai_chat_messages" node_ref=messagesRef aria-live="polite">
						{move || chatMessages_view(&messagesDocument.get(),&messagesRuntime.get())}
					</div>
					<div class="ai_chat_statuses" aria-live="polite">
						{move || chatFeedback_view(&feedbackRuntime.get())}
						{move || chatContextTruncated_view(&truncatedRuntime.get())}
					</div>
					<form class="ai_chat_composer" on:submit=messageSubmit>
						<label class="visually_hidden" for="ai-chat-message">
							<TranslateText key="MODULE_CHAT_MESSAGE"/>
						</label>
						<textarea
							id="ai-chat-message"
							node_ref=textareaRef
							rows="3"
							maxlength={CHAT_MESSAGE_MAXIMUM_BYTES}
							disabled=move || chatComposer_isDisabled(&textareaDocument.get(),&textareaRuntime.get())
							on:keydown=messageKeyboard
						></textarea>
						<div class="ai_chat_composer_actions">
							{move || if actionRuntime.get().pending.is_some()
							{
								view! {
									<button type="button" class="secondary danger" on:click=requestCancel.clone()>
										<TranslateText key="MODULE_CHAT_CANCEL"/>
									</button>
								}.into_any()
							}
							else if chatRetry_isVisible(&retryVisibleDocument.get(),&retryVisibleRuntime.get())
							{
								view! {
									<button type="button" class="secondary" on:click=requestRetry.clone()>
										<TranslateText key="MODULE_CHAT_RETRY"/>
									</button>
								}.into_any()
							}
							else
							{
								view!{}.into_any()
							}}
							<button type="submit" disabled=move || chatComposer_isDisabled(&submitDocument.get(),&submitRuntime.get())>
								<TranslateText key="MODULE_CHAT_SEND"/>
							</button>
						</div>
					</form>
				</section>
			</div>
			{move || renameConversationId.get().map(|conversationId| view! {
				<AiChatRenameOverlay
					conversationId
					title=renameTitle
					error=renameError
					document=renameOverlayDocument.clone()
					runtime=renameOverlayRuntime.clone()
					lifecycleEpoch
					onClose=renameConversationId
				/>
			})}
			{move || deleteConversationId.get().map(|conversationId| view! {
				<AiChatDeleteOverlay
					conversationId
					document=deleteOverlayDocument.clone()
					runtime=deleteOverlayRuntime.clone()
					lifecycleEpoch
					onClose=deleteConversationId
				/>
			})}
		</div>
	}.into_any()
}

fn chatConversationLists_view(
	document: &ChatDocument,
	runtime: ArcRwSignal<ChatRuntime>,
	documentSignal: ArcRwSignal<ChatDocument>,
	lifecycleEpoch: ModuleHolderEpoch,
	textareaRef: NodeRef<Textarea>,
) -> leptos::prelude::AnyView
{
	if (document.conversations.is_empty())
	{
		return view! {
			<p class="ai_chat_sidebar_empty"><TranslateText key="MODULE_CHAT_NO_CONVERSATIONS"/></p>
		}.into_any();
	}
	let mut favorites = document.conversations.iter().filter(|conversation| conversation.favorite).cloned().collect::<Vec<_>>();
	let mut recent = document.conversations.iter().filter(|conversation| !conversation.favorite).cloned().collect::<Vec<_>>();
	conversationSort(&mut favorites);
	conversationSort(&mut recent);
	let favoriteSection = (!favorites.is_empty()).then(|| view! {
		<div class="ai_chat_conversation_section">
			<h3><TranslateText key="MODULE_CHAT_FAVORITES"/></h3>
			{chatConversationItems_view(favorites,runtime.clone(),documentSignal.clone(),lifecycleEpoch,textareaRef)}
		</div>
	});
	let recentSection = (!recent.is_empty()).then(|| view! {
		<div class="ai_chat_conversation_section">
			<h3><TranslateText key="MODULE_CHAT_RECENT"/></h3>
			{chatConversationItems_view(recent,runtime,documentSignal,lifecycleEpoch,textareaRef)}
		</div>
	});
	return view! {{favoriteSection}{recentSection}}.into_any();
}

fn chatConversationItems_view(
	conversations: Vec<ChatConversation>,
	runtime: ArcRwSignal<ChatRuntime>,
	document: ArcRwSignal<ChatDocument>,
	lifecycleEpoch: ModuleHolderEpoch,
	textareaRef: NodeRef<Textarea>,
) -> leptos::prelude::AnyView
{
	return view! {
		<div class="ai_chat_conversation_list">
			{conversations.into_iter().map(|conversation| {
				let conversationId = conversation.id.clone();
				let selectId = conversationId.clone();
				let selectedClassRuntime = runtime.clone();
				let selectRuntime = runtime.clone();
				let selectTextareaRef = textareaRef;
				let favoriteId = conversationId.clone();
				let favoriteDocument = document.clone();
				let favoriteRuntime = runtime.clone();
				let favorite = conversation.favorite;
				let title = conversation.title.clone();
				view! {
					<div class="ai_chat_conversation_item" class:selected=move || {
						selectedClassRuntime.get().selectedConversationId.as_deref() == Some(conversationId.as_str())
					}>
						<button type="button" class="ai_chat_conversation_select" on:click=move |_| {
							selectRuntime.update(|runtime| runtime.selectedConversationId = Some(selectId.clone()));
							textarea_focus(selectTextareaRef);
						}>
							<span>{if (title.is_empty())
							{
								view!{<TranslateText key="MODULE_CHAT_UNTITLED_CONVERSATION"/>}.into_any()
							}
							else
							{
								view!{<span>{title}</span>}.into_any()
							}}</span>
						</button>
						<button type="button" class="ai_chat_conversation_favorite" on:click=move |_| {
							let result = favoriteDocument.try_update(|document| {
								document.conversation_favoriteToggle(&favoriteId,ChatDocument::now_get())
							}).unwrap_or(Err(ChatError::InvalidDocument));
							match result
							{
								Ok(_) => chatDocument_changed(lifecycleEpoch),
								Err(error) => chatFeedback_set(&favoriteRuntime,Some(favoriteId.clone()),error.translateKey_get(),ChatFeedbackKind::Error),
							}
						}>
							<i class={if (favorite) {"iconoir-star-solid"} else {"iconoir-star"}} aria-hidden="true"></i>
							<span class="visually_hidden"><TranslateText key={if (favorite) {"MODULE_CHAT_UNFAVORITE"} else {"MODULE_CHAT_FAVORITE"}}/></span>
						</button>
					</div>
				}
			}).collect_view()}
		</div>
	}.into_any();
}

fn chatSelectedToolbar_view(
	document: &ChatDocument,
	runtime: ArcRwSignal<ChatRuntime>,
	documentSignal: ArcRwSignal<ChatDocument>,
	lifecycleEpoch: ModuleHolderEpoch,
	renameConversationId: RwSignal<Option<String>>,
	renameTitle: RwSignal<String>,
	renameError: RwSignal<Option<&'static str>>,
	deleteConversationId: RwSignal<Option<String>>,
) -> leptos::prelude::AnyView
{
	let Some(conversationId) = runtime.get_untracked().selectedConversationId else
	{
		return view! {
			<div class="ai_chat_conversation_toolbar ai_chat_conversation_toolbar--empty">
				<TranslateText key="MODULE_CHAT_SELECT_OR_CREATE"/>
			</div>
		}.into_any();
	};
	let Some(conversation) = document.conversation_get(&conversationId) else {return view!{}.into_any();};
	let title = conversation.title.clone();
	let renameId = conversationId.clone();
	let renameCurrentTitle = title.clone();
	let favoriteDocument = documentSignal.clone();
	let favoriteRuntime = runtime.clone();
	let favoriteId = conversationId.clone();
	let removeId = conversationId.clone();
	let favorite = conversation.favorite;

	return view! {
		<div class="ai_chat_conversation_toolbar">
			<h3>{if (title.is_empty())
			{
				view!{<TranslateText key="MODULE_CHAT_UNTITLED_CONVERSATION"/>}.into_any()
			}
			else
			{
				view!{<span>{title}</span>}.into_any()
			}}</h3>
			<div class="ai_chat_conversation_actions">
				<button id="ai-chat-rename-action" type="button" on:click=move |_| {
					renameTitle.set(renameCurrentTitle.clone());
					renameError.set(None);
					renameConversationId.set(Some(renameId.clone()));
				}>
					<i class="iconoir-edit-pencil" aria-hidden="true"></i>
					<span class="visually_hidden"><TranslateText key="MODULE_CHAT_RENAME"/></span>
				</button>
				<button type="button" on:click=move |_| {
					let result = favoriteDocument.try_update(|document| {
						document.conversation_favoriteToggle(&favoriteId,ChatDocument::now_get())
					}).unwrap_or(Err(ChatError::InvalidDocument));
					match result
					{
						Ok(_) => chatDocument_changed(lifecycleEpoch),
						Err(error) => chatFeedback_set(&favoriteRuntime,Some(favoriteId.clone()),error.translateKey_get(),ChatFeedbackKind::Error),
					}
				}>
					<i class={if (favorite) {"iconoir-star-solid"} else {"iconoir-star"}} aria-hidden="true"></i>
					<span class="visually_hidden"><TranslateText key={if (favorite) {"MODULE_CHAT_UNFAVORITE"} else {"MODULE_CHAT_FAVORITE"}}/></span>
				</button>
				<button id="ai-chat-delete-action" type="button" class="danger" on:click=move |_| deleteConversationId.set(Some(removeId.clone()))>
					<i class="iconoir-trash" aria-hidden="true"></i>
					<span class="visually_hidden"><TranslateText key="MODULE_CHAT_DELETE"/></span>
				</button>
			</div>
		</div>
	}.into_any();
}

#[component]
fn AiChatRenameOverlay(
	conversationId: String,
	title: RwSignal<String>,
	error: RwSignal<Option<&'static str>>,
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
	onClose: RwSignal<Option<String>>,
) -> impl IntoView
{
	let inputRef = NodeRef::<Input>::new();
	Effect::new(move |_| input_focus(inputRef));
	let close = move || {
		error.set(None);
		onClose.set(None);
		element_focusById("ai-chat-rename-action");
	};
	let cancel = move |_| close();
	let backdropCancel = move |_| close();
	let keyboardCancel = move |event: KeyboardEvent| {
		if (event.key() == "Escape")
		{
			event.prevent_default();
			event.stop_propagation();
			close();
		}
	};
	let submitDocument = document.clone();
	let submitRuntime = runtime.clone();
	let submit = move |event: SubmitEvent| {
		event.prevent_default();
		let result = submitDocument.try_update(|document| {
			document.conversation_rename(&conversationId,&title.get_untracked(),ChatDocument::now_get())
		}).unwrap_or(Err(ChatError::InvalidDocument));
		match result
		{
			Ok(()) => {
				chatDocument_changed(lifecycleEpoch);
				close();
			},
			Err(chatError) => {
				error.set(Some(chatError.translateKey_get()));
				chatFeedback_set(&submitRuntime,Some(conversationId.clone()),chatError.translateKey_get(),ChatFeedbackKind::Error);
			},
		}
	};

	view! {
		<div class="ai_chat_inner_backdrop" on:click=backdropCancel on:keydown=keyboardCancel>
			<form class="ai_chat_inner_dialog" role="dialog" aria-modal="true" aria-labelledby="ai-chat-rename-title" on:click=|event| event.stop_propagation() on:submit=submit>
				<h3 id="ai-chat-rename-title"><TranslateText key="MODULE_CHAT_RENAME"/></h3>
				<label class="module_config_field">
					<span><TranslateText key="MODULE_CHAT_CONVERSATION_TITLE"/></span>
					<input node_ref=inputRef type="text" maxlength="256" bind:value=title/>
				</label>
				{move || error.get().map(|key| view! {
					<p class="ai_chat_dialog_error" role="alert"><TranslateText key={key}/></p>
				})}
				<div class="ai_chat_inner_actions">
					<button type="button" class="secondary" on:click=cancel><TranslateText key="FRONTUI_OPTIONS_CANCEL"/></button>
					<button type="submit"><TranslateText key="MODULE_CHAT_RENAME_ACTION"/></button>
				</div>
			</form>
		</div>
	}
}

#[component]
fn AiChatDeleteOverlay(
	conversationId: String,
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
	onClose: RwSignal<Option<String>>,
) -> impl IntoView
{
	let cancelRef = NodeRef::<Button>::new();
	Effect::new(move |_| button_focus(cancelRef));
	let cancel = move |_| deleteOverlay_close(onClose,"ai-chat-delete-action");
	let backdropCancel = move |_| deleteOverlay_close(onClose,"ai-chat-delete-action");
	let keyboardCancel = move |event: KeyboardEvent| {
		if (event.key() == "Escape")
		{
			event.prevent_default();
			event.stop_propagation();
			deleteOverlay_close(onClose,"ai-chat-delete-action");
		}
	};
	let removeDocument = document.clone();
	let removeRuntime = runtime.clone();
	let remove = move |_| {
		let result = removeDocument.try_update(|document| {
			document.conversation_remove(&conversationId,ChatDocument::now_get())
		}).unwrap_or(Err(ChatError::InvalidDocument));
		match result
		{
			Ok(()) => {
				let current = removeDocument.get_untracked();
				removeRuntime.update(|runtime| runtime.conversation_removed(&current,&conversationId));
				chatDocument_changed(lifecycleEpoch);
				deleteOverlay_close(onClose,"ai-chat-new-conversation-action");
				return;
			},
			Err(error) => chatFeedback_set(&removeRuntime,Some(conversationId.clone()),error.translateKey_get(),ChatFeedbackKind::Error),
		}
		deleteOverlay_close(onClose,"ai-chat-delete-action");
	};

	view! {
		<div class="ai_chat_inner_backdrop" on:click=backdropCancel on:keydown=keyboardCancel>
			<div class="ai_chat_inner_dialog" role="alertdialog" aria-modal="true" aria-labelledby="ai-chat-delete-title" on:click=|event| event.stop_propagation()>
				<h3 id="ai-chat-delete-title"><TranslateText key="MODULE_CHAT_DELETE_CONFIRM_TITLE"/></h3>
				<p><TranslateText key="MODULE_CHAT_DELETE_CONFIRM"/></p>
				<div class="ai_chat_inner_actions">
					<button node_ref=cancelRef type="button" class="secondary" on:click=cancel><TranslateText key="FRONTUI_OPTIONS_CANCEL"/></button>
					<button type="button" class="danger" on:click=remove><TranslateText key="MODULE_CHAT_DELETE_ACTION"/></button>
				</div>
			</div>
		</div>
	}
}

fn deleteOverlay_close(onClose: RwSignal<Option<String>>,focusId: &'static str)
{
	onClose.set(None);
	element_focusById(focusId);
}

fn chatMessages_view(document: &ChatDocument,runtime: &ChatRuntime) -> leptos::prelude::AnyView
{
	let Some(conversationId) = runtime.selectedConversationId.as_deref() else
	{
		return view! {
			<div class="module_empty_state"><TranslateText key="MODULE_CHAT_EMPTY_STATE"/></div>
		}.into_any();
	};
	let Some(conversation) = document.conversation_get(conversationId) else {return view!{}.into_any();};
	let messages = conversation.messages.iter().cloned().map(|message| {
		let role = message.role.unwrap_or(ChatMessageRole::Assistant);
		let roleClass = match role
		{
			ChatMessageRole::User => "ai_chat_message ai_chat_message--user",
			ChatMessageRole::Assistant => "ai_chat_message ai_chat_message--assistant",
		};
		let roleKey = match role
		{
			ChatMessageRole::User => "MODULE_CHAT_ROLE_USER",
			ChatMessageRole::Assistant => "MODULE_CHAT_ROLE_ASSISTANT",
		};
		view! {
			<article class={roleClass}>
				<span class="ai_chat_message_role"><TranslateText key={roleKey}/></span>
				<p>{message.content}</p>
			</article>
		}
	}).collect_view();
	let pending = runtime.pending.as_ref()
		.filter(|pending| pending.conversationId == conversationId)
		.map(|_| view! {
			<div class="ai_chat_message ai_chat_message--assistant ai_chat_message--pending" role="status">
				<span class="ai_chat_message_role"><TranslateText key="MODULE_CHAT_ROLE_ASSISTANT"/></span>
				<p><TranslateText key="MODULE_CHAT_WAITING"/></p>
			</div>
		});
	if (conversation.messages.is_empty() && pending.is_none())
	{
		return view! {
			<div class="module_empty_state"><TranslateText key="MODULE_CHAT_CONVERSATION_EMPTY"/></div>
		}.into_any();
	}
	return view! {{messages}{pending}}.into_any();
}

fn chatFeedback_view(runtime: &ChatRuntime) -> leptos::prelude::AnyView
{
	let Some(feedback) = &runtime.feedback else {return view!{}.into_any();};
	if (feedback.conversationId.is_some()
		&& feedback.conversationId.as_deref() != runtime.selectedConversationId.as_deref())
	{
		return view!{}.into_any();
	}
	let class = match feedback.kind
	{
		ChatFeedbackKind::Warning => "ai_chat_status ai_chat_status--warning",
		ChatFeedbackKind::Error => "ai_chat_status ai_chat_status--error",
	};
	return view! {
		<p class={class} role={if (feedback.kind == ChatFeedbackKind::Error) {"alert"} else {"status"}}>
			<TranslateText key={feedback.key}/>
		</p>
	}.into_any();
}

fn chatContextTruncated_view(runtime: &ChatRuntime) -> leptos::prelude::AnyView
{
	let Some(conversationId) = runtime.contextTruncatedFor.as_deref() else {return view!{}.into_any();};
	if (Some(conversationId) != runtime.selectedConversationId.as_deref())
	{
		return view!{}.into_any();
	}
	return view! {
		<p class="ai_chat_status"><TranslateText key="MODULE_CHAT_CONTEXT_TRUNCATED"/></p>
	}.into_any();
}

fn chatComposer_isDisabled(document: &ChatDocument,runtime: &ChatRuntime) -> bool
{
	let Some(conversationId) = runtime.selectedConversationId.as_deref() else {return true;};
	let Some(conversation) = document.conversation_get(conversationId) else {return true;};
	return runtime.pending.is_some()
		|| conversation.messages.last().and_then(|message| message.role) == Some(ChatMessageRole::User);
}

fn chatRetry_isVisible(document: &ChatDocument,runtime: &ChatRuntime) -> bool
{
	if (runtime.pending.is_some()) {return false;}
	let Some(conversationId) = runtime.selectedConversationId.as_deref() else {return false;};
	return document.conversation_get(conversationId)
		.and_then(|conversation| conversation.messages.last())
		.and_then(|message| message.role) == Some(ChatMessageRole::User);
}

fn chatMessage_send(
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
	allowedOrigins: AiAllowedOrigins,
	textareaRef: NodeRef<Textarea>,
)
{
	if (runtime.get_untracked().pending.is_some()) {return;}
	let Some(profile) = chatProfile_get(&runtime) else {return;};
	let Some(conversationId) = runtime.get_untracked().selectedConversationId else {return;};
	let Some(textarea) = textareaRef.get() else {return;};
	let content = textarea.value();
	let result = document.try_update(|document| {
		document.userMessage_add(&conversationId,content,ChatDocument::now_get())
	}).unwrap_or(Err(ChatError::InvalidDocument));
	if let Err(error) = result
	{
		chatFeedback_set(&runtime,Some(conversationId),error.translateKey_get(),ChatFeedbackKind::Error);
		return;
	}
	textarea.set_value("");
	chatRequest_start(document,runtime,lifecycleEpoch,allowedOrigins,profile,conversationId);
}

fn chatRequest_retry(
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
	allowedOrigins: AiAllowedOrigins,
)
{
	if (runtime.get_untracked().pending.is_some()) {return;}
	let Some(profile) = chatProfile_get(&runtime) else {return;};
	let Some(conversationId) = runtime.get_untracked().selectedConversationId else {return;};
	chatRequest_start(document,runtime,lifecycleEpoch,allowedOrigins,profile,conversationId);
}

fn chatProfile_get(runtime: &ArcRwSignal<ChatRuntime>) -> Option<AiProfile>
{
	if (!ModuleHolder::aiConfig_isReady())
	{
		chatFeedback_set(runtime,None,"MODULE_CHAT_AI_LOADING",ChatFeedbackKind::Warning);
		return None;
	}
	let Some(profile) = ModuleHolder::aiConfig_get().profile else
	{
		chatFeedback_set(runtime,None,"MODULE_CHAT_AI_REQUIRED",ChatFeedbackKind::Warning);
		return None;
	};
	return Some(profile);
}

fn chatRequest_start(
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	lifecycleEpoch: ModuleHolderEpoch,
	allowedOrigins: AiAllowedOrigins,
	profile: AiProfile,
	conversationId: String,
)
{
	let context = document.with_untracked(|document| document.completionContext_get(&conversationId));
	let context = match context
	{
		Ok(context) => context,
		Err(error) => {
			chatFeedback_set(&runtime,Some(conversationId),error.translateKey_get(),ChatFeedbackKind::Error);
			chatDocument_changed(lifecycleEpoch);
			return;
		},
	};
	let request = completionRequest_get(context,profile.maxOutputTokens);
	let generation = runtime.try_update(|runtime| runtime.request_start(conversationId.clone(),request.1))
		.unwrap_or(0);
	if (generation == 0)
	{
		chatDocument_changed(lifecycleEpoch);
		return;
	}
	let request = request.0;
	let taskDocument = document.clone();
	let taskRuntime = runtime.clone();
	ModuleHolder::task_spawn(lifecycleEpoch,async move {
		let result = AiProviderClient::complete(&profile,&request,&allowedOrigins).await;
		if (!ModuleHolder::lifecycle_isActive(lifecycleEpoch)
			|| !taskRuntime.get_untracked().request_isCurrent(&conversationId,generation))
		{
			return;
		}
		match result
		{
			Ok(response) => {
				let addResult = taskDocument.try_update(|document| {
					document.assistantMessage_add(&conversationId,response.text,ChatDocument::now_get())
				}).unwrap_or(Err(ChatError::InvalidDocument));
				match addResult
				{
					Ok(()) => taskRuntime.update(|runtime| runtime.request_success(&conversationId,generation)),
					Err(error) => taskRuntime.update(|runtime| runtime.request_error(
						&conversationId,generation,chatAssistantError_key(error),
					)),
				}
			},
			Err(error) => taskRuntime.update(|runtime| runtime.request_error(
				&conversationId,generation,error.chatTranslateKey_get(),
			)),
		}
		chatDocument_changed(lifecycleEpoch);
	});
}

fn completionRequest_get(context: ChatCompletionContext,maxOutputTokens: u32) -> (AiCompletionRequest,bool)
{
	let truncated = context.truncated;
	let messages = context.messages.into_iter().filter_map(|message| {
		let role = match message.role?
		{
			ChatMessageRole::User => AiMessageRole::User,
			ChatMessageRole::Assistant => AiMessageRole::Assistant,
		};
		return Some(AiMessage {role,content: message.content});
	}).collect();
	return (AiCompletionRequest {messages,maxOutputTokens,responseJsonSchema: None},truncated);
}

fn chatAssistantError_key(error: ChatError) -> &'static str
{
	return match error
	{
		ChatError::InvalidMessage => "MODULE_CHAT_ERROR_RESPONSE_TOO_LARGE",
		_ => error.translateKey_get(),
	};
}

fn chatFeedback_set(
	runtime: &ArcRwSignal<ChatRuntime>,
	conversationId: Option<String>,
	key: &'static str,
	kind: ChatFeedbackKind,
)
{
	runtime.update(|runtime| runtime.feedback = Some(ChatFeedback {conversationId,key,kind}));
}

fn chatDocument_changed(lifecycleEpoch: ModuleHolderEpoch)
{
	ModuleHolder::aiChat_changed(lifecycleEpoch);
}

fn conversationSort(conversations: &mut [ChatConversation])
{
	conversations.sort_by(|left,right| {
		right.lastActivityAt.cmp(&left.lastActivityAt).then_with(|| left.id.cmp(&right.id))
	});
}

#[cfg(feature="hydrate")]
fn textarea_focus(textareaRef: NodeRef<Textarea>)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		if let Some(textarea) = textareaRef.try_get_untracked().flatten()
		{
			let _ = textarea.focus();
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn textarea_focus(_textareaRef: NodeRef<Textarea>)
{
}

#[cfg(feature="hydrate")]
fn input_focus(inputRef: NodeRef<Input>)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		if let Some(input) = inputRef.try_get_untracked().flatten()
		{
			let _ = input.focus();
			input.select();
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn input_focus(_inputRef: NodeRef<Input>)
{
}

#[cfg(feature="hydrate")]
fn button_focus(buttonRef: NodeRef<Button>)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		if let Some(button) = buttonRef.try_get_untracked().flatten()
		{
			let _ = button.focus();
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn button_focus(_buttonRef: NodeRef<Button>)
{
}

#[cfg(feature="hydrate")]
fn element_focusById(id: &'static str)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(element) = web_sys::window()
			.and_then(|window| window.document())
			.and_then(|document| document.get_element_by_id(id))
		else {return};
		if let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
		{
			let _ = element.focus();
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn element_focusById(_: &'static str)
{
}

#[cfg(feature="hydrate")]
fn messageList_scrollSchedule(messagesRef: NodeRef<Div>)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		if let Some(messages) = messagesRef.try_get_untracked().flatten()
		{
			messages.set_scroll_top(messages.scroll_height());
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn messageList_scrollSchedule(_messagesRef: NodeRef<Div>)
{
}

#[cfg(test)]
mod tests
{
	use super::*;
	use super::super::domain::CHAT_CONTEXT_MESSAGES_MAXIMUM;
	use leptos::prelude::Owner;

	#[test]
	fn disposedNodeRefCanBeIgnoredByDeferredCallbacks()
	{
		let owner = Owner::new();
		let messagesRef = owner.with(NodeRef::<Div>::new);
		owner.cleanup();

		assert!(messagesRef.try_get_untracked().is_none());
	}

	#[test]
	fn normalizedCompletionPreservesRolesAndTruncationFlag()
	{
		let context = ChatCompletionContext {
			messages: vec![
				super::super::domain::ChatMessage {
					id: "a".to_string(),
					role: Some(ChatMessageRole::User),
					content: "hello".to_string(),
					createdAt: 1,
				},
				super::super::domain::ChatMessage {
					id: "b".to_string(),
					role: Some(ChatMessageRole::Assistant),
					content: "world".to_string(),
					createdAt: 2,
				},
			],
			truncated: true,
		};
		let (request,truncated) = completionRequest_get(context,512);
		assert!(truncated);
		assert_eq!(request.messages.len(),2);
		assert_eq!(request.messages[0].role,AiMessageRole::User);
		assert_eq!(request.messages[1].role,AiMessageRole::Assistant);
		assert!(request.messages.len() <= CHAT_CONTEXT_MESSAGES_MAXIMUM);
	}

	#[test]
	fn transportErrorsUseChatSpecificMessages()
	{
		assert_eq!(AiTransportError::Timeout.chatTranslateKey_get(),"MODULE_CHAT_ERROR_TIMEOUT");
		assert_eq!(AiTransportError::Busy.chatTranslateKey_get(),"MODULE_CHAT_ERROR_BUSY");
		assert_eq!(AiTransportError::Unauthorized.chatTranslateKey_get(),"MODULE_CHAT_ERROR_UNAUTHORIZED");
	}
}
