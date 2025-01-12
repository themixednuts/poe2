mod ast;

use crate::core::{
    bom, expr, extends, is_abstract,
    tokens::{Expr, Span},
    version,
};
use nom::{combinator::opt, multi::many0, sequence::Tuple, IResult};

pub fn parse_ao<'a>(input: Span<'a>) -> IResult<Span<'a>, AO<'a>> {
    (opt(bom), version, opt(is_abstract), extends, many0(expr))
        .parse(input)
        .map(|(input, (_, version, is_abstract, extends, blocks))| {
            (
                input,
                AO {
                    version,
                    is_abstract: is_abstract.unwrap_or_default(),
                    extends,
                    children: blocks,
                },
            )
        })
        .inspect(|e| {
            dbg!(&e);
        })
}

#[derive(Debug)]
pub struct AO<'a> {
    version: u32,
    is_abstract: bool,
    extends: Option<Span<'a>>,
    children: Vec<Expr<'a>>,
}

// impl AO<'_> {
//     pub fn visit(&self, visitor: &mut impl Visitor) {
//         visitor.visit_version(self.version);
//         visitor.visit_abstract(self.is_abstract);
//         visitor.visit_extends(self.extends);
//         for child in &self.children {
//             visitor.visit_block(child);
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::parse_ao;
    use crate::{core::tokens::Span, util::read_string_from_utf16};

    // #[test]
    // fn base() {
    //     let src = include_bytes!("../resources/aosetparent.ao");
    //     let src = read_string_from_utf16(src);

    //     let (input, ao) = parse_ao(&src).unwrap();
    //     dbg!(&ao);

    //     // assert_eq!(ao.version, 2);
    //     // assert_eq!(ao.is_abstract, true);
    //     // assert_eq!(ao.extends, None);
    //     // assert_eq!(ao.children.len(), 3);
    // }
    #[test]
    fn equipment() {
        let src = include_bytes!("../resources/equipment.it");
        let src = read_string_from_utf16(src);

        // let (input, ao) = parse_ao(Span::new(&src)).unwrap();
        // dbg!(&ao);

        // assert_eq!(ao.version, 2);
        // assert_eq!(ao.is_abstract, true);
        // assert_eq!(ao.extends, Some("Metadata/Items/Item"));
        // assert_eq!(ao.children.len(), 3);
    }
    #[test]
    fn character() {
        let src = include_bytes!("../resources/character.ao");
        let src = read_string_from_utf16(src);

        // let (input, ao) = parse_ao(Span::new(&src)).unwrap();
        // dbg!(&ao);

        // assert_eq!(ao.version, 2);
        // assert_eq!(ao.is_abstract, false);
        // assert_eq!(ao.extends, Some("Metadata/Parent"));
        // assert_eq!(ao.children.len(), 3);
    }
    #[test]
    fn character_aoc() {
        let src = include_bytes!("../resources/character.aoc");
        let src = read_string_from_utf16(src);

        // let (input, ao) = parse_ao(Span::new(&src, false)).unwrap();
        // dbg!(&ao);

        // assert_eq!(ao.version, 2);
        // assert_eq!(ao.is_abstract, false);
        // assert_eq!(ao.extends, Some("Metadata/Parent"));
        // assert_eq!(ao.children.len(), 3);
    }
    #[test]
    fn gravestone_aoc() {
        let src = include_bytes!("../resources/gravestoneamuletheld.aoc");
        let src = read_string_from_utf16(src);

        // assert_eq!(ao.version, 2);
        // assert_eq!(ao.is_abstract, false);
        // assert_eq!(ao.extends, Some("Metadata/Parent"));
        // assert_eq!(ao.children.len(), 3);
    }
}
