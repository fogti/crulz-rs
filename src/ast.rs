#[derive(Clone, Debug, PartialEq)]
pub enum ASTNode {
    NullNode,

    /// Constant: is_non_space, data
    Constant(bool, Vec<u8>),

    /// Grouped: is_strict, elems
    /// loose groups are created while replacing patterns
    Grouped(bool, Vec<ASTNode>),

    CmdEval(String, Vec<ASTNode>),
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
    pub fn as_constant(&self) -> Option<&Vec<u8>> {
        match &self {
            Constant(_, x) => Some(x),
            _ => None,
        }
    }
}
