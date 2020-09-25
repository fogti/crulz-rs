use delegate_attr::delegate;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

mod mangle;
mod tests;

pub use mangle::{compact_toplevel, Mangle};

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

    Argument {
        /// `= (count of '$'s) - 1`
        indirection: usize,
        /// no given index means something like '$$.'
        index: Option<usize>,
    },

    CmdEval {
        cmd: Vec<Node>,
        args: CmdEvalArgs,
    },

    Constant {
        non_space: bool,
        data: bstr::BString,
    },

    Grouped {
        typ: GroupType,
        elems: Vec<Node>,
    },

    Lambda {
        argc: usize,
        body: Box<Node>,
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

    /// this function applies the 'args' to the AST,
    /// and shifts all non-evaluated args "to the left" with `n = args.len()`
    ///
    /// as this function shifts remaining `Argument`s, it is not reentrant, thus
    /// a second application will further modify the remaining `Argument`s, you
    /// probably don't want this.
    ///
    /// if the remaining AST should be finally called, [`Mangle::apply_arguments_inplace`]
    /// should be called, even if no applicable arguments remain, to
    /// * check if any non-indirect `Argument`s are left over
    /// * reduce the indirection count of all remaining `Argument`s
    pub fn curry_inplace(&mut self, xargs: &CmdEvalArgs) {
        if let Node::Lambda {
            ref mut argc,
            ref mut body,
        } = self
        {
            if *argc != 0 {
                *argc = argc.saturating_sub(xargs.len());
                body.curry2_inplace(xargs);
            }
        } else {
            self.curry2_inplace(xargs);
        }
    }
}

pub fn while_cplx_changes<F, T>(data: &mut T, mut f: F)
where
    F: FnMut(&mut T) -> bool,
    T: Mangle,
{
    let mut cplx = data.get_complexity();
    while f(data) {
        let new_cplx = data.get_complexity();
        if new_cplx == cplx {
            break;
        }
        cplx = new_cplx;
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
        let mut ret = VAN::new();
        let mut it = args.into_iter();

        'outer: loop {
            let mut rpart = loop {
                if let Some(x) = it.next() {
                    if !x.is_space() {
                        break vec![x];
                    }
                } else {
                    break 'outer;
                }
            };
            while let Some(x) = it.next() {
                if x.is_space() {
                    // discard $x implicitly, but this doesn't matter
                    // bc the loop above would skip it in the next round
                    // regardless of that
                    break;
                }
                rpart.push(x);
            }
            let mut res = rpart.lift_ast().simplify();
            if let Node::Grouped { ref mut typ, .. } = res {
                if *typ == GroupType::Dissolving {
                    // fix splitting of white-space separated arguments
                    // bc dissolving would be inlined and expanded, we don't want that
                    *typ = GroupType::Loose;
                }
            }
            ret.push(res);
        }
        CmdEvalArgs(ret)
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
