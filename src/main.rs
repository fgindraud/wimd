/// AST for supported subset of markdown syntax, with parsing.
mod ast;

/// Indexed markdown document data structure.
mod document;
use document::IndexedDocument;

use clap::Arg;
use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version};
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

    let document = IndexedDocument::from(ast, keywords);

    Ok(())
}

fn read_stdin() -> Result<String, String> {
    let mut s = String::new();
    io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Ok(s)
}

// Description / Associate many things with keywords:
// Sentence version "<kwd> : text ; text ; text."
// List version "<kwd>:\n- <text>\n- <text>"

// Keyword occurence -> description "kwd: <text>", and just occurence.

// Wiki : group data by keywords
// - show data in order of .md file (headings, etc). links for keywords
// - by keyword, list of sentences organised by heading position
