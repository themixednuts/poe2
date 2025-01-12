pub(crate) mod tokens;

use nom::{
    branch::{alt, permutation},
    bytes::{
        complete::{escaped, take_until},
        streaming::is_not,
    },
    character::complete::{
        alphanumeric1, digit1, line_ending, multispace0, multispace1, not_line_ending, one_of,
        space0, space1,
    },
    combinator::{consumed, map, map_parser, map_res, opt, recognize, value},
    multi::{many0, many1},
    number::complete::{double, float},
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult,
};
use nom_supreme::tag::complete::{tag, tag_no_case};
use tokens::{BoolLit, Expr, ExprAssign, ExprBlock, FloatLit, Ident, IntLit, Lit, Span, StrLit};

pub fn bom(input: Span) -> IResult<Span, bool> {
    alt((value(true, tag("\u{FEFF}")), value(false, tag("\u{FFFE}"))))(input)
}

pub fn version(input: Span) -> IResult<Span, u32> {
    terminated(
        preceded(
            tag("version"),
            preceded(space1, map_res(digit1, |span: Span| span.parse())),
        ),
        multispace1,
    )(input)
}

pub fn is_abstract(input: Span) -> IResult<Span, bool> {
    terminated(tag("abstract"), multispace1)(input).map(|(s, _)| (s, true))
}

pub fn extends(input: Span) -> IResult<Span, Option<Span>> {
    terminated(
        preceded(
            tag("extends"),
            map(preceded(space1, valid_string), |ext| {
                ext.data().ne(&"nothing").then_some(ext)
            }),
        ),
        multispace1,
    )(input)
}

pub fn expr_block(input: Span) -> IResult<Span, ExprBlock> {
    map(
        tuple((
            opt(comment),
            terminated(not_line_ending, multispace1),
            map(opt(tag("//")), |v| v.is_some()),
            delimited(
                terminated(tag("{"), alt((space1, line_ending))),
                many0(expr),
                tag("}"),
            ),
        )),
        |(doc, span, commented_out, values, ..)| ExprBlock {
            r#type: &span,
            // comment,
            values,
            name: None,
            extends: None,
            span,
            doc,
            commented_out,
        },
    )(input)
    // .inspect_err(|e| {
    //     dbg!(e);
    // })
}

pub fn expr(input: Span) -> IResult<Span, Expr> {
    delimited(
        multispace0,
        alt((
            map(expr_block, Expr::from),
            map(expr_assign, Expr::from),
            map(lit, Expr::from),
        )),
        multispace0,
    )(input)
}

pub fn comment(input: Span) -> IResult<Span, Comment> {
    map(preceded(tag("//"), not_line_ending), |value| Comment {
        value,
    })(input)
}

fn lit(input: Span) -> IResult<Span, Lit> {
    alt((
        map(str_lit, Lit::from),
        map(float_lit, Lit::from),
        map(int_lit, Lit::from),
        map(bool_lit, Lit::from),
    ))(input)
}

pub fn expr_assign(input: Span) -> IResult<Span, ExprAssign> {
    map(
        tuple((
            map(opt(pair(tag("//"), space0)), |v| v.is_some()),
            separated_pair(
                recognize(many1(permutation((opt(tag("_")), alphanumeric1)))),
                tuple((space1, tag("="), multispace1)),
                expr,
            ),
        )),
        |(commented_out, (left, value), ..)| ExprAssign {
            left: Ident {
                sym: &left,
                span: left,
            },
            right: Box::new(value),
            doc: None,
            // comment,
            commented_out,
        },
    )(input)
}

fn valid_string(input: Span) -> IResult<Span, Span> {
    delimited(
        tag("\""),
        escaped(is_not("\"\\"), '\\', one_of(r#""\rnt"#)),
        tag("\""),
    )(input)
}

fn str_lit(input: Span) -> IResult<Span, StrLit<'_>> {
    map(valid_string, |span| StrLit { value: &span, span })(input)
}
fn float_lit(input: Span) -> IResult<Span, FloatLit<'_>> {
    alt((
        map(consumed(double), |(span, value)| FloatLit { value, span }),
        map(consumed(float), |(span, value)| FloatLit {
            value: value as f64,
            span,
        }),
    ))(input)
}

fn int_lit(input: Span) -> IResult<Span, IntLit<'_>> {
    map(
        consumed(map_res(digit1, |span: Span| span.parse::<i64>())),
        |(span, value)| IntLit { value, span },
    )(input)
}

fn bool_lit(input: Span) -> IResult<Span, BoolLit<'_>> {
    map(
        consumed(alt((
            value(true, tag_no_case("true")),
            value(false, tag_no_case("false")),
        ))),
        |(span, value)| BoolLit { value, span },
    )(input)
}

#[derive(Debug, Clone)]
pub struct Comment<'a> {
    value: Span<'a>,
}

// #[cfg(test)]
// mod tests {
//     // Handle mutliple different cases, not really sure what they are atm
//     // BaseEvents
//     // {
//     // 	server_only = true
//     // }
//     //
//     // stance CrossbowTown
//     // {
//     //   with_tag Idle::town;
//     // }

//     // #[test]
//     // pub fn class() {}
// }
