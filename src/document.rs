use crate::ast;
use regex::{escape as escape_regex_special_chars, Regex, RegexBuilder};
use std::fmt::{Display, Write};

type KeywordIndex = usize;

pub struct IndexedDocument {
    root: ast::Document,
    keywords: ast::KeywordSet,
    explicit_keyword_occurrences: Vec<Vec<ast::InlineIndex>>,
    implicit_keyword_occurrences: Vec<Vec<ast::InlineIndex>>,
}
impl IndexedDocument {
    pub fn from(document: ast::Document, keywords: ast::KeywordSet) -> IndexedDocument {
        let regex = keyword_search_regex(&keywords).unwrap();
        let matches: Vec<&str> = regex
            .find_iter("wimd a wimdaa hello Wimd")
            .map(|m| m.as_str())
            .collect();
        println!("MATCHES: {:?}", matches);
        unimplemented!()
    }
}

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
