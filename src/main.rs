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
    let markdown = read_stdin()?;
    for event in pulldown_cmark::Parser::new(&markdown) {
        println!("{:?}", event)
    }
    Ok(())
}
