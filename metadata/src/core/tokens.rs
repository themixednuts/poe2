use super::Comment;
use nom_locate::LocatedSpan;

pub(crate) type Span<'a> = LocatedSpan<&'a str>;

#[derive(Debug, Clone)]
pub(crate) enum Expr<'a> {
    Block(ExprBlock<'a>),
    Assign(ExprAssign<'a>),
    Lit(Lit<'a>),
}

#[derive(Debug, Clone)]
pub(crate) struct Ident<'a> {
    pub sym: &'a str,
    pub span: Span<'a>,
}

#[derive(Debug, Clone)]
pub(crate) enum Lit<'a> {
    Str(StrLit<'a>),
    Int(IntLit<'a>),
    Float(FloatLit<'a>),
    Bool(BoolLit<'a>),
}

impl<'a> From<Lit<'a>> for Expr<'a> {
    fn from(value: Lit<'a>) -> Self {
        Expr::Lit(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StrLit<'a> {
    pub value: &'a str,
    pub span: Span<'a>,
}

impl<'a> From<StrLit<'a>> for Lit<'a> {
    fn from(value: StrLit<'a>) -> Self {
        Self::Str(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IntLit<'a> {
    pub value: i64,
    pub span: Span<'a>,
}

impl<'a> From<IntLit<'a>> for Lit<'a> {
    fn from(value: IntLit<'a>) -> Self {
        Self::Int(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FloatLit<'a> {
    pub value: f64,
    pub span: Span<'a>,
}

impl<'a> From<FloatLit<'a>> for Lit<'a> {
    fn from(value: FloatLit<'a>) -> Self {
        Self::Float(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BoolLit<'a> {
    pub value: bool,
    pub span: Span<'a>,
}

impl<'a> From<BoolLit<'a>> for Lit<'a> {
    fn from(value: BoolLit<'a>) -> Self {
        Self::Bool(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExprBlock<'a> {
    pub name: Option<StrLit<'a>>,
    pub r#type: &'a str,
    pub extends: Option<&'a str>,
    pub doc: Option<Comment<'a>>,
    pub comment: Option<Comment<'a>>,
    pub values: Vec<Expr<'a>>,
    pub span: Span<'a>,
    pub commented_out: bool,
}

impl<'a> From<ExprBlock<'a>> for Expr<'a> {
    fn from(value: ExprBlock<'a>) -> Self {
        Expr::Block(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExprAssign<'a> {
    pub left: Ident<'a>,
    pub right: Box<Expr<'a>>,
    pub doc: Option<Comment<'a>>,
    pub comment: Option<Comment<'a>>,
    pub commented_out: bool,
}

impl<'a> From<ExprAssign<'a>> for Expr<'a> {
    fn from(value: ExprAssign<'a>) -> Self {
        Expr::Assign(value)
    }
}
