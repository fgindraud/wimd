extern crate pulldown_cmark;

use std::io::{self, Read};

fn read_stdin() -> Result<String, String> {
    let mut s = String::new();
    io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Ok(s)
}

fn main() -> Result<(), String> {
    let text = read_stdin()?;
    // Test print token stream
    for event in pulldown_cmark::Parser::new(&text) {
        println!("{:?}", event)
    }
    // Ast test
    let (root, keywords) = ast::parse(&text)?;
    println!("AST: {:?}", root);
    println!("KWDS: {:?}", keywords);
    Ok(())
}

// Modes:
// - wiki : generates a wiki
// - keyword : extract list of keywords

// Keywords:
// - emphasized words added as kwds once.
// - links generated for all occurences.
// Do not use links "[<kwd>]" as handling is more complex.

// Description / Associate many things with keywords:
// Sentence version "<kwd> : text ; text ; text."
// List version "<kwd>:\n- <text>\n- <text>"

// Keyword occurence -> description "kwd: <text>", and just occurence.

// Wiki : group data by keywords
// - show data in order of .md file (headings, etc). links for keywords
// - by keyword, list of sentences organised by heading position

mod ast {
    use pulldown_cmark::{CowStr, Event, Parser, Tag};
    use std::borrow::Cow;
    use std::collections::HashSet;
    use std::ops::Range;

    /// Root of a markdown document. Equivalent to a level-0 section with no title.
    #[derive(Debug)]
    pub struct Document<'a> {
        blocks: Vec<BlockElement<'a>>,
        sections: Vec<Section<'a>>,
    }

    #[derive(Debug)]
    pub enum BlockElement<'a> {
        Paragraph(Vec<InlineStr<'a>>),
        Rule,
        List,
    }

    #[derive(Debug)]
    pub struct Section<'a> {
        title: InlineStr<'a>,
        blocks: Vec<BlockElement<'a>>,
        sub_sections: Vec<Section<'a>>,
    }

    #[derive(Debug)]
    pub struct InlineStr<'a> {
        /// Raw string content without any formatting
        string: Cow<'a, str>,
        /// List of ranges where a strong marker applies (in order, no overlap)
        strong_parts: Vec<Range<usize>>,
    }

    /// Closure-like struct to allow use of recursive functions for parsing
    struct ParsingState<'a> {
        iter: Parser<'a>,
        keywords: HashSet<String>,
    }

    /// Return type for events consumed by not processed by a parsing function.
    /// Returned by functions that require an unexpected event to stop parsing (inline, section).
    type Consumed<'a> = Option<Event<'a>>;

    impl<'a> ParsingState<'a> {
        fn new(text: &'a str) -> Self {
            Self {
                iter: Parser::new(text),
                keywords: HashSet::new(),
            }
        }

        fn parse_document(mut self) -> Result<(Document<'a>, HashSet<String>), String> {
            let (blocks, sections, next) = self.parse_section_content_at_level(0)?;
            match next {
                None => Ok((Document { blocks, sections }, self.keywords)),
                Some(e) => Err(format!("Unexpected element: {:?}", e)),
            }
        }

        /// Parse section (header + content) from start tag (already consumed) to end of section.
        fn parse_section_of_level(
            &mut self,
            level: i32,
        ) -> Result<(Section<'a>, Consumed<'a>), String> {
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
        ) -> Result<(Vec<BlockElement<'a>>, Vec<Section<'a>>, Consumed<'a>), String> {
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

        /// Try to parse a block element
        fn try_parse_block(&mut self) -> Result<Result<BlockElement<'a>, Consumed<'a>>, String> {
            Ok(match self.iter.next() {
                Some(Event::Start(Tag::Paragraph)) => Ok(self.parse_paragraph()?),
                Some(Event::Start(Tag::Rule)) => Ok(self.parse_rule()),
                Some(Event::Start(Tag::List(ordered))) => Ok(self.parse_list(ordered)?),
                next => Err(next),
            })
        }

        /// Parse paragraph from start tag (already consumed) to end tag (included)
        fn parse_paragraph(&mut self) -> Result<BlockElement<'a>, String> {
            let mut finished_strings = Vec::new();
            loop {
                let (inline_str, next) = self.parse_inline();
                finished_strings.push(inline_str.expect("Empty inline string"));
                let next_event = next.expect("Unclosed paragraph");
                match next_event {
                    Event::End(Tag::Paragraph) => {
                        assert!(finished_strings.len() > 0);
                        return Ok(BlockElement::Paragraph(finished_strings));
                    }
                    Event::SoftBreak | Event::HardBreak => (),
                    e => return Err(format!("Parsing paragraph: unexpected {:?}", e)),
                }
            }
        }

        /// Parse paragraph from start tag (already consumed) to end tag (included)
        fn parse_rule(&mut self) -> BlockElement<'a> {
            let event = self.iter.next().expect("Unclosed rule");
            match event {
                Event::End(Tag::Rule) => BlockElement::Rule,
                e => panic!("Expected rule events: {:?}", e)
            }
        }

        fn parse_list(&mut self, ordered: Option<usize>) -> Result<BlockElement<'a>, String> {
            //TODO handle lists
            for event in &mut self.iter {
                match event {
                    Event::End(Tag::List(_)) => return Ok(BlockElement::List),
                    _ => (),
                }
            }
            Err("Unclosed list".into())
        }

        /// Parse one inline text unit (with emphasis / strong).
        /// Returns no error, and will panic in case of structural errors slipping past the markdown parser.
        fn parse_inline(&mut self) -> (Option<InlineStr<'a>>, Consumed<'a>) {
            let opt_cow_len = |s: &Option<Cow<'a, str>>| s.as_ref().map_or(0, |s| s.len());
            // local state
            let mut string: Option<Cow<'a, str>> = None;
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
            let inline_str = string.map(|string| InlineStr {
                string,
                strong_parts,
            });
            (inline_str, next)
        }
    }

    pub fn parse<'a>(text: &'a str) -> Result<(Document<'a>, HashSet<String>), String> {
        ParsingState::new(text).parse_document()
    }
}

/*struct OriginPosition {
    lines: ops::Range<usize>,
    file: usize,
}
struct Sentence {
    sentence: String,
    heading: HeadingPosition,
    origin: OriginPosition,
}
struct Heading {
    text: String,
    sub_headings: Vec<Heading>,
    sentences: ops::Range<usize>,
    origin: OriginPosition,
}
struct HeadingPosition {
    file_index: usize,
    heading_indexes: [Option<usize>; 6],
}
struct File {
    filename: String,
    headings: Vec<Heading>,
}
struct Keyword {
    keyword: String,
    in_sentences: Vec<usize>,
}

struct Database {
    /// All sentences (not heading)
    sentences: Vec<Sentence>,
    /// File & heading tree
    files: Vec<File>,
    /// All found keywords
    keywords: Vec<Keyword>,
    keyword_indexes: HashMap<String, usize>,
}*/
