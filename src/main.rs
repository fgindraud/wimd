#[macro_use]
extern crate clap; // CLI interface

// Data structures
extern crate indexmap; // Indexed keyword set
extern crate unicase; // Case insensitive string

// Parsing
extern crate pulldown_cmark; // Markdown parser
extern crate regex; // Regex for efficient keyword search

/// AST for supported subset of markdown syntax, with parsing.
mod ast;

use clap::Arg;
use std::io::{self, Read};

fn main() -> Result<(), String> {
    let args = app_from_crate!()
        .arg(
            Arg::with_name("tokens")
                .help("Prints markdown token list and stop")
                .long("tokens"),
        )
        .arg(
            Arg::with_name("keywords")
                .help("Prints extracted keyword list and stop")
                .short("k")
                .long("keywords"),
        )
        .get_matches();

    let text = read_stdin()?;

    if args.is_present("tokens") {
        // Test print token stream
        for event in pulldown_cmark::Parser::new(&text) {
            println!("{:?}", event)
        }
        return Ok(());
    }

    let (ast, keywords) = ast::parse(&text)?;

    if args.is_present("keywords") {
        let mut keywords: Vec<_> = keywords.into_iter().collect();
        keywords.sort_unstable();
        for keyword in keywords {
            println!("{}", keyword);
        }
        return Ok(());
    }

    let document = IndexedDocument::new(ast, keywords);

    Ok(())
}

fn read_stdin() -> Result<String, String> {
    let mut s = String::new();
    io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Ok(s)
}

type KeywordIndex = usize;
struct IndexedDocument {
    root: ast::Document,
    keywords: ast::KeywordSet,
    explicit_keyword_occurrences: Vec<Vec<ast::InlineIndex>>,
    implicit_keyword_occurrences: Vec<Vec<ast::InlineIndex>>,
}
impl IndexedDocument {
    fn new(document: ast::Document, keywords: ast::KeywordSet) -> IndexedDocument {
        let regex = keyword_search_regex(&keywords).unwrap();
        let matches: Vec<&str> = regex
            .find_iter("wimd a wimdaa hello Wimd")
            .map(|m| m.as_str())
            .collect();
        println!("MATCHES: {:?}", matches);
        unimplemented!()
    }
}

use regex::{escape as escape_regex_special_chars, Regex, RegexBuilder};
use std::fmt::{Display, Write};

/// Build the regex used to find keywords in linear time.
/// Return the regex, or None if the keyword set contains the empty string or is empty.
///
/// The regex is built like "\b(kwd1|kwd2|...|kwdN)\b" and will be run on all inline text.
/// It matches when one of the keywords is found on word boundaries.
/// This avoids matching word prefixes, like "hell" in "hello world".
/// Matches are non overlapping so extracted keywords will be non overlapping.
/// Lastly, keywords in the alternate part are ordered by decreasing length to prefer the biggest valid matches.
fn keyword_search_regex(keywords: &ast::KeywordSet) -> Option<Regex> {
    let mut keyword_list: Vec<&str> = keywords.iter().map(|s| s.as_ref()).collect();
    keyword_list.sort_unstable_by_key(|s| -(s.len() as i64));
    if keyword_list.last().map_or(true, |s| s.len() == 0) {
        return None; // Fail if empty list of empty string in list
    }
    let keyword_list = keyword_list.into_iter().map(escape_regex_special_chars);
    let regex_str = format!(r"\b({})\b", join(keyword_list, "|"));
    let regex = RegexBuilder::new(&regex_str)
        .case_insensitive(true)
        .unicode(true)
        .build()
        .expect("Keyword regex construction");
    Some(regex)
}

fn join<I>(mut iter: I, sep: &str) -> String
where
    I: Iterator,
    <I as Iterator>::Item: Display,
{
    match iter.next() {
        None => String::new(),
        Some(first) => {
            let (lower, _) = iter.size_hint();
            let mut s = String::with_capacity(sep.len() * lower);
            write!(&mut s, "{}", first).unwrap();
            for element in iter {
                s.push_str(sep);
                write!(&mut s, "{}", element).unwrap();
            }
            s
        }
    }
}

// Description / Associate many things with keywords:
// Sentence version "<kwd> : text ; text ; text."
// List version "<kwd>:\n- <text>\n- <text>"

// Keyword occurence -> description "kwd: <text>", and just occurence.

// Wiki : group data by keywords
// - show data in order of .md file (headings, etc). links for keywords
// - by keyword, list of sentences organised by heading position
