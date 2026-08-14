use std::sync::LazyLock;

use regex::Regex;

use crate::front::utils::SafeExternalUrl;

const MAX_BLOCKS: usize = 4096;

static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"(?i)https?://[^\s<>\"']+"#).expect("the TODO URL pattern is valid")
});

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct TodoBlockId(u64);

impl TodoBlockId
{
	#[cfg(feature="hydrate")]
	pub(super) fn value_parse(value: &str) -> Option<Self>
	{
		return value.parse::<u64>().ok().map(Self);
	}

	pub(super) fn value_get(self) -> u64
	{
		return self.0;
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TodoBlockKind
{
	Paragraph,
	Heading(u8),
	ListItem,
	Task(bool),
}

impl TodoBlockKind
{
	fn markedLine_parse(line: &str,separator: char) -> Option<(Self,String)>
	{
		for (marker,kind) in [
			("###",Self::Heading(3)),
			("##",Self::Heading(2)),
			("#",Self::Heading(1)),
			("*x",Self::Task(true)),
			("*",Self::Task(false)),
			("-",Self::ListItem),
		]
		{
			let Some(text) = line.strip_prefix(marker).and_then(|text| text.strip_prefix(separator)) else {continue};
			return Some((kind,text.to_string()));
		}
		return None;
	}

	fn sourceLine_parse(line: &str) -> (Self,String)
	{
		if let Some(markedLine) = Self::markedLine_parse(line,' ')
		{
			return markedLine;
		}

		return (Self::Paragraph,line.to_string());
	}

	fn editorLine_parse(line: &str) -> Option<(Self,String)>
	{
		return Self::markedLine_parse(line,' ')
			.or_else(|| Self::markedLine_parse(line,'\u{a0}'));
	}

	fn shortcutMarker_parse(marker: &str) -> Option<Self>
	{
		return match marker {
			"###" => Some(Self::Heading(3)),
			"##" => Some(Self::Heading(2)),
			"#" => Some(Self::Heading(1)),
			"*x" => Some(Self::Task(true)),
			"*" => Some(Self::Task(false)),
			"-" => Some(Self::ListItem),
			_ => None,
		};
	}

	fn sourcePrefix_get(self) -> &'static str
	{
		return match self {
			Self::Paragraph => "",
			Self::Heading(1) => "# ",
			Self::Heading(2) => "## ",
			Self::Heading(_) => "### ",
			Self::ListItem => "- ",
			Self::Task(false) => "* ",
			Self::Task(true) => "*x ",
		};
	}

	fn nextBlock_get(self) -> Self
	{
		return match self {
			Self::ListItem => Self::ListItem,
			Self::Task(_) => Self::Task(false),
			Self::Paragraph | Self::Heading(_) => Self::Paragraph,
		};
	}

	fn emptyEnter_unstyles(self) -> bool
	{
		return matches!(self,Self::ListItem | Self::Task(_));
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TodoBlock
{
	id: TodoBlockId,
	kind: TodoBlockKind,
	text: String,
}

impl TodoBlock
{
	pub(super) fn id_get(&self) -> TodoBlockId
	{
		return self.id;
	}

	pub(super) fn kind_get(&self) -> TodoBlockKind
	{
		return self.kind;
	}

	pub(super) fn text_get(&self) -> &str
	{
		return &self.text;
	}

	pub(super) fn inlines_get(&self) -> Vec<TodoInline>
	{
		return TodoInline::parse(&self.text);
	}

	fn sourceLine_get(&self) -> String
	{
		return format!("{}{}",self.kind.sourcePrefix_get(),self.text);
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TodoInline
{
	Text(String),
	Link {
		text: String,
		href: String,
	},
}

impl TodoInline
{
	fn parse(text: &str) -> Vec<Self>
	{
		let mut result = Vec::new();
		let mut lastEnd = 0;

		for matched in URL_PATTERN.find_iter(text)
		{
			let rawCandidate = matched.as_str();
			let candidate = rawCandidate.trim_end_matches(['.',',',';',':','!','?',')',']','}']);
			if candidate.is_empty()
			{
				continue;
			}
			let Some(href) = SafeExternalUrl::parse(candidate).map(SafeExternalUrl::into_string) else {continue};
			let linkEnd = matched.start()+candidate.len();
			if (matched.start()>lastEnd)
			{
				result.push(Self::Text(text[lastEnd..matched.start()].to_string()));
			}
			result.push(Self::Link {
				text: candidate.to_string(),
				href,
			});
			lastEnd = linkEnd;
		}

		if (lastEnd<text.len())
		{
			result.push(Self::Text(text[lastEnd..].to_string()));
		}
		if (result.is_empty())
		{
			result.push(Self::Text(text.to_string()));
		}
		return result;
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TodoEditorDocument
{
	blocks: Vec<TodoBlock>,
	nextBlockId: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TodoEnterResult
{
	Inserted(TodoBlockId),
	Unstyled,
}

impl TodoEditorDocument
{
	pub(super) fn source_parse(source: &str) -> Self
	{
		let lines = source.split('\n').collect::<Vec<_>>();
		let mut blocks = Vec::with_capacity(lines.len().min(MAX_BLOCKS));
		for (index,line) in lines.iter().take(MAX_BLOCKS).enumerate()
		{
			if (index==MAX_BLOCKS-1 && lines.len()>MAX_BLOCKS)
			{
				blocks.push(TodoBlock {
					id: TodoBlockId(index as u64),
					kind: TodoBlockKind::Paragraph,
					text: lines[index..].join("\n"),
				});
				break;
			}
			let (kind,text) = TodoBlockKind::sourceLine_parse(line);
			blocks.push(TodoBlock {
				id: TodoBlockId(index as u64),
				kind,
				text,
			});
		}

		return Self {
			nextBlockId: blocks.len() as u64,
			blocks,
		};
	}

	pub(super) fn source_get(&self) -> String
	{
		return self.blocks.iter()
			.map(TodoBlock::sourceLine_get)
			.collect::<Vec<_>>()
			.join("\n");
	}

	pub(super) fn blocks_get(&self) -> &[TodoBlock]
	{
		return &self.blocks;
	}

	pub(super) fn block_get(&self, id: TodoBlockId) -> Option<&TodoBlock>
	{
		return self.blocks.iter().find(|block| block.id==id);
	}

	pub(super) fn block_position_get(&self,id: TodoBlockId) -> Option<usize>
	{
		return self.blocks.iter().position(|block| block.id==id);
	}

	pub(super) fn block_text_set(&mut self, id: TodoBlockId, text: String) -> bool
	{
		let Some(block) = self.blocks.iter_mut().find(|block| block.id==id) else {return false};
		if (block.text==text)
		{
			return false;
		}
		block.text = text;
		return true;
	}

	pub(super) fn block_task_toggle(&mut self, id: TodoBlockId) -> bool
	{
		let Some(block) = self.blocks.iter_mut().find(|block| block.id==id) else {return false};
		let TodoBlockKind::Task(checked) = block.kind else {return false};
		block.kind = TodoBlockKind::Task(!checked);
		return true;
	}

	pub(super) fn block_shortcut_apply(&mut self,id: TodoBlockId,separatorEnd: usize) -> bool
	{
		let Some(block) = self.blocks.iter_mut().find(|block| block.id==id) else {return false};
		let Some(markerText) = block.text.get(..separatorEnd) else {return false};
		let Some((kind,remainingMarkerText)) = TodoBlockKind::editorLine_parse(markerText) else {return false};
		if (!remainingMarkerText.is_empty())
		{
			return false;
		}
		let Some(text) = block.text.get(separatorEnd..).map(str::to_string) else {return false};
		block.kind = kind;
		block.text = text;
		return true;
	}

	pub(super) fn block_shortcutSpace_apply(
		&mut self,
		id: TodoBlockId,
		visibleText: &str,
		markerEnd: usize,
	) -> bool
	{
		let Some(marker) = visibleText.get(..markerEnd) else {return false};
		let Some(text) = visibleText.get(markerEnd..).map(str::to_string) else {return false};
		let Some(kind) = TodoBlockKind::shortcutMarker_parse(marker) else {return false};
		let Some(block) = self.blocks.iter_mut().find(|block| block.id==id) else {return false};
		block.kind = kind;
		block.text = text;
		return true;
	}

	pub(super) fn block_linesReplace(&mut self, id: TodoBlockId, text: &str) -> Option<(Vec<TodoBlockId>,bool)>
	{
		let index = self.blocks.iter().position(|block| block.id==id)?;
		let currentKind = self.blocks.get(index)?.kind;
		let lines = text.split('\n').collect::<Vec<_>>();
		if (self.blocks.len().saturating_sub(1).saturating_add(lines.len())>MAX_BLOCKS)
		{
			return None;
		}
		let firstLine = lines.first().copied().unwrap_or_default();
		let (firstKind,firstText) = if (currentKind==TodoBlockKind::Paragraph)
		{
			TodoBlockKind::sourceLine_parse(firstLine)
		}
		else
		{
			(currentKind,firstLine.to_string())
		};
		let structureChanged = lines.len()!=1 || firstKind!=currentKind;
		let firstBlock = self.blocks.get_mut(index)?;
		firstBlock.kind = firstKind;
		firstBlock.text = firstText;

		let mut insertIndex = index+1;
		let mut blockIds = vec![id];
		for line in lines.into_iter().skip(1)
		{
			let (kind,text) = TodoBlockKind::sourceLine_parse(line);
			let blockId = TodoBlockId(self.nextBlockId);
			self.nextBlockId = self.nextBlockId.saturating_add(1);
			self.blocks.insert(insertIndex,TodoBlock {
				id: blockId,
				kind,
				text,
			});
			insertIndex += 1;
			blockIds.push(blockId);
		}
		return Some((blockIds,structureChanged));
	}

	pub(super) fn blocks_sourceOffsetPosition(&self, blockIds: &[TodoBlockId], mut sourceOffset: usize) -> Option<(TodoBlockId,usize)>
	{
		for (index,id) in blockIds.iter().enumerate()
		{
			let block = self.block_get(*id)?;
			let prefixLength = block.kind.sourcePrefix_get().len();
			let sourceLength = prefixLength+block.text.len();
			if (sourceOffset<=sourceLength || index+1==blockIds.len())
			{
				let mut textOffset = sourceOffset.saturating_sub(prefixLength).min(block.text.len());
				while (textOffset>0 && !block.text.is_char_boundary(textOffset))
				{
					textOffset -= 1;
				}
				return Some((*id,textOffset));
			}
			sourceOffset = sourceOffset.saturating_sub(sourceLength+1);
		}
		return None;
	}

	pub(super) fn block_rangeReplace(
		&mut self,
		firstId: TodoBlockId,
		firstByte: usize,
		secondId: TodoBlockId,
		secondByte: usize,
		replacement: &str,
	) -> Option<(TodoBlockId,usize)>
	{
		let firstPosition = self.block_position_get(firstId)?;
		let secondPosition = self.block_position_get(secondId)?;
		let (startPosition,startId,startByte,endPosition,endId,endByte) = if (firstPosition<secondPosition)
		{
			(firstPosition,firstId,firstByte,secondPosition,secondId,secondByte)
		}
		else if (firstPosition==secondPosition)
		{
			(firstPosition,firstId,firstByte.min(secondByte),secondPosition,secondId,firstByte.max(secondByte))
		}
		else
		{
			(secondPosition,secondId,secondByte,firstPosition,firstId,firstByte)
		};
		let startBlock = self.block_get(startId)?;
		let endBlock = self.block_get(endId)?;
		if (!startBlock.text.is_char_boundary(startByte) || !endBlock.text.is_char_boundary(endByte))
		{
			return None;
		}

		let prefix = startBlock.text[..startByte].to_string();
		let suffix = endBlock.text[endByte..].to_string();
		let focusSourceOffset = prefix.len()+replacement.len();
		let replacementText = format!("{}{}{}",prefix,replacement,suffix);
		let replacementLineCount = replacementText.split('\n').count();
		let removedBlockCount = endPosition-startPosition;
		let resultBlockCount = self.blocks.len().saturating_sub(removedBlockCount).saturating_add(replacementLineCount.saturating_sub(1));
		if (resultBlockCount>MAX_BLOCKS)
		{
			return None;
		}
		if (endPosition>startPosition)
		{
			self.blocks.drain(startPosition+1..=endPosition);
		}
		let (blockIds,_) = self.block_linesReplace(startId,&replacementText)?;
		return self.blocks_sourceOffsetPosition(&blockIds,focusSourceOffset);
	}

	pub(super) fn block_enter(&mut self, id: TodoBlockId, byteIndex: usize) -> Option<TodoEnterResult>
	{
		let index = self.blocks.iter().position(|block| block.id==id)?;
		let atBlockLimit = self.blocks.len()>=MAX_BLOCKS;
		let block = self.blocks.get_mut(index)?;
		if (!block.text.is_char_boundary(byteIndex))
		{
			return None;
		}
		if (block.text.is_empty() && block.kind.emptyEnter_unstyles())
		{
			block.kind = TodoBlockKind::Paragraph;
			return Some(TodoEnterResult::Unstyled);
		}
		if (atBlockLimit)
		{
			return None;
		}

		let rightText = block.text.split_off(byteIndex);
		let nextKind = block.kind.nextBlock_get();
		let nextId = TodoBlockId(self.nextBlockId);
		self.nextBlockId = self.nextBlockId.saturating_add(1);
		self.blocks.insert(index+1,TodoBlock {
			id: nextId,
			kind: nextKind,
			text: rightText,
		});
		return Some(TodoEnterResult::Inserted(nextId));
	}

	pub(super) fn block_enterRange(&mut self, id: TodoBlockId, byteStart: usize, byteEnd: usize) -> Option<TodoEnterResult>
	{
		let index = self.blocks.iter().position(|block| block.id==id)?;
		let atBlockLimit = self.blocks.len()>=MAX_BLOCKS;
		let block = self.blocks.get_mut(index)?;
		if (byteStart>byteEnd || !block.text.is_char_boundary(byteStart) || !block.text.is_char_boundary(byteEnd))
		{
			return None;
		}
		let willBeEmpty = block.text.len()==byteEnd-byteStart;
		if (atBlockLimit && !(willBeEmpty && block.kind.emptyEnter_unstyles()))
		{
			return None;
		}
		block.text.replace_range(byteStart..byteEnd,"");
		return self.block_enter(id,byteStart);
	}

	pub(super) fn block_mergePrevious(&mut self, id: TodoBlockId) -> Option<(TodoBlockId,usize)>
	{
		let index = self.blocks.iter().position(|block| block.id==id)?;
		if (index==0)
		{
			return None;
		}
		let block = self.blocks.remove(index);
		let previous = self.blocks.get_mut(index-1)?;
		let caretIndex = previous.text.len();
		previous.text.push_str(&block.text);
		return Some((previous.id,caretIndex));
	}

	pub(super) fn block_mergeNext(&mut self, id: TodoBlockId) -> Option<usize>
	{
		let index = self.blocks.iter().position(|block| block.id==id)?;
		if (index+1>=self.blocks.len())
		{
			return None;
		}
		let next = self.blocks.remove(index+1);
		let block = self.blocks.get_mut(index)?;
		let caretIndex = block.text.len();
		block.text.push_str(&next.text);
		return Some(caretIndex);
	}

	pub(super) fn block_unstyle(&mut self, id: TodoBlockId) -> bool
	{
		let Some(block) = self.blocks.iter_mut().find(|block| block.id==id) else {return false};
		if (block.kind==TodoBlockKind::Paragraph)
		{
			return false;
		}
		block.kind = TodoBlockKind::Paragraph;
		return true;
	}
}

#[cfg(test)]
mod tests
{
	use super::{MAX_BLOCKS,TodoBlockKind,TodoEditorDocument,TodoEnterResult,TodoInline};

	#[test]
	fn sourceRoundTrip_preservesHistoricalTextAndTrailingLines()
	{
		let source = "plain\n# title\n## subtitle\n### small\n- item\n* pending\n*x done\n\nmarker * inside\n";
		let document = TodoEditorDocument::source_parse(source);

		assert_eq!(document.source_get(),source);
		assert_eq!(document.blocks_get()[1].kind_get(),TodoBlockKind::Heading(1));
		assert_eq!(document.blocks_get()[4].kind_get(),TodoBlockKind::ListItem);
		assert_eq!(document.blocks_get()[5].kind_get(),TodoBlockKind::Task(false));
		assert_eq!(document.blocks_get()[6].kind_get(),TodoBlockKind::Task(true));
	}

	#[test]
	fn incompleteOrIndentedMarkers_remainPlainText()
	{
		let source = "#\n##\n###\n-\n*\n*x\n * item\n#### heading";
		let document = TodoEditorDocument::source_parse(source);

		assert!(document.blocks_get().iter().all(|block| block.kind_get()==TodoBlockKind::Paragraph));
		assert_eq!(document.source_get(),source);
	}

	#[test]
	fn taskToggle_targetsStableBlockAmongDuplicateLabels()
	{
		let mut document = TodoEditorDocument::source_parse("* same\n* same");
		let secondId = document.blocks_get()[1].id_get();

		assert!(document.block_task_toggle(secondId));
		assert_eq!(document.source_get(),"* same\n*x same");
		assert!(!document.block_task_toggle(super::TodoBlockId(99)));
	}

	#[test]
	fn shortcutTransformation_removesOnlyLeadingMarker()
	{
		let cases = [
			("# ",TodoBlockKind::Heading(1),""),
			("## title",TodoBlockKind::Heading(2),"title"),
			("### title",TodoBlockKind::Heading(3),"title"),
			("- item",TodoBlockKind::ListItem,"item"),
			("* task",TodoBlockKind::Task(false),"task"),
			("*x task",TodoBlockKind::Task(true),"task"),
		];

		for (source,expectedKind,expectedText) in cases
		{
			let mut document = TodoEditorDocument::source_parse("plain");
			let id = document.blocks_get()[0].id_get();
			document.block_text_set(id,source.to_string());
			let separatorEnd = source.find(' ').unwrap()+1;
			assert!(document.block_shortcut_apply(id,separatorEnd),"shortcut was not applied for {source:?}");
			assert_eq!(document.block_get(id).unwrap().kind_get(),expectedKind);
			assert_eq!(document.block_get(id).unwrap().text_get(),expectedText);
		}
	}

	#[test]
	fn browserShortcut_acceptsNonBreakingSpaceWithoutChangingStoredLegacyText()
	{
		let legacySource = "#\u{a0}legacy";
		let legacyDocument = TodoEditorDocument::source_parse(legacySource);
		assert_eq!(legacyDocument.blocks_get()[0].kind_get(),TodoBlockKind::Paragraph);
		assert_eq!(legacyDocument.source_get(),legacySource);

		let mut document = TodoEditorDocument::source_parse("plain");
		let id = document.blocks_get()[0].id_get();
		document.block_text_set(id,"#\u{a0}".to_string());
		assert!(document.block_shortcut_apply(id,"#\u{a0}".len()));
		assert_eq!(document.block_get(id).unwrap().kind_get(),TodoBlockKind::Heading(1));
		assert_eq!(document.source_get(),"# ");
	}

	#[test]
	fn spaceKeyShortcut_replacesEveryCurrentBlockKindAndPreservesText()
	{
		for (currentSource,marker,expectedKind) in [
			("* stale","-",TodoBlockKind::ListItem),
			("- stale","*",TodoBlockKind::Task(false)),
			("# stale","##",TodoBlockKind::Heading(2)),
			("## stale","###",TodoBlockKind::Heading(3)),
			("### stale","#",TodoBlockKind::Heading(1)),
			("plain","*x",TodoBlockKind::Task(true)),
		]
		{
			let mut document = TodoEditorDocument::source_parse(currentSource);
			let id = document.blocks_get()[0].id_get();
			let visibleText = format!("{marker}kept text");
			assert!(
				document.block_shortcutSpace_apply(id,&visibleText,marker.len()),
				"space shortcut was not applied for {marker:?}",
			);
			assert_eq!(document.block_get(id).unwrap().kind_get(),expectedKind);
			assert_eq!(document.block_get(id).unwrap().text_get(),"kept text");
		}
	}

	#[test]
	fn shortcut_requiresCaretImmediatelyAfterLeadingMarker()
	{
		let mut document = TodoEditorDocument::source_parse("* task");
		let id = document.blocks_get()[0].id_get();
		document.block_text_set(id,"- item with space".to_string());

		assert!(!document.block_shortcut_apply(id,"- item ".len()));
		assert!(!document.block_shortcutSpace_apply(id,"task - marker","task -".len()));
		assert_eq!(document.block_get(id).unwrap().kind_get(),TodoBlockKind::Task(false));
	}

	#[test]
	fn enterContinuesListsAndLeavesEmptyTask()
	{
		let mut document = TodoEditorDocument::source_parse("- first\n* ");
		let listId = document.blocks_get()[0].id_get();
		let taskId = document.blocks_get()[1].id_get();

		let Some(TodoEnterResult::Inserted(newId)) = document.block_enter(listId,5) else {panic!("list Enter must insert")};
		assert_eq!(document.block_get(newId).unwrap().kind_get(),TodoBlockKind::ListItem);
		assert_eq!(document.block_get(newId).unwrap().text_get(),"");
		assert_eq!(document.block_enter(taskId,0),Some(TodoEnterResult::Unstyled));
		assert_eq!(document.block_get(taskId).unwrap().kind_get(),TodoBlockKind::Paragraph);
	}

	#[test]
	fn enterRejectsInvalidUtf8Boundary()
	{
		let mut document = TodoEditorDocument::source_parse("éclair");
		let id = document.blocks_get()[0].id_get();

		assert_eq!(document.block_enter(id,1),None);
		assert_eq!(document.source_get(),"éclair");
	}

	#[test]
	fn excessiveLines_arePreservedInsideBoundedDocument()
	{
		let source = (0..MAX_BLOCKS+20)
			.map(|index| format!("# line-{index}"))
			.collect::<Vec<_>>()
			.join("\n");
		let document = TodoEditorDocument::source_parse(&source);

		assert_eq!(document.blocks_get().len(),MAX_BLOCKS);
		assert_eq!(document.source_get(),source);
	}

	#[test]
	fn urls_areDetectedWithoutConsumingTrailingPunctuation()
	{
		let document = TodoEditorDocument::source_parse("See https://Example.com/path, then http://example.org/test.");
		let inlines = document.blocks_get()[0].inlines_get();

		assert_eq!(inlines,[
			TodoInline::Text("See ".to_string()),
			TodoInline::Link {text: "https://Example.com/path".to_string(),href: "https://example.com/path".to_string()},
			TodoInline::Text(", then ".to_string()),
			TodoInline::Link {text: "http://example.org/test".to_string(),href: "http://example.org/test".to_string()},
			TodoInline::Text(".".to_string()),
		]);
	}

	#[test]
	fn unsupportedUrls_remainText()
	{
		let document = TodoEditorDocument::source_parse("javascript:alert(1) example.com /relative");
		assert_eq!(document.blocks_get()[0].inlines_get(),[
			TodoInline::Text("javascript:alert(1) example.com /relative".to_string()),
		]);
	}

	#[test]
	fn unstyle_preservesBlockText()
	{
		let mut document = TodoEditorDocument::source_parse("### title");
		let id = document.blocks_get()[0].id_get();

		assert!(document.block_unstyle(id));
		assert_eq!(document.source_get(),"title");
		assert!(!document.block_unstyle(id));
	}

	#[test]
	fn rangeEnter_replacesSelectionAndSplitsBlock()
	{
		let mut document = TodoEditorDocument::source_parse("abcdef");
		let id = document.blocks_get()[0].id_get();

		let Some(TodoEnterResult::Inserted(nextId)) = document.block_enterRange(id,2,4) else {panic!("range Enter must insert")};
		assert_eq!(document.block_get(id).unwrap().text_get(),"ab");
		assert_eq!(document.block_get(nextId).unwrap().text_get(),"ef");
		assert_eq!(document.source_get(),"ab\nef");
	}

	#[test]
	fn boundaryMerges_preserveTheReceivingBlockKind()
	{
		let mut document = TodoEditorDocument::source_parse("- first\n# second\nthird");
		let secondId = document.blocks_get()[1].id_get();
		let thirdId = document.blocks_get()[2].id_get();

		let Some((firstId,caret)) = document.block_mergePrevious(secondId) else {panic!("Backspace merge must work")};
		assert_eq!(caret,5);
		assert_eq!(document.block_get(firstId).unwrap().kind_get(),TodoBlockKind::ListItem);
		assert_eq!(document.source_get(),"- firstsecond\nthird");
		assert_eq!(document.block_mergeNext(firstId),Some(11));
		assert!(document.block_get(thirdId).is_none());
		assert_eq!(document.source_get(),"- firstsecondthird");
	}

	#[test]
	fn multilineReplacement_parsesPastedShortcutsAndKeepsStableFirstId()
	{
		let mut document = TodoEditorDocument::source_parse("before\n- after");
		let firstId = document.blocks_get()[0].id_get();

		let (blockIds,structureChanged) = document.block_linesReplace(firstId,"# title\n* task\nhttps://example.com").unwrap();
		let lastId = *blockIds.last().unwrap();
		assert!(structureChanged);
		assert_eq!(document.block_get(firstId).unwrap().kind_get(),TodoBlockKind::Heading(1));
		assert_eq!(document.block_get(lastId).unwrap().text_get(),"https://example.com");
		assert_eq!(document.source_get(),"# title\n* task\nhttps://example.com\n- after");
	}

	#[test]
	fn ordinaryLines_remainAdjacentParagraphRows()
	{
		let source = "first line\nsecond line\n\nlast line";
		let document = TodoEditorDocument::source_parse(source);

		assert_eq!(document.blocks_get().len(),4);
		assert!(document.blocks_get().iter().all(|block| block.kind_get()==TodoBlockKind::Paragraph));
		assert_eq!(document.source_get(),source);
	}

	#[test]
	fn multilineShortcut_keepsCaretOnTheTransformedLine()
	{
		let mut document = TodoEditorDocument::source_parse("first\nsecond");
		let firstId = document.blocks_get()[0].id_get();

		let (blockIds,structureChanged) = document.block_linesReplace(firstId,"first\n* task\nlast").unwrap();
		assert!(structureChanged);
		assert_eq!(blockIds.len(),3);
		assert_eq!(document.blocks_sourceOffsetPosition(&blockIds,"first\n* ".len()),Some((blockIds[1],0)));
		assert_eq!(document.source_get(),"first\n* task\nlast\nsecond");
	}

	#[test]
	fn multilinePlainEdit_createsAdjacentRowsInOneDocument()
	{
		let mut document = TodoEditorDocument::source_parse("before");
		let firstId = document.blocks_get()[0].id_get();

		let (blockIds,structureChanged) = document.block_linesReplace(firstId,"first\nsecond").unwrap();
		assert!(structureChanged);
		assert_eq!(blockIds.len(),2);
		assert_eq!(blockIds[0],firstId);
		assert_eq!(document.blocks_get().len(),2);
	}

	#[test]
	fn rangeReplacement_acrossRowsKeepsOneNaturalCaretTarget()
	{
		let mut document = TodoEditorDocument::source_parse("first\n- middle\nlast");
		let firstId = document.blocks_get()[0].id_get();
		let lastId = document.blocks_get()[2].id_get();

		let (focusId,focusByte) = document.block_rangeReplace(firstId,2,lastId,2,"X").unwrap();
		assert_eq!(focusId,firstId);
		assert_eq!(focusByte,3);
		assert_eq!(document.source_get(),"fiXst");
	}

	#[test]
	fn structuralMutations_respectTheBlockLimitWithoutPartialEdit()
	{
		let source = (0..MAX_BLOCKS)
			.map(|index| format!("# line-{index}"))
			.collect::<Vec<_>>()
			.join("\n");
		let mut document = TodoEditorDocument::source_parse(&source);
		let firstId = document.blocks_get()[0].id_get();

		assert_eq!(document.block_enterRange(firstId,0,4),None);
		assert_eq!(document.block_linesReplace(firstId,"first\nsecond"),None);
		assert_eq!(document.source_get(),source);
	}
}
