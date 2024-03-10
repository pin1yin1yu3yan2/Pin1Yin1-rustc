use pin1yin1_parser::*;

pub mod expr;
pub mod syntax;
pub mod types;

#[derive(Debug, Clone)]
pub struct Ident<'s> {
    pub ident: Token<'s, String>,
}

impl ParseUnit for Ident<'_> {
    type Target<'t> = Ident<'t>;

    fn parse<'s>(p: &mut Parser<'s>) -> ParseResult<'s, Self> {
        let ident = p.parse::<String>()?;
        let Some(start_char) = ident.chars().next() else {
            return ident.throw("empty ident!");
        };

        if !(start_char.is_alphabetic() || start_char == '_') {
            return ident.throw("bad ident");
        }
        p.finish(Ident { ident })
    }
}