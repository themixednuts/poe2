pub(crate) mod tokens;

use indexmap::{map::Entry, IndexMap};
use nom::{
    branch::{alt, permutation},
    bytes::{
        complete::{escaped, tag, tag_no_case, take_until},
        streaming::is_not,
    },
    character::complete::{
        alphanumeric1, digit0, digit1, line_ending, multispace0, multispace1, not_line_ending,
        one_of, space0, space1, tab,
    },
    combinator::{consumed, map, map_parser, map_res, opt, recognize, value},
    multi::{count, many0, many1, many1_count},
    number::complete::{double, float},
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult, Parser,
};
use tokens::{BoolLit, Expr, ExprAssign, ExprBlock, FloatLit, Ident, IntLit, Lit, Span, StrLit};

pub fn bom(input: Span) -> IResult<Span, bool> {
    alt((value(true, tag("\u{FEFF}")), value(false, tag("\u{FFFE}"))))(input)
}

pub fn version(input: Span) -> IResult<Span, u32> {
    terminated(
        preceded(
            tag("version"),
            preceded(
                space1,
                map_res(digit1, |span: Span| span.fragment().parse()),
            ),
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
                ext.fragment().ne(&"nothing").then_some(ext)
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
            map(opt(value(true, tag("//"))), |v| v.unwrap_or_default()),
            delimited(
                terminated(tag("{"), alt((space1, line_ending))),
                preceded(
                    many1(tab),
                    many0(map_parser(take_until("}"), |input| expr(input))),
                ),
                terminated(tag("}"), multispace0),
            ),
            opt(comment),
        )),
        |(doc, span, commented_out, values, comment, ..)| ExprBlock {
            r#type: &span,
            comment,
            values,
            name: None,
            extends: None,
            span,
            doc,
            commented_out,
        },
    )(input)
    .inspect_err(|e| {
        dbg!(e);
    })
}

fn expr(input: Span) -> IResult<Span, Expr> {
    alt((
        map(expr_block, Expr::from),
        map(expr_assign, Expr::from),
        map(lit, Expr::from),
    ))(input)
}

pub fn comment(input: Span) -> IResult<Span, Comment> {
    map(
        terminated(preceded(tag("//"), not_line_ending), many0(line_ending)),
        |value| Comment { value },
    )(input)
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
        terminated(
            tuple((
                map(opt(value(true, pair(tag("//"), space0))), |v| {
                    v.unwrap_or_default()
                }),
                separated_pair(
                    recognize(many1(permutation((opt(tag("_")), alphanumeric1)))),
                    tuple((space1, tag("="), multispace1)),
                    expr,
                ),
                opt(comment),
            )),
            many0(line_ending),
        ),
        |(commented_out, (left, value), comment)| ExprAssign {
            left: Ident {
                sym: &left,
                span: left,
            },
            right: Box::new(value),
            doc: None,
            comment,
            commented_out,
        },
    )(input)
}

fn valid_string(input: Span) -> IResult<Span, Span> {
    delimited(
        tag("\""),
        escaped(is_not("\"\\"), '\\', one_of(r#""\rnts"#)),
        tag("\""),
    )(input)
}

fn str_lit(input: Span) -> IResult<Span, StrLit<'_>> {
    map(terminated(valid_string, many0(line_ending)), |span| {
        StrLit { value: &span, span }
    })(input)
}
fn float_lit(input: Span) -> IResult<Span, FloatLit<'_>> {
    alt((
        map(
            terminated(consumed(double), many0(line_ending)),
            |(span, value)| FloatLit { value, span },
        ),
        map(
            terminated(consumed(float), many0(line_ending)),
            |(span, value)| FloatLit {
                value: value as f64,
                span,
            },
        ),
    ))(input)
}

fn int_lit(input: Span) -> IResult<Span, IntLit<'_>> {
    map(
        terminated(
            consumed(map_res(digit1, |span: Span| span.fragment().parse::<i64>())),
            many0(line_ending),
        ),
        |(span, value)| IntLit { value, span },
    )(input)
}

fn bool_lit(input: Span) -> IResult<Span, BoolLit<'_>> {
    map(
        terminated(
            consumed(alt((
                value(true, tag_no_case("true")),
                value(false, tag_no_case("false")),
            ))),
            many0(line_ending),
        ),
        |(span, value)| BoolLit { value, span },
    )(input)
}

// #[derive(Debug, Clone)]
// pub enum Value<'a> {
//     String(StrLit<'a>),
//     Integer(IntLit<'a>),
//     Float(FloatLit<'a>),
//     Bool(BoolLit<'a>),
//     Array(Arr<'a>),
//     Object(Object<'a>),
//     Block(ExprBlock<'a>),
// }

// impl<'a: 'b, 'b> Value<'b> {
//     pub fn set_comment(&mut self, value: Option<Comment<'a>>) {
//         match self {
//             Value::String(str_lit) => str_lit.comment = value,
//             Value::Integer(int_lit) => int_lit.comment = value,
//             Value::Float(float_lit) => float_lit.comment = value,
//             Value::Bool(bool_lit) => bool_lit.comment = value,
//             Value::Array(arr) => arr.comment = value,
//             Value::Object(object) => object.comment = value,
//             Value::Block(block) => block.comment = value,
//         };
//     }
//     pub fn set_name(&mut self, value: &'a str) {
//         match self {
//             Value::String(str_lit) => str_lit.name = value,
//             Value::Integer(int_lit) => int_lit.name = value,
//             Value::Float(float_lit) => float_lit.name = value,
//             Value::Bool(bool_lit) => bool_lit.name = value,
//             Value::Array(arr) => arr.name = value,
//             Value::Object(object) => object.name = value,
//             Value::Block(block) => block.r#type = value,
//         };
//     }

//     pub fn name(&self) -> &str {
//         match self {
//             Value::String(value) => value.name,
//             Value::Integer(value) => value.name,
//             Value::Float(value) => value.name,
//             Value::Bool(value) => value.name,
//             Value::Array(value) => value.name,
//             Value::Object(value) => value.name,
//             Value::Block(value) => value.r#type,
//         }
//     }
//     pub fn is_commented_out(&self) -> bool {
//         match self {
//             // Value::String(value) => value.is_commented_out,
//             // Value::Integer(value) => value.is_commented_out,
//             // Value::Float(value) => value.is_commented_out,
//             // Value::Bool(value) => value.is_commented_out,
//             Value::Array(value) => value.is_commented_out,
//             Value::Object(value) => value.is_commented_out,
//             // Value::Block(value) => value.is_commented_out,
//             _ => false,
//         }
//     }
//     pub fn comment(&self) -> Option<Comment> {
//         match self {
//             Value::String(value) => value.comment.clone(),
//             Value::Integer(value) => value.comment.clone(),
//             Value::Float(value) => value.comment.clone(),
//             Value::Bool(value) => value.comment.clone(),
//             Value::Array(value) => value.comment.clone(),
//             Value::Object(value) => value.comment.clone(),
//             Value::Block(value) => value.comment.clone(),
//         }
//     }
// }

// #[derive(Debug, Clone, Default)]
// pub struct Object<'a> {
//     pub name: &'a str,
//     pub r#type: Option<&'a str>,
//     pub value: IndexMap<&'a str, Value<'a>>,
//     pub comment: Option<Comment<'a>>,
//     pub is_commented_out: bool,
// }

// impl<'a: 'b, 'b> From<Object<'a>> for Value<'b> {
//     fn from(value: Object<'a>) -> Self {
//         Self::Object(value)
//     }
// }

// #[derive(Debug, Clone, Default)]
// pub struct Arr<'a> {
//     pub name: &'a str,
//     pub value: Vec<Value<'a>>,
//     pub comment: Option<Comment<'a>>,
//     pub is_commented_out: bool,
// }

// impl<'a: 'b, 'b> From<Arr<'a>> for Value<'b> {
//     fn from(value: Arr<'a>) -> Self {
//         Self::Array(value)
//     }
// }

#[derive(Debug, Clone)]
pub struct Comment<'a> {
    value: Span<'a>,
}

// impl<'a: 'b, 'b> From<StrLit<'a>> for Expr<'b> {
//     fn from(value: StrLit<'a>) -> Self {
//         Self::Lit(Lit::Str(value))
//     }
// }

// impl<'a: 'b, 'b> From<IntLit<'a>> for Expr<'b> {
//     fn from(value: IntLit<'a>) -> Self {
//         Self::Lit(Lit::Int(value))
//     }
// }

// impl<'a: 'b, 'b> From<FloatLit<'a>> for Expr<'b> {
//     fn from(value: FloatLit<'a>) -> Self {
//         Self::Lit(Lit::Float(value))
//     }
// }

// impl<'a: 'b, 'b> From<BoolLit<'a>> for Expr<'b> {
//     fn from(value: BoolLit<'a>) -> Self {
//         Self::Lit(Lit::Bool(value))
//     }
// }

// #[derive(Debug, Clone)]
// pub struct KeyValue<'a> {
//     key: &'a str,
//     value: Value<'a>,
//     children: Vec<KeyValue<'a>>,
//     doc: Option<Comment<'a>>,
//     comment: Option<Comment<'a>>,
//     is_commented_out: bool,
// }

// fn process<'a>(kv: &KeyValue<'a>, children: IndexMap<&'a str, Value<'a>>) -> Value<'a> {
//     let first = &children.values().next().unwrap();
//     let same_type = children.values().all(|v| {
//         matches!(
//             (first, v),
//             (
//                 Value::String(StrLit { name: old, .. }),
//                 Value::String(StrLit { name: new, .. })
//             ) | (
//                 Value::Integer(IntLit { name: old, .. }),
//                 Value::Integer(IntLit { name: new, .. })
//             ) | (
//                 Value::Float(FloatLit { name: old, .. }),
//                 Value::Float(FloatLit { name: new, .. })
//             ) | (
//                 Value::Bool(BoolLit { name: old, .. }),
//                 Value::Bool(BoolLit { name: new, .. })
//             ) | (Value::Array(Arr { name: old , .. }), Value::Array(Arr { name: new, .. }))
//                 | (Value::Object(Object { name: old ,.. }), Value::Object(Object { name: new,..}))
//                 | (Value::Block(ExprBlock {  r#type: old,..}), Value::Block(ExprBlock { r#type: new, ..}))
//                 if old == new
//         )
//     });

//     if same_type {
//         Value::Array(Arr {
//             name: kv.key,
//             value: children.into_values().collect(),
//             comment: kv.doc.clone(),
//             is_commented_out: kv.is_commented_out,
//         })
//     } else {
//         Value::Object(Object {
//             name: kv.key,
//             value: Default::default(),
//             comment: kv.doc.clone(),
//             is_commented_out: kv.is_commented_out,
//             ..Default::default()
//         })
//     }
// }

// impl<'a> KeyValue<'a> {
//     pub fn process(mut self) -> Value<'a> {
//         if !self.children.is_empty() {
//             let mut children: IndexMap<&'a str, Value<'a>> = IndexMap::new();

//             self.children.clone().into_iter().for_each(|kv| {
//                 match children.entry(kv.key) {
//                     Entry::Vacant(entry) => {
//                         entry.insert(kv.value);
//                     }
//                     Entry::Occupied(mut entry) => {
//                         let existing = entry.get_mut();
//                         if let Value::Array(ref mut arr) = existing {
//                             arr.value.push(kv.value);
//                         } else {
//                             let old = std::mem::replace(
//                                 existing,
//                                 Value::Array(Arr {
//                                     name: kv.key,
//                                     value: vec![],
//                                     comment: None,
//                                     is_commented_out: false,
//                                 }),
//                             );

//                             if let Value::Array(ref mut arr) = existing {
//                                 arr.is_commented_out = old.is_commented_out();
//                                 arr.value.push(old);
//                                 arr.value.push(kv.value);
//                             }
//                         }
//                     }
//                 };
//             });

//             process(&self, children)
//         } else {
//             self.value.set_name(self.key);
//             self.value
//         }
//     }
// }

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
