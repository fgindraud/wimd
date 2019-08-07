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
    markdown::print_event_list(&text);
    let (root, keywords) = ast::parse_text(&text)?;
    println!("AST: {:?}", root);
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

mod markdown {
    use pulldown_cmark::{Event, Parser};

    pub fn print_event_list(text: &str) {
        for event in Parser::new(&text) {
            println!("{:?}", event)
        }
    }
}

mod ast {
    use pulldown_cmark::{CowStr, Event, Parser, Tag};
    use std::borrow::Cow;
    use std::collections::HashSet;

    /// Root of a file. A section with no real title.
    #[derive(Debug)]
    pub struct File<'a> {
        /// Name, or None if stdin
        name: Option<String>,
        blocks: Vec<BlockElement<'a>>,
        sections: Vec<Section<'a>>,
    }

    #[derive(Debug)]
    pub enum BlockElement<'a> {
        Paragraph(Vec<Cow<'a, str>>),
        List,
    }

    #[derive(Debug)]
    pub struct Section<'a> {
        title: Cow<'a, str>,
        blocks: Vec<BlockElement<'a>>,
        sub_sections: Vec<Section<'a>>,
    }

    /// Closure-like struct to allow use of recursive functions for parsing
    struct ParsingState<'a> {
        iter: Parser<'a>,
        keywords: HashSet<String>,
    }

    enum TryParseValue<T> {
        /// No value due to end of event stream
        NoEvent,
        /// No value because a header outside our capabilities has appeared. Header start event consumed.
        HeaderStart(i32),
        Value(T),
    }

    fn to_std_cow<'a>(s: CowStr<'a>) -> Cow<'a, str> {
        match s {
            CowStr::Borrowed(b) => Cow::Borrowed(b),
            owned => Cow::Owned(owned.to_string()),
        }
    }
    fn accumulate_to_cow<'a>(acc: &mut Option<Cow<'a, str>>, s: CowStr<'a>) {
        match acc {
            None => *acc = Some(to_std_cow(s)),
            Some(cow) => cow.to_mut().push_str(&s),
        }
    }

    impl<'a> ParsingState<'a> {
        fn new(text: &'a str) -> Self {
            Self {
                iter: Parser::new(text),
                keywords: HashSet::new(),
            }
        }

        fn parse_file(
            mut self,
            filename: Option<String>,
        ) -> Result<(File<'a>, HashSet<String>), String> {
            let (blocks, sections, no_next_header) = self.parse_section_content_at_level(0)?;
            assert_eq!(no_next_header, None);
            Ok((
                File {
                    name: filename,
                    blocks,
                    sections,
                },
                self.keywords,
            ))
        }

        /// Parse section (header + content) from start tag (already consumed) to end of section.
        /// Return section and next header level if not end of events.
        fn parse_section_of_level(
            &mut self,
            level: i32,
        ) -> Result<(Section<'a>, Option<i32>), String> {
            let title = {
                let mut title_string = None;
                loop {
                    match self.iter.next() {
                        None => return Err("Unclosed header title".into()),
                        Some(Event::Text(s)) => accumulate_to_cow(&mut title_string, s),
                        Some(Event::End(Tag::Header(n))) if n == level => match title_string {
                            None => return Err("Empty header title".into()),
                            Some(cow) => break cow,
                        },
                        Some(e) => {
                            return Err(format!(
                                "Expected header title for level {}: {:?}",
                                level, e
                            ))
                        }
                    }
                }
            };
            let (blocks, sub_sections, next_header_level) =
                self.parse_section_content_at_level(level)?;
            Ok((
                Section {
                    title,
                    blocks,
                    sub_sections,
                },
                next_header_level,
            ))
        }

        /// Parse contents of a section (recursively) : blocks, then sub sections until next lesser header level.
        /// Assume the current header has just been processed.
        /// Returns contents and next header level if any (start tag already parsed).
        fn parse_section_content_at_level(
            &mut self,
            level: i32,
        ) -> Result<(Vec<BlockElement<'a>>, Vec<Section<'a>>, Option<i32>), String> {
            let mut blocks = Vec::new();
            let mut sub_sections = Vec::new();
            // Parse all blocks before first section
            let mut next_header_start = loop {
                match self.try_parse_block()? {
                    TryParseValue::NoEvent => break None,
                    TryParseValue::HeaderStart(level) => break Some(level),
                    TryParseValue::Value(block) => blocks.push(block),
                }
            };
            // Parse all sub sections
            let after_section_header_start = loop {
                match next_header_start.take() {
                    None => break None,
                    Some(next_header_level) => {
                        assert!((1..=6).contains(&next_header_level));
                        if next_header_level <= level {
                            break Some(next_header_level);
                        } else if next_header_level == level + 1 {
                            let (sub_section, current_next_header) =
                                self.parse_section_of_level(next_header_level)?;
                            sub_sections.push(sub_section);
                            next_header_start = current_next_header;
                        } else {
                            return Err(format!(
                                "Header {} is too deep for current level {}",
                                next_header_level, level
                            ));
                        }
                    }
                }
            };
            Ok((blocks, sub_sections, after_section_header_start))
        }

        /// Try to parse a block element
        fn try_parse_block(&mut self) -> Result<TryParseValue<BlockElement<'a>>, String> {
            match self.iter.next() {
                None => Ok(TryParseValue::NoEvent),
                Some(Event::Start(Tag::Header(level))) => Ok(TryParseValue::HeaderStart(level)),
                Some(Event::Start(Tag::Paragraph)) => {
                    Ok(TryParseValue::Value(self.parse_paragraph()?))
                }
                Some(Event::Start(Tag::List(ordered))) => {
                    Ok(TryParseValue::Value(self.parse_list(ordered)?))
                }
                Some(e) => Err(format!("Expected block element: {:?}", e)),
            }
        }

        /// Parse paragraph from start tag (already consumed) to end tag (included)
        fn parse_paragraph(&mut self) -> Result<BlockElement<'a>, String> {
            let mut finished_strings = Vec::new();
            let mut current_string = None;
            for event in &mut self.iter {
                match event {
                    Event::End(Tag::Paragraph) => {
                        if let Some(s) = current_string.take() {
                            finished_strings.push(s)
                        }
                        return Ok(BlockElement::Paragraph(finished_strings));
                    }
                    Event::Text(s) => accumulate_to_cow(&mut current_string, s),
                    Event::SoftBreak | Event::HardBreak => {
                        if let Some(s) = current_string.take() {
                            finished_strings.push(s)
                        }
                    }
                    Event::Start(Tag::Emphasis) | Event::End(Tag::Emphasis) => {
                        // TODO handle emphasis
                    }
                    Event::Start(Tag::Strong) | Event::End(Tag::Strong) => {
                        // TODO handle Strong
                    }
                    e => return Err(format!("Parsing paragraph: unexpected {:?}", e)),
                }
            }
            Err("Unclosed paragraph".into())
        }

        fn parse_list(&mut self, ordered: Option<usize>) -> Result<BlockElement<'a>, String> {
            //TODO handle lists
            for event in &mut self.iter {
                match event {
                    Event::End(Tag::List(_)) => return Ok(BlockElement::List),
                    _ => ()
                }
            }
            Err("Unclosed list".into())
        }
    }

    pub fn parse_text<'a>(text: &'a str) -> Result<(File<'a>, HashSet<String>), String> {
        ParsingState::new(text).parse_file(None)
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
