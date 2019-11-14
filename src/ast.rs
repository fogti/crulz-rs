use delegate::delegate;
use serde::{Deserialize, Serialize};

pub type Atom = crate::crulst::CrulzAtom;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GroupType {
    Strict,
    Loose,
    Dissolving,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CmdEvalArgs(pub Vec<ASTNode>);

impl std::iter::IntoIterator for CmdEvalArgs {
    type Item = ASTNode;
    type IntoIter = std::vec::IntoIter<ASTNode>;

    #[inline(always)]
    fn into_iter(self) -> std::vec::IntoIter<ASTNode> {
        self.0.into_iter()
    }
}

impl std::iter::FromIterator<ASTNode> for CmdEvalArgs {
    #[inline(always)]
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = ASTNode>,
    {
        CmdEvalArgs(Vec::from_iter(iter))
    }
}

impl CmdEvalArgs {
    delegate! {
        target self.0 {
            pub fn iter(&self) -> std::slice::Iter<ASTNode>;
            pub fn iter_mut(&mut self) -> std::slice::IterMut<ASTNode>;
            pub fn len(&self) -> usize;
            pub fn is_empty(&self) -> bool;
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ASTNode {
    NullNode,

    /// Constant: is_non_space, data
    Constant(bool, Atom),

    /// Argument: indirection (= (count of '$'s) - 1), index
    /// no given index means something like '$$.'
    Argument {
        indirection: usize,
        index: Option<usize>,
    },

    /// Grouped: type, elems
    /// loose groups are created while replacing patterns
    Grouped(GroupType, Vec<ASTNode>),

    /// CmdEval: cmd, args
    CmdEval(Vec<ASTNode>, CmdEvalArgs),
}

use ASTNode::*;
pub type VAN = Vec<ASTNode>;

impl std::default::Default for ASTNode {
    #[inline(always)]
    fn default() -> Self {
        NullNode
    }
}

impl ASTNode {
    #[inline(always)]
    pub(crate) fn as_constant(&self) -> Option<&Atom> {
        match &self {
            Constant(_, ref x) => Some(x),
            _ => None,
        }
    }
}
