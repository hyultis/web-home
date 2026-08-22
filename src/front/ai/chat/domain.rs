use serde::{Deserialize,Serialize};
use std::collections::HashSet;
use time::OffsetDateTime;
use crate::global_security::hash;

pub(super) const CHAT_CONVERSATIONS_MAXIMUM: usize = 32;
pub(super) const CHAT_MESSAGES_MAXIMUM: usize = 100;
pub(super) const CHAT_CONTEXT_MESSAGES_MAXIMUM: usize = 64;
pub(super) const CHAT_MESSAGE_MAXIMUM_BYTES: usize = 64 * 1024;
pub(super) const CHAT_DOCUMENT_MAXIMUM_BYTES: usize = 1024 * 1024;
const CHAT_CONTEXT_MAXIMUM_BYTES: usize = 256 * 1024;
const CHAT_CONVERSATION_TITLE_MAXIMUM_BYTES: usize = 256;
const CHAT_ID_MAXIMUM_BYTES: usize = 128;
const CHAT_RETENTION_SECONDS: i64 = 5 * 24 * 60 * 60;
const CHAT_DOCUMENT_VERSION: u8 = 1;

#[derive(Clone,Copy,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(super) enum ChatMessageRole
{
	User,
	Assistant,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(super) struct ChatMessage
{
	pub(super) id: String,
	pub(super) role: Option<ChatMessageRole>,
	pub(super) content: String,
	pub(super) createdAt: i64,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(super) struct ChatConversation
{
	pub(super) id: String,
	pub(super) title: String,
	pub(super) createdAt: i64,
	pub(super) lastActivityAt: i64,
	pub(super) favorite: bool,
	pub(super) messages: Vec<ChatMessage>,
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct ChatDocument
{
	version: u8,
	pub(super) conversations: Vec<ChatConversation>,
}

impl Default for ChatDocument
{
	fn default() -> Self
	{
		return Self {
			version: CHAT_DOCUMENT_VERSION,
			conversations: Vec::new(),
		};
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum ChatError
{
	InvalidDocument,
	UnsupportedVersion,
	DocumentTooLarge,
	ConversationLimit,
	ConversationNotFound,
	InvalidTitle,
	MessageLimit,
	InvalidMessage,
	AwaitingAssistant,
	NoPendingMessage,
}

impl ChatError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::InvalidDocument | Self::UnsupportedVersion => "MODULE_CHAT_ERROR_DOCUMENT_INVALID",
			Self::DocumentTooLarge => "MODULE_CHAT_ERROR_DOCUMENT_TOO_LARGE",
			Self::ConversationLimit => "MODULE_CHAT_ERROR_CONVERSATION_LIMIT",
			Self::ConversationNotFound => "MODULE_CHAT_ERROR_CONVERSATION_MISSING",
			Self::InvalidTitle => "MODULE_CHAT_ERROR_TITLE_INVALID",
			Self::MessageLimit => "MODULE_CHAT_ERROR_MESSAGE_LIMIT",
			Self::InvalidMessage => "MODULE_CHAT_ERROR_MESSAGE_INVALID",
			Self::AwaitingAssistant => "MODULE_CHAT_ERROR_RETRY_REQUIRED",
			Self::NoPendingMessage => "MODULE_CHAT_ERROR_RETRY_UNAVAILABLE",
		};
	}
}

pub(super) struct ChatCompletionContext
{
	pub(super) messages: Vec<ChatMessage>,
	pub(super) truncated: bool,
}

impl ChatDocument
{
	pub(super) fn now_get() -> i64
	{
		return OffsetDateTime::now_utc().unix_timestamp();
	}

	pub(super) fn deserialize(content: &str,now: i64) -> Result<(Self,bool),ChatError>
	{
		if (content.len() > CHAT_DOCUMENT_MAXIMUM_BYTES)
		{
			return Err(ChatError::DocumentTooLarge);
		}
		let mut document = serde_json::from_str::<Self>(content).map_err(|_| ChatError::InvalidDocument)?;
		document.validate()?;
		let purged = document.expired_purge(now);
		document.validate()?;
		return Ok((document,purged));
	}

	pub(super) fn serialize(&self) -> Result<String,ChatError>
	{
		self.validate()?;
		return serde_json::to_string(self).map_err(|_| ChatError::InvalidDocument);
	}

	pub(super) fn validate(&self) -> Result<(),ChatError>
	{
		if (self.version != CHAT_DOCUMENT_VERSION)
		{
			return Err(ChatError::UnsupportedVersion);
		}
		if (self.conversations.len() > CHAT_CONVERSATIONS_MAXIMUM)
		{
			return Err(ChatError::ConversationLimit);
		}
		let mut conversationIds = HashSet::new();
		for conversation in &self.conversations
		{
			if (!id_isValid(&conversation.id) || !conversationIds.insert(conversation.id.as_str()))
			{
				return Err(ChatError::InvalidDocument);
			}
			if (!title_isValid(&conversation.title))
			{
				return Err(ChatError::InvalidTitle);
			}
			if (conversation.createdAt <= 0 || conversation.lastActivityAt < conversation.createdAt)
			{
				return Err(ChatError::InvalidDocument);
			}
			if (conversation.messages.len() > CHAT_MESSAGES_MAXIMUM)
			{
				return Err(ChatError::MessageLimit);
			}
			if (conversation.messages.first().and_then(|message| message.role) == Some(ChatMessageRole::Assistant))
			{
				return Err(ChatError::InvalidMessage);
			}
			let mut messageIds = HashSet::new();
			let mut previousRole = None;
			let mut previousTimestamp = conversation.createdAt;
			for message in &conversation.messages
			{
				if (!id_isValid(&message.id) || !messageIds.insert(message.id.as_str()))
				{
					return Err(ChatError::InvalidDocument);
				}
				let Some(role) = message.role else {return Err(ChatError::InvalidDocument);};
				if (previousRole == Some(role)
					|| !message_isValid(&message.content)
					|| message.createdAt < previousTimestamp
					|| message.createdAt > conversation.lastActivityAt)
				{
					return Err(ChatError::InvalidMessage);
				}
				previousRole = Some(role);
				previousTimestamp = message.createdAt;
			}
		}
		let content = serde_json::to_string(self).map_err(|_| ChatError::InvalidDocument)?;
		if (content.len() > CHAT_DOCUMENT_MAXIMUM_BYTES)
		{
			return Err(ChatError::DocumentTooLarge);
		}
		return Ok(());
	}

	pub(super) fn expired_purge(&mut self,now: i64) -> bool
	{
		let before = self.conversations.len();
		let cutoff = now.saturating_sub(CHAT_RETENTION_SECONDS);
		self.conversations.retain(|conversation| conversation.favorite || conversation.lastActivityAt > cutoff);
		return self.conversations.len() != before;
	}

	pub(super) fn conversation_create(&mut self,now: i64) -> Result<String,ChatError>
	{
		let mut updated = self.clone();
		updated.expired_purge(now);
		if (updated.conversations.len() >= CHAT_CONVERSATIONS_MAXIMUM)
		{
			return Err(ChatError::ConversationLimit);
		}
		let id = uuid::Uuid::new_v4().to_string();
		updated.conversations.push(ChatConversation {
			id: id.clone(),
			title: String::new(),
			createdAt: now,
			lastActivityAt: now,
			favorite: false,
			messages: Vec::new(),
		});
		updated.validate()?;
		*self = updated;
		return Ok(id);
	}

	pub(super) fn conversation_rename(&mut self,id: &str,title: &str,now: i64) -> Result<(),ChatError>
	{
		let title = title.trim();
		if (!title_isValid(title))
		{
			return Err(ChatError::InvalidTitle);
		}
		let mut updated = self.clone();
		updated.expired_purge(now);
		let conversation = updated.conversations.iter_mut()
			.find(|conversation| conversation.id == id)
			.ok_or(ChatError::ConversationNotFound)?;
		conversation.title = title.to_string();
		updated.validate()?;
		*self = updated;
		return Ok(());
	}

	pub(super) fn conversation_favoriteToggle(&mut self,id: &str,now: i64) -> Result<bool,ChatError>
	{
		let mut updated = self.clone();
		updated.expired_purge(now);
		let conversation = updated.conversations.iter_mut()
			.find(|conversation| conversation.id == id)
			.ok_or(ChatError::ConversationNotFound)?;
		conversation.favorite = !conversation.favorite;
		let favorite = conversation.favorite;
		updated.validate()?;
		*self = updated;
		return Ok(favorite);
	}

	pub(super) fn conversation_remove(&mut self,id: &str,now: i64) -> Result<(),ChatError>
	{
		let mut updated = self.clone();
		updated.expired_purge(now);
		let before = updated.conversations.len();
		updated.conversations.retain(|conversation| conversation.id != id);
		if (updated.conversations.len() == before)
		{
			return Err(ChatError::ConversationNotFound);
		}
		updated.validate()?;
		*self = updated;
		return Ok(());
	}

	pub(super) fn userMessage_add(&mut self,id: &str,content: String,now: i64) -> Result<(),ChatError>
	{
		if (!message_isValid(&content))
		{
			return Err(ChatError::InvalidMessage);
		}
		let mut updated = self.clone();
		updated.expired_purge(now);
		let conversation = updated.conversations.iter_mut()
			.find(|conversation| conversation.id == id)
			.ok_or(ChatError::ConversationNotFound)?;
		if (conversation.messages.last().and_then(|message| message.role) == Some(ChatMessageRole::User))
		{
			return Err(ChatError::AwaitingAssistant);
		}
		if (conversation.messages.len().saturating_add(2) > CHAT_MESSAGES_MAXIMUM)
		{
			return Err(ChatError::MessageLimit);
		}
		if (conversation.messages.is_empty() && conversation.title.is_empty())
		{
			conversation.title = conversationTitle_fromMessage(&content);
		}
		let messageTimestamp = now.max(conversation.lastActivityAt);
		conversation.messages.push(ChatMessage {
			id: uuid::Uuid::new_v4().to_string(),
			role: Some(ChatMessageRole::User),
			content,
			createdAt: messageTimestamp,
		});
		conversation.lastActivityAt = messageTimestamp;
		updated.validate()?;
		*self = updated;
		return Ok(());
	}

	pub(super) fn assistantMessage_add(&mut self,id: &str,content: String,now: i64) -> Result<(),ChatError>
	{
		if (!message_isValid(&content))
		{
			return Err(ChatError::InvalidMessage);
		}
		let mut updated = self.clone();
		updated.expired_purge(now);
		let conversation = updated.conversations.iter_mut()
			.find(|conversation| conversation.id == id)
			.ok_or(ChatError::ConversationNotFound)?;
		if (conversation.messages.last().and_then(|message| message.role) != Some(ChatMessageRole::User))
		{
			return Err(ChatError::NoPendingMessage);
		}
		if (conversation.messages.len().saturating_add(1) > CHAT_MESSAGES_MAXIMUM)
		{
			return Err(ChatError::MessageLimit);
		}
		conversation.messages.push(ChatMessage {
			id: uuid::Uuid::new_v4().to_string(),
			role: Some(ChatMessageRole::Assistant),
			content,
			createdAt: now.max(conversation.lastActivityAt),
		});
		conversation.lastActivityAt = now.max(conversation.lastActivityAt);
		updated.validate()?;
		*self = updated;
		return Ok(());
	}

	pub(super) fn completionContext_get(&self,id: &str) -> Result<ChatCompletionContext,ChatError>
	{
		let conversation = self.conversations.iter()
			.find(|conversation| conversation.id == id)
			.ok_or(ChatError::ConversationNotFound)?;
		if (conversation.messages.last().and_then(|message| message.role) != Some(ChatMessageRole::User))
		{
			return Err(ChatError::NoPendingMessage);
		}
		let mut selected = Vec::new();
		let mut totalBytes = 0usize;
		for message in conversation.messages.iter().rev()
		{
			if (selected.len() == CHAT_CONTEXT_MESSAGES_MAXIMUM
				|| totalBytes.saturating_add(message.content.len()) > CHAT_CONTEXT_MAXIMUM_BYTES)
			{
				break;
			}
			totalBytes += message.content.len();
			selected.push(message.clone());
		}
		selected.reverse();
		return Ok(ChatCompletionContext {
			truncated: selected.len() < conversation.messages.len(),
			messages: selected,
		});
	}

	pub(super) fn legacy_merge(&mut self,sourceId: &str,legacy: &Self) -> Result<bool,ChatError>
	{
		let mut updated = self.clone();
		let mut changed = false;
		for legacyConversation in &legacy.conversations
		{
			if let Some(existingIndex) = updated.conversations.iter()
				.position(|conversation| conversation.id == legacyConversation.id)
			{
				if let Some(conversationChanged) = conversation_compatibleMerge(
					&mut updated.conversations[existingIndex],
					legacyConversation,
				)
				{
					changed |= conversationChanged;
					continue;
				}
			}

			let legacyBaseId = legacyConversationId_baseGet(sourceId,&legacyConversation.id);
			let compatibleIndex = updated.conversations.iter()
				.position(|conversation| legacyConversationId_matches(&conversation.id,&legacyBaseId)
					&& conversation_canMerge(conversation,legacyConversation));
			if let Some(compatibleIndex) = compatibleIndex
			{
				if let Some(conversationChanged) = conversation_compatibleMerge(
					&mut updated.conversations[compatibleIndex],
					legacyConversation,
				)
				{
					changed |= conversationChanged;
					continue;
				}
			}

			let mut preserved = legacyConversation.clone();
			preserved.id = legacyConversationId_get(&updated,&legacyBaseId);
			updated.conversations.push(preserved);
			changed = true;
		}
		updated.validate()?;
		if (changed)
		{
			*self = updated;
		}
		return Ok(changed);
	}

	pub(super) fn selectedFallback_get(&self) -> Option<String>
	{
		return self.conversations.iter()
			.max_by(|left,right| left.lastActivityAt.cmp(&right.lastActivityAt).then_with(|| left.id.cmp(&right.id)))
			.map(|conversation| conversation.id.clone());
	}

	pub(super) fn conversation_get(&self,id: &str) -> Option<&ChatConversation>
	{
		return self.conversations.iter().find(|conversation| conversation.id == id);
	}
}

fn id_isValid(value: &str) -> bool
{
	return !value.is_empty()
		&& value.len() <= CHAT_ID_MAXIMUM_BYTES
		&& value.trim() == value
		&& !value.chars().any(char::is_control);
}

fn title_isValid(value: &str) -> bool
{
	return value.len() <= CHAT_CONVERSATION_TITLE_MAXIMUM_BYTES
		&& !value.chars().any(char::is_control);
}

fn message_isValid(value: &str) -> bool
{
	return !value.trim().is_empty()
		&& value.len() <= CHAT_MESSAGE_MAXIMUM_BYTES
		&& !value.contains('\0');
}

fn conversation_contains(candidate: &ChatConversation,prefix: &ChatConversation) -> bool
{
	return candidate.createdAt == prefix.createdAt
		&& candidate.messages.len() >= prefix.messages.len()
		&& candidate.messages[..prefix.messages.len()] == prefix.messages;
}

fn conversation_canMerge(existing: &ChatConversation,incoming: &ChatConversation) -> bool
{
	return conversation_contains(existing,incoming) || conversation_contains(incoming,existing);
}

fn conversation_compatibleMerge(existing: &mut ChatConversation,incoming: &ChatConversation) -> Option<bool>
{
	if (!conversation_canMerge(existing,incoming))
	{
		return None;
	}
	let original = existing.clone();
	if (conversation_contains(incoming,existing) && incoming.messages.len() > existing.messages.len())
	{
		let existingId = existing.id.clone();
		let favorite = existing.favorite || incoming.favorite;
		let title = if incoming.title.is_empty() {existing.title.clone()} else {incoming.title.clone()};
		*existing = incoming.clone();
		existing.id = existingId;
		existing.favorite = favorite;
		existing.title = title;
	}
	else
	{
		existing.favorite |= incoming.favorite;
		if (existing.title.is_empty() && !incoming.title.is_empty())
		{
			existing.title = incoming.title.clone();
		}
	}
	return Some(*existing != original);
}

fn legacyConversationId_baseGet(sourceId: &str,conversationId: &str) -> String
{
	return format!("legacy-{}",hash(format!("{sourceId}\0{conversationId}")));
}

fn legacyConversationId_matches(candidate: &str,base: &str) -> bool
{
	if (candidate == base)
	{
		return true;
	}
	return candidate.strip_prefix(&format!("{base}-"))
		.and_then(|suffix| suffix.parse::<usize>().ok())
		.is_some_and(|suffix| suffix >= 2);
}

fn legacyConversationId_get(document: &ChatDocument,base: &str) -> String
{
	if (!document.conversations.iter().any(|conversation| conversation.id == base))
	{
		return base.to_string();
	}
	for suffix in 2usize..
	{
		let candidate = format!("{base}-{suffix}");
		if (!document.conversations.iter().any(|conversation| conversation.id == candidate))
		{
			return candidate;
		}
	}
	unreachable!();
}

fn conversationTitle_fromMessage(content: &str) -> String
{
	let singleLine = content.lines().next().unwrap_or_default().trim();
	let mut title = String::new();
	for character in singleLine.chars()
	{
		if (title.len() + character.len_utf8() > CHAT_CONVERSATION_TITLE_MAXIMUM_BYTES || title.chars().count() >= 64)
		{
			break;
		}
		title.push(character);
	}
	return title;
}

#[cfg(test)]
mod tests
{
	use super::*;

	const NOW: i64 = 2_000_000;

	fn conversationWithMessage_get(document: &mut ChatDocument) -> String
	{
		let id = document.conversation_create(NOW).unwrap();
		document.userMessage_add(&id,"hello".to_string(),NOW + 1).unwrap();
		document.assistantMessage_add(&id,"world".to_string(),NOW + 2).unwrap();
		return id;
	}

	#[test]
	fn legacyDocumentWithoutVersionUsesCurrentSchema()
	{
		let (document,purged) = ChatDocument::deserialize(r#"{"conversations":[]}"#,NOW).unwrap();
		assert!(!purged);
		assert_eq!(document.serialize().unwrap(),r#"{"version":1,"conversations":[]}"#);
	}

	#[test]
	fn ordinaryConversationExpiresExactlyAfterFiveDaysButFavoriteDoesNot()
	{
		let mut document = ChatDocument::default();
		let expired = conversationWithMessage_get(&mut document);
		let favorite = document.conversation_create(NOW + 3).unwrap();
		document.conversation_favoriteToggle(&favorite,NOW + 3).unwrap();

		assert!(!document.expired_purge(NOW + CHAT_RETENTION_SECONDS - 1));
		assert!(document.expired_purge(NOW + 2 + CHAT_RETENTION_SECONDS));
		assert!(document.conversation_get(&expired).is_none());
		assert!(document.conversation_get(&favorite).is_some());
	}

	#[test]
	fn conversationLimitNeverDeletesValidOrFavoriteContent()
	{
		let mut document = ChatDocument::default();
		for offset in 0..CHAT_CONVERSATIONS_MAXIMUM
		{
			document.conversation_create(NOW + offset as i64).unwrap();
		}
		assert_eq!(document.conversation_create(NOW + CHAT_CONVERSATIONS_MAXIMUM as i64),Err(ChatError::ConversationLimit));
		assert_eq!(document.conversations.len(),CHAT_CONVERSATIONS_MAXIMUM);
	}

	#[test]
	fn failedTurnMustBeRetriedBeforeAddingAnotherUserMessage()
	{
		let mut document = ChatDocument::default();
		let id = document.conversation_create(NOW).unwrap();
		document.userMessage_add(&id,"first line\nsecond line".to_string(),NOW + 1).unwrap();
		assert_eq!(document.conversation_get(&id).unwrap().title,"first line");
		assert_eq!(document.userMessage_add(&id,"duplicate".to_string(),NOW + 2),Err(ChatError::AwaitingAssistant));
		document.assistantMessage_add(&id,"answer".to_string(),NOW + 2).unwrap();
		assert!(document.userMessage_add(&id,"next".to_string(),NOW + 3).is_ok());
	}

	#[test]
	fn completionContextKeepsNewestSixtyFourMessagesInOrder()
	{
		let mut document = ChatDocument::default();
		let id = document.conversation_create(NOW).unwrap();
		for turn in 0..40
		{
			document.userMessage_add(&id,format!("user-{turn}"),NOW + turn * 2 + 1).unwrap();
			document.assistantMessage_add(&id,format!("assistant-{turn}"),NOW + turn * 2 + 2).unwrap();
		}
		document.userMessage_add(&id,"latest".to_string(),NOW + 100).unwrap();

		let context = document.completionContext_get(&id).unwrap();
		assert!(context.truncated);
		assert_eq!(context.messages.len(),CHAT_CONTEXT_MESSAGES_MAXIMUM);
		assert_eq!(context.messages.last().unwrap().content,"latest");
		assert_eq!(context.messages.last().unwrap().role,Some(ChatMessageRole::User));
	}

	#[test]
	fn oversizedMessageAndDocumentAreRejectedWithoutMutation()
	{
		let mut document = ChatDocument::default();
		let id = document.conversation_create(NOW).unwrap();
		let before = document.clone();
		assert_eq!(
			document.userMessage_add(&id,"x".repeat(CHAT_MESSAGE_MAXIMUM_BYTES + 1),NOW + 1),
			Err(ChatError::InvalidMessage),
		);
		assert_eq!(document,before);
		assert_eq!(
			ChatDocument::deserialize(&"x".repeat(CHAT_DOCUMENT_MAXIMUM_BYTES + 1),NOW),
			Err(ChatError::DocumentTooLarge),
		);
	}

	#[test]
	fn renameFavoriteAndDeleteUseStableConversationId()
	{
		let mut document = ChatDocument::default();
		let id = document.conversation_create(NOW).unwrap();
		document.conversation_rename(&id,"  Important  ",NOW).unwrap();
		assert_eq!(document.conversation_get(&id).unwrap().title,"Important");
		assert!(document.conversation_favoriteToggle(&id,NOW).unwrap());
		document.conversation_remove(&id,NOW).unwrap();
		assert!(document.conversations.is_empty());
	}

	#[test]
	fn legacyMergeIsIdempotentAndPreservesDivergentConversations()
	{
		let mut current = ChatDocument::default();
		let sharedId = conversationWithMessage_get(&mut current);
		let mut legacy = current.clone();
		legacy.conversation_get(&sharedId).unwrap();
		legacy.conversations[0].messages[1].content = "legacy answer".to_string();

		assert!(current.legacy_merge("legacy-module",&legacy).unwrap());
		assert_eq!(current.conversations.len(),2);
		let merged = current.clone();
		assert!(!current.legacy_merge("legacy-module",&legacy).unwrap());
		assert_eq!(current,merged);
	}

	#[test]
	fn legacyMergeDoesNotDuplicateAnOlderPrefix()
	{
		let mut current = ChatDocument::default();
		let id = conversationWithMessage_get(&mut current);
		let legacy = current.clone();
		current.userMessage_add(&id,"new question".to_string(),NOW + 3).unwrap();
		current.assistantMessage_add(&id,"new answer".to_string(),NOW + 4).unwrap();

		assert!(!current.legacy_merge("legacy-module",&legacy).unwrap());
		assert_eq!(current.conversations.len(),1);
		assert_eq!(current.conversation_get(&id).unwrap().messages.len(),4);
	}
}
