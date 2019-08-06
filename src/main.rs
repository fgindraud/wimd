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
    let ast = ast::parse_text(&text)?;
    println!("AST: {:?}", ast.value);
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

    fn to_std_cow<'a>(s: CowStr<'a>) -> Cow<'a, str> {
        match s {
            CowStr::Borrowed(b) => Cow::Borrowed(b),
            owned => Cow::Owned(owned.to_string()),
        }
    }

    /// Root of a file
    pub struct File<'a> {
        name: String,
        elements: Elements<'a>,
    }

    pub type Elements<'a> = Vec<Element<'a>>;

    /// Structural elements
    #[derive(Debug)]
    pub enum Element<'a> {
        Paragraph(Vec<Cow<'a, str>>),
        Section {
            title: Cow<'a, str>,
            elements: Elements<'a>,
        },
    }

    pub struct KeywordsAnd<T> {
        pub keywords: HashSet<String>,
        pub value: T,
    }

    struct ParsingState<'a> {
        iter: Parser<'a>,
        keywords: HashSet<String>,
    }
    impl<'a> ParsingState<'a> {
        fn new(text: &'a str) -> Self {
            Self {
                iter: Parser::new(text),
                keywords: HashSet::new(),
            }
        }

        fn parse_structural_sequence(&mut self) -> Result<Elements<'a>, String> {
            let mut v = Vec::new();
            while let Some(structural) = self.try_parse_structural()? {
                v.push(structural)
            }
            Ok(v)
        }

        fn try_parse_structural(&mut self) -> Result<Option<Element<'a>>, String> {
            match self.iter.next() {
                None => Ok(None),
                Some(Event::Start(Tag::Paragraph)) => Ok(Some(self.parse_paragraph()?)),
                Some(e) => Err(format!("Expected structural element: {:?}", e)),
            }
        }

        fn parse_paragraph(&mut self) -> Result<Element<'a>, String> {
            let mut finished_strings = Vec::new();
            let mut current_string = None;
            for event in &mut self.iter {
                match event {
                    Event::End(Tag::Paragraph) => {
                        if let Some(s) = current_string {
                            finished_strings.push(s);
                            current_string = None
                        }
                        return Ok(Element::Paragraph(finished_strings));
                    }
                    Event::Text(s) => {
                        current_string = match current_string {
                            None => Some(to_std_cow(s)),
                            Some(mut cow) => {
                                cow.to_mut().push_str(&s);
                                Some(cow)
                            }
                        };
                    }
                    Event::SoftBreak | Event::HardBreak => {
                        if let Some(s) = current_string {
                            finished_strings.push(s);
                            current_string = None
                        }
                    }
                    e => return Err(format!("Parsing paragraph: unexpected {:?}", e)),
                }
            }
            Err("Unclosed paragraph".into())
        }
    }

    pub fn parse_text<'a>(text: &'a str) -> Result<KeywordsAnd<Elements<'a>>, String> {
        let mut parsing_state = ParsingState::new(text);
        let elements = parsing_state.parse_structural_sequence()?;
        Ok(KeywordsAnd {
            keywords: parsing_state.keywords,
            value: elements,
        })
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
