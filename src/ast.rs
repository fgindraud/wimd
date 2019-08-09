use pulldown_cmark::{CowStr, Event, Parser, Tag};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

/******************************************************************************
 * Ast definition.
 *
 * All blocks use the lifetime of the parsed text, as it uses Cow to avoid copies.
 * Identified keywords are added to a set separate from the ast during parsing.
 * The variant of supported markdown is CommonMark.
 * All elements of the AST are in order of appearance in the original document.
 *
 * The supported subset of markdown is:
 * - headers (section titles), cutting text into a tree structure
 * - paragraphs
 * - horizontal rule
 * - lists (recursive, ordered or not, specific)
 * - strong tags in any inline: non-semantic highlighting, conserved in output
 * - emphasis tags in any inline: indicate a keyword, removed from output
 *
 * Other elements are deemed not useful for RPG notes for now.
 * Using them will generate a fatal parsing error.
 *
 * Links are not used for keyword definition as they have complex cases to handle.
 */

/// Root of a markdown document. Equivalent to a level-0 section with no title.
#[derive(Debug)]
pub struct Document<'s> {
    pub blocks: Vec<BlockElement<'s>>,
    pub sections: Vec<Section<'s>>,
}

#[derive(Debug)]
pub enum BlockElement<'s> {
    Paragraph(Vec<InlineElement<'s>>),
    Rule,
    List(List<'s>),
}

#[derive(Debug)]
pub struct List<'s> {
    pub ordered: bool,
    pub items: Vec<ListItem<'s>>,
}

#[derive(Debug)]
pub struct ListItem<'s> {
    /// Possibly multiline text. Must be non empty.
    pub text_content: Vec<InlineElement<'s>>,
    pub sub_list: Option<List<'s>>,
}

#[derive(Debug)]
pub struct Section<'s> {
    pub title: InlineElement<'s>,
    pub blocks: Vec<BlockElement<'s>>,
    pub sub_sections: Vec<Section<'s>>,
}

#[derive(Debug)]
pub struct InlineElement<'s> {
    /// Raw string content without any formatting
    pub string: Cow<'s, str>,
    /// List of ranges where a strong marker applies (in order, no overlap)
    pub strong_parts: Vec<Range<usize>>,
}

/******************************************************************************
 * Parsing.
 */

/// Closure-like struct to allow use of recursive functions for parsing.
struct ParsingState<'s, 'k> {
    iter: Parser<'s>,
    keywords: &'k mut HashSet<String>,
}

/// Return type for events consumed by not processed by a parsing function.
/// Returned by functions that require an unexpected event to stop parsing (inline, section).
type Consumed<'s> = Option<Event<'s>>;

impl<'s, 'k> ParsingState<'s, 'k> {
    fn new(text: &'s str, keywords: &'k mut HashSet<String>) -> Self {
        Self {
            iter: Parser::new(text),
            keywords,
        }
    }

    /// Parse one markdown document. Consumes the parsing state as the iterator is now empty.
    fn parse_document(mut self) -> Result<Document<'s>, String> {
        let (blocks, sections, next) = self.parse_section_content_at_level(0)?;
        match next {
            None => Ok(Document { blocks, sections }),
            Some(e) => Err(format!("Unexpected element: {:?}", e)),
        }
    }

    /// Parse section (header + content) from start tag (already consumed) to end of section.
    fn parse_section_of_level(
        &mut self,
        level: i32,
    ) -> Result<(Section<'s>, Consumed<'s>), String> {
        let title = match self.parse_inline() {
            (Some(string), Some(Event::End(Tag::Header(n)))) => {
                assert_eq!(n, level);
                string
            }
            (_, Some(e)) => {
                return Err(format!(
                    "Expected header title for level {}: {:?}",
                    level, e
                ))
            }
            (None, _) => panic!("Header without title"),
            (_, None) => panic!("Unclosed header"),
        };
        let (blocks, sub_sections, next) = self.parse_section_content_at_level(level)?;
        Ok((
            Section {
                title,
                blocks,
                sub_sections,
            },
            next,
        ))
    }

    /// Parse contents of a section (recursively) : blocks, then sub sections until next lesser header level.
    /// Assume the current header has just been processed.
    fn parse_section_content_at_level(
        &mut self,
        level: i32,
    ) -> Result<(Vec<BlockElement<'s>>, Vec<Section<'s>>, Consumed<'s>), String> {
        // Local state
        let mut blocks = Vec::new();
        let mut sub_sections = Vec::new();
        // Parse all blocks before first section
        let mut next = loop {
            match self.try_parse_block()? {
                Ok(block) => blocks.push(block),
                Err(next) => break next,
            }
        };
        // Parse all sub sections
        while let Some(Event::Start(Tag::Header(new_level))) = &mut next {
            let new_level = *new_level; // End mut reference to next
            assert!((1..=6).contains(&new_level));
            if new_level <= level {
                // End current section, let caller handle this
                break;
            } else if new_level == level + 1 {
                // Sub section, parse and update next
                let (sub_section, new_next) = self.parse_section_of_level(new_level)?;
                sub_sections.push(sub_section);
                next = new_next
            } else {
                return Err(format!(
                    "Header {} is too deep for current level {}",
                    new_level, level
                ));
            }
        }
        Ok((blocks, sub_sections, next))
    }

    /// Try to parse a block element.
    fn try_parse_block(&mut self) -> Result<Result<BlockElement<'s>, Consumed<'s>>, String> {
        Ok(match self.iter.next() {
            Some(Event::Start(Tag::Paragraph)) => {
                Ok(BlockElement::Paragraph(self.parse_paragraph()?))
            }
            Some(Event::Start(Tag::Rule)) => {
                let event = self.iter.next().expect("Unclosed rule");
                match event {
                    Event::End(Tag::Rule) => Ok(BlockElement::Rule),
                    e => panic!("Expected rule end: {:?}", e),
                }
            }
            Some(Event::Start(Tag::List(start_i))) => {
                Ok(BlockElement::List(self.parse_list(start_i.is_some())?))
            }
            next => Err(next),
        })
    }

    /// Parse paragraph from start tag (already consumed) to end tag (included).
    fn parse_paragraph(&mut self) -> Result<Vec<InlineElement<'s>>, String> {
        let (inline_sequence, next) = self.parse_inline_sequence();
        let next_event = next.expect("Unclosed paragraph");
        match next_event {
            Event::End(Tag::Paragraph) => {
                assert!(inline_sequence.len() > 0);
                Ok(inline_sequence)
            }
            e => Err(format!("Parsing paragraph: unexpected {:?}", e)),
        }
    }

    /// Parse list from start tag (already consumed) to end tag (included).
    fn parse_list(&mut self, ordered: bool) -> Result<List<'s>, String> {
        let mut items: Vec<ListItem<'s>> = Vec::new();
        loop {
            match self.iter.next().expect("Unclosed list") {
                Event::Start(Tag::Item) => items.push(self.parse_list_item()?),
                Event::End(Tag::List(_)) => return Ok(List { ordered, items }),
                e => panic!("Expected list items: {:?}", e),
            }
        }
    }
    fn parse_list_item(&mut self) -> Result<ListItem<'s>, String> {
        let (text_content, next) = self.parse_inline_sequence();
        let next_event = next.expect("Unclosed list item");
        let sub_list = match next_event {
            Event::End(Tag::Item) => None,
            Event::Start(Tag::List(start_i)) => {
                let sub_list = self.parse_list(start_i.is_some())?;
                match self.iter.next().expect("Unclosed list item") {
                    Event::End(Tag::Item) => Some(sub_list),
                    e => return Err(format!("Expected list item end: {:?}", e)),
                }
            }
            e => return Err(format!("Expected list item: {:?}", e)),
        };
        if text_content.len() > 0 {
            Ok(ListItem {
                text_content,
                sub_list,
            })
        } else {
            Err("List item with empty text".into())
        }
    }

    /// Parse a sequence of inline separated by breaks. Sequence may be empty.
    fn parse_inline_sequence(&mut self) -> (Vec<InlineElement<'s>>, Consumed<'s>) {
        let mut inline_elements = Vec::new();
        loop {
            let (inline, next) = self.parse_inline();
            if let Some(inline) = inline {
                inline_elements.push(inline);
            }
            match next {
                Some(Event::SoftBreak) => (),
                Some(Event::HardBreak) => (),
                next => return (inline_elements, next),
            }
        }
    }

    /// Parse one inline text unit (with emphasis / strong), may be empty.
    /// Will panic in case of structural errors slipping past the markdown parser.
    fn parse_inline(&mut self) -> (Option<InlineElement<'s>>, Consumed<'s>) {
        let opt_cow_len = |s: &Option<Cow<'s, str>>| s.as_ref().map_or(0, |s| s.len());
        // local state
        let mut string: Option<Cow<'s, str>> = None;
        let mut strong_parts: Vec<Range<usize>> = Vec::new();
        let mut strong_start: Option<usize> = None;
        let mut emphasis_start: Option<usize> = None;
        // Parse all inline elements
        let next = loop {
            match self.iter.next() {
                Some(Event::Text(s)) => match &mut string {
                    None => {
                        let std_cow = match s {
                            CowStr::Borrowed(b) => Cow::Borrowed(b),
                            owned => Cow::Owned(owned.to_string()),
                        };
                        string = Some(std_cow)
                    }
                    Some(cow) => cow.to_mut().push_str(&s),
                },
                // Emphasis
                Some(Event::Start(Tag::Emphasis)) => {
                    assert_eq!(emphasis_start, None);
                    emphasis_start = Some(opt_cow_len(&string))
                }
                Some(Event::End(Tag::Emphasis)) => {
                    let start = emphasis_start.take().expect("Not in emphasis block");
                    let string = string.as_ref().expect("Empty emphasis block");
                    let end = string.len();
                    self.keywords.insert(string[start..end].to_string());
                }
                // Strong
                Some(Event::Start(Tag::Strong)) => {
                    assert_eq!(strong_start, None);
                    strong_start = Some(opt_cow_len(&string))
                }
                Some(Event::End(Tag::Strong)) => {
                    let start = strong_start.take().expect("Not in strong block");
                    let string = string.as_ref().expect("Empty strong block");
                    let end = string.len();
                    strong_parts.push(start..end);
                }
                next => break next,
            }
        };
        let inline = string.map(|string| InlineElement {
            string,
            strong_parts,
        });
        (inline, next)
    }
}

/// Parse a single document from a string. Also returns the set of keywords.
pub fn parse<'s>(text: &'s str) -> Result<(Document<'s>, HashSet<String>), String> {
    let mut keywords = HashSet::new();
    let document = ParsingState::new(text, &mut keywords).parse_document()?;
    Ok((document, keywords))
}
