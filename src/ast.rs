use delegate_attr::delegate;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum GroupType {
    Strict,
    Loose,
    /// dissolving groups are created while replacing patterns
    Dissolving,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ASTNode {
    NullNode,

    Constant(bstr::BString),

    Argument {
        /// `= (count of '$'s) - 1`
        indirection: usize,
        /// no given index means something like '$$.'
        index: Option<usize>,
    },

    Grouped {
        typ: GroupType,
        elems: Vec<ASTNode>,
    },

    CmdEval {
        cmd: Vec<ASTNode>,
        args: CmdEvalArgs,
    },
}

use ASTNode::*;
pub type VAN = Vec<ASTNode>;

impl std::default::Default for ASTNode {
    #[inline(always)]
    fn default() -> Self {
        NullNode
    }
}

pub(crate) fn is_space(data: &[u8]) -> bool {
    data.iter().all(u8::is_ascii_whitespace)
}

impl ASTNode {
    #[inline(always)]
    pub(crate) fn is_space(&self) -> bool {
        match self {
            NullNode => true,
            Constant(data) => is_space(&data[..]),
            _ => false,
        }
    }

    #[inline(always)]
    pub(crate) fn as_constant(&self) -> Option<&[u8]> {
        match self {
            NullNode => Some(&[]),
            Constant(ref data) => Some(data.as_slice()),
            _ => None,
        }
    }

    pub(crate) fn conv_to_constant(&self) -> Option<Cow<'_, [u8]>> {
        match self {
            ASTNode::Constant(ref data) => Some((&**data).into()),
            ASTNode::Grouped { typ, elems } if *typ != GroupType::Strict => {
                let mut impc = elems.iter().map(ASTNode::conv_to_constant);
                if elems.len() == 1 {
                    impc.next().unwrap()
                } else if impc.clone().any(|i| i.is_none()) {
                    None
                } else {
                    use bstr::ByteSlice;
                    let impc: Vec<_> = impc.map(Option::unwrap).collect();
                    Some(
                        impc.iter()
                            .map(|i| i.as_ref().bytes())
                            .flatten()
                            .collect::<Vec<_>>()
                            .into(),
                    )
                }
            }
            _ => None,
        }
    }
}

pub trait LiftAST {
    type LiftT: LiftAST;

    // lift the AST one level up (ASTNode -> VAN || VAN -> ASTNode),
    // used as helper for MangleAST::simplify_inplace and others
    // to convert to the appropriate datatype
    fn lift_ast(self) -> Self::LiftT;
}

impl LiftAST for ASTNode {
    type LiftT = VAN;

    #[inline(always)]
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }
}

impl LiftAST for VAN {
    type LiftT = ASTNode;

    #[inline(always)]
    fn lift_ast(self) -> Self::LiftT {
        Grouped {
            typ: GroupType::Dissolving,
            elems: self,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CmdEvalArgs(pub VAN);

impl std::iter::IntoIterator for CmdEvalArgs {
    type Item = ASTNode;
    type IntoIter = std::vec::IntoIter<ASTNode>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl std::iter::FromIterator<ASTNode> for CmdEvalArgs {
    #[inline(always)]
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = ASTNode>,
    {
        Self(Vec::from_iter(iter))
    }
}

impl CmdEvalArgs {
    /// constructs `CmdEvalArgs` from a `VAN` with white-space as arguments delimiter
    pub fn from_wsdelim(args: VAN) -> Self {
        use crate::mangle_ast::MangleAST;
        use itertools::Itertools;
        args.into_iter()
            .peekable()
            .batching(|it| {
                let mut ret = loop {
                    let tmp = it.next()?;
                    if !tmp.is_space() {
                        break vec![tmp];
                    }
                };
                while let Some(tmp) = it.peek() {
                    if tmp.is_space() {
                        break;
                    }
                    ret.push(it.next().unwrap());
                }
                Some(ret.lift_ast().simplify())
            })
            .collect()
    }
}

#[delegate(self.0)]
#[rustfmt::skip]
impl CmdEvalArgs {
    pub fn iter(&self) -> std::slice::Iter<ASTNode> { }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<ASTNode> { }
    pub fn len(&self) -> usize { }
    pub fn is_empty(&self) -> bool { }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args2unspaced() {
        use ASTNode::*;
        assert_eq!(
            CmdEvalArgs::from_wsdelim(vec![
                Constant(b"a".to_vec().into()),
                Constant(b" ".to_vec().into()),
                Constant(b"a".to_vec().into()),
                Constant(b"a".to_vec().into()),
                Constant(b"  ".to_vec().into())
            ]),
            CmdEvalArgs(vec![
                Constant(b"a".to_vec().into()),
                Constant(b"aa".to_vec().into())
            ])
        );
    }
}
