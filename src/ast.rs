use serde::{Deserialize, Serialize};

pub type Atom = crate::crulst::CrulzAtom;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ASTNode {
    NullNode,

    /// Constant: is_non_space, data
    Constant(bool, Atom),

    /// Grouped: is_strict, elems
    /// loose groups are created while replacing patterns
    Grouped(bool, Vec<ASTNode>),

    /// CmdEval: cmd, args
    CmdEval(Vec<ASTNode>, Vec<ASTNode>),
}

use ASTNode::*;
pub type VAN = Vec<ASTNode>;

impl std::default::Default for ASTNode {
    #[inline]
    fn default() -> Self {
        NullNode
    }
}

impl ASTNode {
    pub fn as_constant(&self) -> Option<&Atom> {
        match &self {
            Constant(_, ref x) => Some(x),
            _ => None,
        }
    }
}
