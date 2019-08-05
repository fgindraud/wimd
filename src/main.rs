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
    let first_pass = markdown::first_pass(&text)?;
    println!("FIRST {:?}", first_pass.strings);
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

    // Single markdown file
    struct MarkdownDocument {
        /// All text segments, in order of appearance.
        strings: Vec<String>,
        /// All keywords.
        keywords: Vec<String>,
    }

    pub struct FirstPassOutput {
        pub strings: Vec<String>,
        keywords: Vec<String>,
    }

    // First pass : extract sentences, structure, keywords
    pub fn first_pass(text: &str) -> Result<FirstPassOutput, String> {
        let mut strings = Vec::new();
        let mut keywords = Vec::new();

        // Use peekable iterator to avoid accumulator ?
        // Filter accumulated strings ?

        let mut current_string = String::new();
        for event in Parser::new(text) {
            match event {
                // Text elements can be split if they contain markers, regroup them
                Event::Text(text) => current_string.push_str(&text),
                Event::Start(_) | Event::End(_) => {
                    let s = std::mem::replace(&mut current_string, String::new());
                    strings.extend(s.split('.').map(|s| s.to_string()))
                }
                // Ignore breaks
                Event::SoftBreak => (),
                Event::HardBreak => (),
                // Everything else is rejected for now
                _ => return Err(format!("Event not supported: {:?}", event)),
            }
        }

        Ok(FirstPassOutput { strings, keywords })
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
