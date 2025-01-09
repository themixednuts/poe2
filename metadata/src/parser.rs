use std::collections::HashMap;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{
        alphanumeric1, digit1, line_ending, multispace0, multispace1, not_line_ending, space0,
        space1, tab,
    },
    combinator::{map, map_res},
    multi::many1,
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult, Parser,
};

pub fn version(input: &str) -> IResult<&str, u32> {
    map_res(
        terminated(
            preceded(tag("version"), preceded(space1, digit1)),
            multispace1,
        ),
        str::parse,
    )(input)
}

pub fn is_abstract(input: &str) -> IResult<&str, bool> {
    terminated(tag("abstract"), multispace1)(input).map(|(s, _)| (s, true))
}

pub fn extends(input: &str) -> IResult<&str, Option<&str>> {
    terminated(
        preceded(
            tag("extends"),
            preceded(space1, delimited(tag("\""), take_until("\""), tag("\""))),
        ),
        multispace1,
    )(input)
    .map(|(s, extends)| {
        if extends == "nothing" {
            (s, None)
        } else {
            (s, Some(extends))
        }
    })
}

pub fn brackets(input: &str) -> IResult<&str, (&str, &str, &str)> {
    tuple((tag("{"), space1, tag("}")))(input)
}

// use of alt? what do we call this
// might be a list/array
pub fn block(input: &str) -> IResult<&str, Block> {
    let t1 = terminated(
        take_until(" "),
        tuple((multispace1, tag("{"), multispace1, tag("}"), multispace0)),
    )
    .map(|name| Block {
        name,
        values: HashMap::new(),
    });

    let t2 = tuple((
        terminated(not_line_ending, line_ending),
        delimited(
            terminated(tag("{"), line_ending),
            take_until("}"),
            alt((terminated(tag("}"), multispace0), tag("}"))),
        ),
    ))
    .map(|(name, contents)| {
        let mut block = Block {
            name,
            values: HashMap::new(),
        };

        let mut current_input = contents;
        while !current_input.is_empty() {
            match block_content(current_input) {
                Ok((remaining, (key, value))) => {
                    // Handle repeated keys by converting to array
                    match block.values.get_mut(key) {
                        Some(Value::Array(arr)) => {
                            arr.push(value);
                        }
                        Some(existing) => {
                            let arr = vec![existing.clone(), value];
                            block.values.insert(key, Value::Array(arr));
                        }
                        None => {
                            block.values.insert(key, value);
                        }
                    }
                    current_input = remaining;
                }
                Err(_) => break,
            }
        }

        block
    });

    alt((t1, t2))(input)
}
fn block_content(input: &str) -> IResult<&str, (&str, Value)> {
    alt((
        // Handle nested blocks
        map(block, |b| (b.name, Value::Object(b.values))),
        // Handle key-value pairs
        separated_pair(
            preceded(multispace0, alphanumeric1),
            tuple((space0, tag("="), space0)),
            alt((
                // Handle quoted strings
                map(
                    delimited(tag("\""), take_until("\""), tag("\"")),
                    Value::String,
                ),
                // Handle numbers
                map_res(
                    take_until("\n"),
                    |s: &str| -> Result<Value, std::num::ParseFloatError> {
                        s.trim().parse::<f64>().map(Value::Number)
                    },
                ),
            )),
        ),
    ))(input)
}

pub fn comment(input: &str) -> IResult<&str, Comment> {
    terminated(pair(tag("//"), not_line_ending), line_ending)(input)
        .map(|(s, (_, comment))| (s, Comment { value: comment }))
}

#[derive(Debug, Default)]
pub struct Block<'a> {
    pub name: &'a str,
    pub values: HashMap<&'a str, Value<'a>>,
}

#[derive(Debug, Clone)]
pub enum Value<'a> {
    String(&'a str),
    Number(f64),
    Boolean(bool),
    Array(Vec<Value<'a>>),
    Object(HashMap<&'a str, Value<'a>>),
}

struct Comment<'a> {
    value: &'a str,
}

#[cfg(test)]
mod tests {
    // Handle mutliple different cases, not really sure what they are atm
    // BaseEvents
    // {
    // 	server_only = true
    // }
    //
    // stance CrossbowTown
    // {
    //   with_tag Idle::town;
    // }

    #[test]
    pub fn class() {}
}
