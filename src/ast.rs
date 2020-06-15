use delegate_attr::delegate;
use serde::{Deserialize, Serialize};

pub type Atom = crate::crulst::CrulzAtom;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GroupType {
    Strict,
    Loose,
    Dissolving,
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
    pub(crate) fn is_space(&self) -> bool {
        match self {
            NullNode | Constant(false, _) => true,
            _ => false,
        }
    }

    #[inline(always)]
    pub(crate) fn as_constant(&self) -> Option<&Atom> {
        match self {
            Constant(_, ref x) => Some(x),
            _ => None,
        }
    }

    pub(crate) fn conv_to_constant(&self) -> Option<Atom> {
        Some(match self {
            ASTNode::Constant(_, x) => x.clone(),
            ASTNode::Grouped(gt, x) if *gt != GroupType::Strict => {
                let mut impc = x.iter().map(ASTNode::conv_to_constant);
                if x.len() == 1 {
                    impc.next().unwrap()?
                } else {
                    impc.try_fold(String::new(), |acc, i| Some(acc + i.as_ref()?))?
                        .into()
                }
            }
            _ => return None,
        })
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
        ASTNode::Grouped(GroupType::Dissolving, self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CmdEvalArgs(pub VAN);

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
    /// constructs `CmdEvalArgs` from a `VAN` with white-space as arguments delimiter
    pub fn from_wsdelim(args: VAN) -> Self {
        use crate::mangle_ast::MangleAST;
        use itertools::Itertools;
        args.into_iter()
            .group_by(ASTNode::is_space)
            .into_iter()
            .filter(|(d, _)| !*d)
            .map(|(_, i)| i.collect::<VAN>().lift_ast().simplify())
            .collect()
    }
}

#[delegate(self.0)]
impl CmdEvalArgs {
    pub fn iter(&self) -> std::slice::Iter<ASTNode>;
    pub fn iter_mut(&mut self) -> std::slice::IterMut<ASTNode>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args2unspaced() {
        use ASTNode::*;
        assert_eq!(
            CmdEvalArgs::from_wsdelim(vec![
                Constant(true, "a".into()),
                Constant(false, "a".into()),
                Constant(true, "a".into()),
                Constant(true, "a".into()),
                Constant(false, "a".into())
            ]),
            CmdEvalArgs(vec![
                Constant(true, "a".into()),
                Constant(true, "aa".into())
            ])
        );
    }
}
