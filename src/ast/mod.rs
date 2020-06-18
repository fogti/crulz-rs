use delegate_attr::delegate;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

mod mangle;
mod tests;

pub use mangle::{Mangle, MangleExt};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum GroupType {
    Strict,
    Loose,
    /// dissolving groups are created while replacing patterns
    Dissolving,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Node {
    NullNode,

    Constant {
        non_space: bool,
        data: bstr::BString,
    },

    Argument {
        /// `= (count of '$'s) - 1`
        indirection: usize,
        /// no given index means something like '$$.'
        index: Option<usize>,
    },

    Grouped {
        typ: GroupType,
        elems: Vec<Node>,
    },

    CmdEval {
        cmd: Vec<Node>,
        args: CmdEvalArgs,
    },
}

use Node::*;
pub type VAN = Vec<Node>;

impl std::default::Default for Node {
    #[inline(always)]
    fn default() -> Self {
        NullNode
    }
}

impl Node {
    #[inline(always)]
    pub(crate) fn is_space(&self) -> bool {
        match self {
            NullNode
            | Constant {
                non_space: false, ..
            } => true,
            _ => false,
        }
    }

    #[inline(always)]
    pub(crate) fn as_constant(&self) -> Option<&[u8]> {
        match self {
            Constant { ref data, .. } => Some(data.as_slice()),
            _ => None,
        }
    }

    pub(crate) fn conv_to_constant(&self) -> Option<Cow<'_, [u8]>> {
        match self {
            Node::Constant { ref data, .. } => Some((&**data).into()),
            Node::Grouped { typ, elems } if *typ != GroupType::Strict => {
                let mut impc = elems.iter().map(Node::conv_to_constant);
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

#[doc(hidden)]
pub trait Lift {
    type LiftT: Lift;

    /// lift the AST one level up (Node -> VAN || VAN -> Node),
    /// used as helper for [`Mangle::simplify_inplace`] and others
    /// to convert to the appropriate datatype
    fn lift_ast(self) -> Self::LiftT;
}

impl Lift for Node {
    type LiftT = VAN;

    #[inline(always)]
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }
}

impl Lift for VAN {
    type LiftT = Node;

    #[inline(always)]
    fn lift_ast(self) -> Self::LiftT {
        Node::Grouped {
            typ: GroupType::Dissolving,
            elems: self,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CmdEvalArgs(pub VAN);

impl std::iter::IntoIterator for CmdEvalArgs {
    type Item = Node;
    type IntoIter = std::vec::IntoIter<Node>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl std::iter::FromIterator<Node> for CmdEvalArgs {
    #[inline(always)]
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Node>,
    {
        Self(Vec::from_iter(iter))
    }
}

impl CmdEvalArgs {
    /// constructs `CmdEvalArgs` from a `VAN` with white-space as arguments delimiter
    pub fn from_wsdelim(args: VAN) -> Self {
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
    pub fn iter(&self) -> std::slice::Iter<Node> { }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<Node> { }
    pub fn len(&self) -> usize { }
    pub fn is_empty(&self) -> bool { }
}
