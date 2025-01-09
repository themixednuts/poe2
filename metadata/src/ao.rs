use nom::{
    bytes::complete::tag,
    character::complete::{line_ending, multispace1, newline},
    combinator::opt,
    multi::{many0, many1, separated_list0},
    sequence::Tuple,
    IResult,
};

use crate::parser::{block, extends, is_abstract, version, Block};

pub fn parse_ao(input: &str) -> IResult<&str, AO> {
    (version, opt(is_abstract), extends, many0(block))
        .parse(input)
        .map(|(input, (version, is_abstract, extends, blocks))| {
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
    extends: Option<&'a str>,

    children: Vec<Block<'a>>,
}

#[cfg(test)]
mod tests {
    use super::parse_ao;
    use crate::util::read_string_from_utf16;

    #[test]
    fn base() {
        // let src = include_bytes!("../resources/aosetparent.ao");

        let src = std::fs::read("D:/Projects/poe2/metadata/resources/aosetparent.ao").unwrap();
        let src = read_string_from_utf16(&src);

        let (input, ao) = parse_ao(&src).unwrap();
        dbg!(&ao);

        assert_eq!(ao.version, 2);
        assert_eq!(ao.is_abstract, true);
        assert_eq!(ao.extends, None);
        assert_eq!(ao.children.len(), 3);
    }
    #[test]
    fn equipment() {
        let src = include_bytes!("../resources/equipment.it");
        let src = read_string_from_utf16(src);

        let (input, ao) = parse_ao(&src).unwrap();
        dbg!(&ao);

        assert_eq!(ao.version, 2);
        assert_eq!(ao.is_abstract, true);
        assert_eq!(ao.extends, Some("Metadata/Items/Item"));
        assert_eq!(ao.children.len(), 3);
    }
    #[test]
    fn character() {
        let src = include_bytes!("../resources/character.ao");
        let src = read_string_from_utf16(src);

        let (input, ao) = parse_ao(&src).unwrap();
        dbg!(&ao);

        assert_eq!(ao.version, 2);
        assert_eq!(ao.is_abstract, false);
        assert_eq!(ao.extends, Some("Metadata/Parent"));
        // assert_eq!(ao.children.len(), 3);
    }
}
