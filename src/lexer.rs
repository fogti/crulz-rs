extern crate readfilez;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LowerLexerToken {
    Constant(bool, u8),
    // Paren ? opening : closing
    Paren(bool),
    Escape(u8),
}

use LowerLexerToken::*;

impl LowerLexerToken {
    #[inline]
    pub fn is_space(self) -> bool {
        match self {
            Constant(false, _) => true,
            _ => false,
        }
    }
}

impl Into<u8> for LowerLexerToken {
    fn into(self) -> u8 {
        match self {
            Constant(_, x) | Escape(x) => x,
            Paren(true) => 40,
            Paren(false) => 41,
        }
    }
}

impl Into<u8> for &LowerLexerToken {
    #[inline]
    fn into(self) -> u8 {
        Into::<u8>::into(*self)
    }
}

pub fn lex(input: &[u8], escc: u8) -> Vec<LowerLexerToken> {
    use rayon::prelude::*;
    input
        .par_iter()
        .map(|&i| match i {
            _ if i == escc => Escape(escc),
            9 | 10 | 11 | 12 | 13 | 32 => Constant(false, i),
            40 => Paren(true),  // '('
            41 => Paren(false), // ')'
            _ => Constant(true, i),
        })
        .collect()
}
