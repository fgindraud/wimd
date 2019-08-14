use indexmap::IndexSet;
use pulldown_cmark::{Event, OffsetIter, Parser, Tag};
use std::ops::Range;
use unicase::UniCase;

/******************************************************************************
 * Ast definition.
 *
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
 * Restrictions:
 * - strong/emphasis tags cannot be multiline (not used, and not willing to support).
 *
 * Other elements are deemed not useful for RPG notes for now.
 * Using them will generate a fatal parsing error.
 * Links are not used for keyword definition as they have complex cases to handle.
 */

/// Root of a markdown document. Equivalent to a level-0 section with no title.
pub type Document = SectionContent;

#[derive(Debug)]
pub struct Section {
    pub title: InlineElement,
    pub content: SectionContent,
}

#[derive(Debug)]
pub struct SectionContent {
    pub blocks: Vec<BlockElement>,
    pub sub_sections: Vec<Section>,
}

#[derive(Debug)]
pub enum BlockElement {
    Paragraph(Vec<InlineElement>),
    Rule,
    List(List),
}

#[derive(Debug)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
}

#[derive(Debug)]
pub struct ListItem {
    /// Possibly multiline text. Must be non empty.
    pub text_content: Vec<InlineElement>,
    pub sub_list: Option<List>,
}

#[derive(Debug)]
pub struct InlineElement {
    /// Unique index
    pub index: InlineIndex,
    /// Raw string content without any formatting
    pub string: String,
    /// List of tagged ranges (order FIXME)
    pub tags: Vec<(Range<usize>, InlineTag)>,
}

/// All InlineElement are indexed by order of appearance in the document.
pub type InlineIndex = usize;

/// Tags for parts of an inline element. Unless specified, must not overlap.
#[derive(Debug)]
pub enum InlineTag {
    /// Non semantic highlight, mapped to strong in markdown/html. May overlap with keyword.
    Highlight,
    /// Explicit keyword occurrence (using emphasis) with keyword index.
    ExplicitKeyword(usize),
    /// Implicit keyword occurrence, found by search of known keywords.
    ImplicitKeyword(usize),
}

/******************************************************************************
 * Parsing.
 *
 * Parsing error behavior:
 * - normal error for unsupported parts of markdown.
 * - panic if the Parser returns unexpected events: unclosed tags, etc.
 */

/// Closure-like struct to allow use of recursive functions for parsing.
struct ParsingState<'s, 'k> {
    iter: OffsetIter<'s>,
    keywords: &'k mut KeywordSet,
    inline_element_count: usize,
}

/// Return type for events consumed by not processed by a parsing function.
/// Returned by functions that require an unexpected event to stop parsing (inline, section).
type Consumed<'s> = Option<(Event<'s>, usize)>;

/// Error message and indicative offset.
type Error = (String, usize);

impl<'s, 'k> ParsingState<'s, 'k> {
    fn new(text: &'s str, keywords: &'k mut KeywordSet) -> Self {
        Self {
            iter: Parser::new(text).into_offset_iter(),
            keywords,
            inline_element_count: 0,
        }
    }

    fn consume(&mut self) -> Consumed<'s> {
        self.iter.next().map(|(e, r)| (e, r.start))
    }

    /// Parse one markdown document. Consumes the parsing state as the iterator is now empty.
    fn parse_document(mut self) -> Result<Document, Error> {
        let (root_content, next) = self.parse_section_content_at_level(0)?;
        match next {
            None => Ok(root_content),
            Some((e, o)) => Err((format!("Unexpected element: {:?}", e), o)),
        }
    }

    /// Parse section (header + content) from start tag (already consumed) to end of section.
    fn parse_section_of_level(&mut self, level: i32) -> Result<(Section, Consumed<'s>), Error> {
        let title = match self.parse_inline()? {
            (Some(string), Some((Event::End(Tag::Header(n)), _))) => {
                assert_eq!(n, level);
                string
            }
            (_, Some((e, o))) => {
                return Err((
                    format!("Expected header title for level {}: {:?}", level, e),
                    o,
                ))
            }
            (None, _) => panic!("Header without title"),
            (_, None) => panic!("Unclosed header"),
        };
        let (content, next) = self.parse_section_content_at_level(level)?;
        Ok((Section { title, content }, next))
    }

    /// Parse contents of a section (recursively) : blocks, then sub sections until next lesser header level.
    /// Assume the current header has just been processed.
    fn parse_section_content_at_level(
        &mut self,
        level: i32,
    ) -> Result<(SectionContent, Consumed<'s>), Error> {
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
        while let Some((Event::Start(Tag::Header(new_level)), o)) = &mut next {
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
                return Err((
                    format!(
                        "Header {} is too deep for current level {}",
                        new_level, level
                    ),
                    *o,
                ));
            }
        }
        Ok((
            SectionContent {
                blocks,
                sub_sections,
            },
            next,
        ))
    }

    /// Try to parse a block element.
    fn try_parse_block(&mut self) -> Result<Result<BlockElement, Consumed<'s>>, Error> {
        Ok(match self.consume() {
            Some((Event::Start(Tag::Paragraph), _)) => {
                Ok(BlockElement::Paragraph(self.parse_paragraph()?))
            }
            Some((Event::Start(Tag::Rule), _)) => {
                let event = self.consume().expect("Unclosed rule").0;
                match event {
                    Event::End(Tag::Rule) => Ok(BlockElement::Rule),
                    e => panic!("Expected rule end: {:?}", e),
                }
            }
            Some((Event::Start(Tag::List(start_i)), _)) => {
                Ok(BlockElement::List(self.parse_list(start_i.is_some())?))
            }
            next => Err(next),
        })
    }

    /// Parse paragraph from start tag (already consumed) to end tag (included).
    fn parse_paragraph(&mut self) -> Result<Vec<InlineElement>, Error> {
        let (inline_sequence, next) = self.parse_inline_sequence()?;
        let next_event = next.expect("Unclosed paragraph");
        match next_event {
            (Event::End(Tag::Paragraph), _) => {
                assert!(inline_sequence.len() > 0);
                Ok(inline_sequence)
            }
            (e, o) => Err((format!("Parsing paragraph: unexpected {:?}", e), o)),
        }
    }

    /// Parse list from start tag (already consumed) to end tag (included).
    fn parse_list(&mut self, ordered: bool) -> Result<List, Error> {
        let mut items: Vec<ListItem> = Vec::new();
        loop {
            match self.consume().expect("Unclosed list").0 {
                Event::Start(Tag::Item) => items.push(self.parse_list_item()?),
                Event::End(Tag::List(_)) => return Ok(List { ordered, items }),
                e => panic!("Expected list items: {:?}", e),
            }
        }
    }
    fn parse_list_item(&mut self) -> Result<ListItem, Error> {
        let (text_content, next) = self.parse_inline_sequence()?;
        let next_event = next.expect("Unclosed list item");
        if text_content.len() == 0 {
            return Err(("List item with empty text".into(), next_event.1));
        }
        let sub_list = match next_event {
            (Event::End(Tag::Item), _) => None,
            (Event::Start(Tag::List(start_i)), _) => {
                let sub_list = self.parse_list(start_i.is_some())?;
                match self.consume().expect("Unclosed list item") {
                    (Event::End(Tag::Item), _) => Some(sub_list),
                    (e, o) => return Err((format!("Expected list item end: {:?}", e), o)),
                }
            }
            (e, o) => return Err((format!("Expected list item: {:?}", e), o)),
        };
        Ok(ListItem {
            text_content,
            sub_list,
        })
    }

    /// Parse a sequence of inline separated by breaks. Sequence may be empty.
    fn parse_inline_sequence(&mut self) -> Result<(Vec<InlineElement>, Consumed<'s>), Error> {
        let mut inline_elements = Vec::new();
        loop {
            let (inline, next) = self.parse_inline()?;
            if let Some(inline) = inline {
                inline_elements.push(inline);
            }
            match next {
                Some((Event::SoftBreak, _)) => (),
                Some((Event::HardBreak, _)) => (),
                next => return Ok((inline_elements, next)),
            }
        }
    }

    /// Parse one inline text unit (with emphasis / strong), may be empty.
    /// Will panic in case of structural errors slipping past the markdown parser.
    fn parse_inline(&mut self) -> Result<(Option<InlineElement>, Consumed<'s>), Error> {
        let opt_len = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        // local state
        let mut string: Option<String> = None;
        let mut tags: Vec<(Range<usize>, InlineTag)> = Vec::new();
        let mut strong_start: Option<usize> = None;
        let mut emphasis_start: Option<usize> = None;
        // Parse all inline elements
        let next = loop {
            match self.consume() {
                Some((Event::Text(s), _)) => match &mut string {
                    None => string = Some(s.into_string()),
                    Some(string) => string.push_str(&s),
                },
                // Emphasis
                Some((Event::Start(Tag::Emphasis), _)) => {
                    assert_eq!(emphasis_start, None);
                    emphasis_start = Some(opt_len(&string))
                }
                Some((Event::End(Tag::Emphasis), o)) => {
                    let start = match emphasis_start.take() {
                        Some(start) => start,
                        // Assume Parser is correct and this end tag has a start tag before.
                        // This end tag is for a start tag on a previous inline.
                        None => return Err(("Multiline emphasis not supported".into(), o)),
                    };
                    let string = string.as_ref().expect("Empty emphasis block");
                    let end = string.len();
                    let string = string[start..end].to_string();
                    let (index, _) = self.keywords.insert_full(UniCase::new(string));
                    tags.push((start..end, InlineTag::ExplicitKeyword(index)))
                }
                // Strong
                Some((Event::Start(Tag::Strong), _)) => {
                    assert_eq!(strong_start, None);
                    strong_start = Some(opt_len(&string))
                }
                Some((Event::End(Tag::Strong), o)) => {
                    let start = match strong_start.take() {
                        Some(start) => start,
                        None => return Err(("Multiline strong not supported".into(), o)),
                    };
                    let string = string.as_ref().expect("Empty strong block");
                    let end = string.len();
                    tags.push((start..end, InlineTag::Highlight))
                }
                next => break next,
            }
        };
        let inline = string.map(|string| {
            let index = self.inline_element_count;
            self.inline_element_count = self.inline_element_count + 1;
            InlineElement {
                index,
                string,
                tags,
            }
        });
        Ok((inline, next))
    }
}

/// Return the line number at a given offset, starting from 0.
fn line_number_of_offset(text: &str, offset: usize) -> usize {
    text.bytes().take(offset).filter(|b| *b == b'\n').count()
}

/// Set of keywords: indexed, and case insensitive.
pub type KeywordSet = IndexSet<UniCase<String>>;

/// Parse a single document from a string. Also returns the set of keywords.
/// The returned AST only contains explicit keyword occurrences.
/// The AST should not be modified, as it might break internal indexation.
/// This is not restricted by the interface for simplicity.
pub fn parse(text: &str) -> Result<(Document, KeywordSet), String> {
    let mut keywords = IndexSet::new();
    match ParsingState::new(text, &mut keywords).parse_document() {
        Ok(document) => Ok((document, keywords)),
        Err((msg, offset)) => Err(format!(
            "At line {}: {}",
            line_number_of_offset(text, offset) + 1,
            msg
        )),
    }
}

#[test]
fn parsing() {
    // Line number
    assert_eq!(line_number_of_offset("Blah", 0), 0);
    assert_eq!(line_number_of_offset("Blah", 4), 0);
    assert_eq!(line_number_of_offset("\nBlah\n", 0), 0);
    assert_eq!(line_number_of_offset("\nBlah\n", 1), 1);
    assert_eq!(line_number_of_offset("\nBlah\n", 5), 1);
    assert_eq!(line_number_of_offset("\nBlah\n", 6), 2);
}
