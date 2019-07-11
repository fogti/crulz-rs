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

#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    // both coords are zero-based
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexerToken {
    pub llt: LowerLexerToken,
    pub pos: Position,
}

impl LexerToken {
    #[inline]
    pub fn is_space(&self) -> bool {
        self.llt.is_space()
    }
}

pub fn lex(input: &[u8], escc: u8) -> Vec<LexerToken> {
    use rayon::prelude::*;
    let mut pos = Position { line: 0, col: 0 };
    let mut is_fi = true;
    let mut got_nl = false;
    input
        .par_iter()
        .map(|&i| match i {
            _ if i == escc => Escape(escc),
            9 | 10 | 11 | 12 | 13 | 32 => Constant(false, i),
            40 => Paren(true),  // '('
            41 => Paren(false), // ')'
            _ => Constant(true, i),
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|llt| {
            if is_fi {
                // don't increment pos.col at the beginning of the file
                is_fi = false;
            } else if got_nl {
                pos.col = 0;
                pos.line += 1;
                got_nl = false;
            } else {
                pos.col += 1;
            }
            if Constant(false, 10) == llt {
                got_nl = true;
            }
            LexerToken { llt, pos: pos.clone() }
        })
        .collect()
}
