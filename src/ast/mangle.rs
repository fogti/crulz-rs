use super::{CmdEvalArgs, GroupType, Lift as _, Node as ASTNode, VAN};
use bstr::ByteSlice;
use delegate_attr::delegate;
use itertools::Itertools;

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

pub trait Mangle: Default {
    /// transform this AST into a byte string
    fn to_vec(self, escc: u8) -> Vec<u8>;

    /// helper for [`Mangle::simplify`] and [`interp::eval`](crate::interp::eval)
    fn get_complexity(&self) -> usize;

    #[inline(always)]
    fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    #[inline]
    fn simplify_inplace(&mut self) {
        *self = self.take().simplify();
    }

    /// performs a cleanup of the AST, opposite of two lift_ast invocations
    fn simplify(self) -> Self;

    /// this function applies the 'args' to the AST
    ///
    /// this function is partially reentrant, e.g. it leaves `self` in a half-consistent
    /// state in case of failure, thus it may be desirable to `self.clone()` before applying
    /// this function
    ///
    /// # Return value
    /// * `Err(idx)`: the first applied index which wasn't present in 'args'
    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize>;

    /// helper function for `crate::ast::Node::curry_inplace`
    #[doc(hidden)]
    fn curry2_inplace(&mut self, args: &CmdEvalArgs);
}

impl Mangle for ASTNode {
    fn to_vec(self, escc: u8) -> Vec<u8> {
        use ASTNode::*;
        match self {
            NullNode => Vec::new(),
            Constant { data, .. } => data.into(),
            Grouped { typ, elems } => {
                let inner = elems.to_vec(escc);
                if typ == GroupType::Strict {
                    [b"(", &inner[..], b")"]
                        .iter()
                        .map(core::ops::Deref::deref)
                        .flat_map(ByteSlice::bytes)
                        .collect()
                } else {
                    inner
                }
            }
            Argument { indirection, index } => std::iter::repeat(b'$')
                .take(indirection + 1)
                .chain(index.map(|i| i.to_string()).iter().flat_map(|i| i.bytes()))
                .collect(),
            CmdEval { cmd, args } => {
                let mut ret = Vec::new();
                ret.push(escc);
                ret.push(b'(');
                ret.extend_from_slice(&cmd.to_vec(escc)[..]);
                ret.extend_from_slice(&args.to_vec(escc)[..]);
                ret.push(b')');
                ret
            }
            Lambda { argc, body } => {
                let mut ret = Vec::new();
                ret.push(escc);
                ret.extend_from_slice(b"(lambda ");
                ret.extend_from_slice(argc.to_string().as_bytes());
                ret.push(b' ');
                ret.extend_from_slice(&body.to_vec(escc)[..]);
                ret.push(b')');
                ret
            }
        }
    }

    fn get_complexity(&self) -> usize {
        use ASTNode::*;
        match &self {
            NullNode => 0,
            Argument { indirection, .. } => 3 + indirection,
            CmdEval { cmd, args } => 1 + cmd.get_complexity() + args.get_complexity(),
            Constant { data, .. } => 1 + data.len(),
            Grouped { typ, elems } => {
                (match *typ {
                    GroupType::Dissolving => 0,
                    GroupType::Loose => 1,
                    GroupType::Strict => 2,
                }) + elems.get_complexity()
            }
            Lambda { body, .. } => 2 + body.get_complexity(),
        }
    }

    fn simplify(mut self) -> Self {
        use ASTNode::*;
        crate::ast::while_cplx_changes(&mut self, |this| {
            match this {
                Grouped {
                    ref mut typ,
                    ref mut elems,
                } => {
                    match elems.len() {
                        0 => {
                            if *typ != GroupType::Strict {
                                *this = NullNode;
                                return false;
                            }
                        }
                        1 => {
                            let y = elems[0].take().simplify();
                            if *typ != GroupType::Strict {
                                *this = y;
                            } else if let Grouped {
                                typ: GroupType::Dissolving,
                                elems: z,
                            } = y
                            {
                                *elems = z;
                            } else {
                                // swap it back, omit clone
                                elems[0] = y;
                            }
                        }
                        _ => elems.simplify_inplace(),
                    }
                }
                CmdEval {
                    ref mut cmd,
                    ref mut args,
                } => {
                    cmd.simplify_inplace();
                    args.simplify_inplace();
                }
                Lambda { ref mut body, .. } => body.simplify_inplace(),
                _ => return false,
            }
            true
        });
        self
    }

    fn apply_arguments_inplace(&mut self, xargs: &CmdEvalArgs) -> Result<(), usize> {
        use ASTNode::*;
        match self {
            Argument {
                indirection: 0,
                index,
            } => {
                *self = match *index {
                    Some(index) => match xargs.0.get(index) {
                        Some(x) => x.clone(),
                        None => return Err(index),
                    },
                    None => Constant {
                        non_space: true,
                        data: vec![b'$'].into(),
                    },
                };
            }
            Argument {
                ref mut indirection,
                ..
            } => *indirection -= 1,

            Grouped { ref mut elems, .. } => elems.apply_arguments_inplace(xargs)?,
            CmdEval {
                ref mut cmd,
                ref mut args,
            } => {
                cmd.apply_arguments_inplace(xargs)?;
                args.apply_arguments_inplace(xargs)?;
            }
            Lambda { ref mut body, .. } => body.apply_arguments_inplace(xargs)?,
            _ => {}
        }
        Ok(())
    }

    #[doc(hidden)]
    fn curry2_inplace(&mut self, xargs: &CmdEvalArgs) {
        use ASTNode::*;
        match self {
            Argument {
                indirection: 0,
                index,
            } => {
                *self = match *index {
                    Some(index) => match xargs.0.get(index) {
                        Some(x) => x.clone(),
                        None => Argument {
                            indirection: 0,
                            index: Some(index - xargs.len()),
                        },
                    },
                    None => Constant {
                        non_space: true,
                        data: vec![b'$'].into(),
                    },
                };
            }

            Grouped { ref mut elems, .. } => elems.curry2_inplace(xargs),
            CmdEval {
                ref mut cmd,
                ref mut args,
            } => {
                cmd.curry2_inplace(xargs);
                args.curry2_inplace(xargs);
            }

            // ignore sub-lambdas
            _ => {}
        }
    }
}

impl Mangle for VAN {
    fn to_vec(self, escc: u8) -> Vec<u8> {
        self.into_iter().flat_map(|i| i.to_vec(escc)).collect()
    }

    #[inline]
    fn get_complexity(&self) -> usize {
        self.iter().map(Mangle::get_complexity).sum()
    }

    fn simplify(self) -> Self {
        self.into_iter()
            .map(Mangle::simplify)
            .filter(|i| match i {
                ASTNode::Grouped { typ, elems }
                    if elems.is_empty() && *typ != GroupType::Strict =>
                {
                    false
                }
                ASTNode::Constant { data, .. } if data.is_empty() => false,
                ASTNode::NullNode => false,
                _ => true,
            })
            .peekable()
            .batching(|it| {
                use ASTNode::*;
                let mut base = it.next()?;
                match &mut base {
                    Constant {
                        non_space,
                        ref mut data,
                    } => {
                        while let Some(Constant {
                            non_space: ins2,
                            data: ref y,
                        }) = it.peek()
                        {
                            if non_space != ins2 {
                                break;
                            }
                            data.extend_from_slice(&y[..]);
                            it.next();
                        }
                    }
                    Grouped {
                        typ: GroupType::Dissolving,
                        ref mut elems,
                    } => {
                        while let Some(Grouped { typ, elems: ref y }) = it.peek() {
                            if *typ != GroupType::Dissolving {
                                break;
                            }
                            elems.extend_from_slice(&y[..]);
                            it.next();
                        }
                        return Some(std::mem::take(elems));
                    }
                    _ => {}
                }
                Some(base.lift_ast())
            })
            .flatten()
            .collect()
    }

    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize> {
        for i in self.iter_mut() {
            i.apply_arguments_inplace(args)?;
        }
        Ok(())
    }

    fn curry2_inplace(&mut self, args: &CmdEvalArgs) {
        for i in self.iter_mut() {
            i.curry2_inplace(args);
        }
    }
}

impl Mangle for CmdEvalArgs {
    fn to_vec(self, escc: u8) -> Vec<u8> {
        self.0.into_iter().fold(Vec::new(), |mut acc, i| {
            acc.push(b' ');
            acc.extend_from_slice(&i.to_vec(escc)[..]);
            acc
        })
    }

    fn simplify(self) -> Self {
        self.into_iter()
            .map(|i| i.simplify())
            .flat_map(|i| {
                if let ASTNode::Grouped {
                    typ: GroupType::Dissolving,
                    elems,
                } = i
                {
                    elems
                } else {
                    i.lift_ast()
                }
            })
            .collect()
    }

    #[delegate(self.0)]
    fn get_complexity(&self) -> usize {}

    #[delegate(self.0)]
    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize> {}

    #[delegate(self.0)]
    fn curry2_inplace(&mut self, args: &CmdEvalArgs) {}
}

pub fn compact_toplevel(x: VAN) -> VAN {
    // we are at the top level, wo can inline non-strict groups
    // and then put all constants heaps into single constants
    x.simplify()
        .into_iter()
        // 1. inline non-strict groups
        .flat_map(|i| match i {
            ASTNode::NullNode => vec![],
            ASTNode::Grouped { typ, elems } if typ != GroupType::Strict => compact_toplevel(elems),
            _ => vec![i],
        })
        // 2. aggressive concat constant-after-constants
        .peekable()
        .batching(|it| {
            let mut ret = it.next()?;
            if let ASTNode::Constant {
                non_space: ref mut rnsp,
                data: ref mut rdat,
            } = &mut ret
            {
                while let Some(ASTNode::Constant {
                    non_space: nsp,
                    data: ref dat,
                }) = it.peek()
                {
                    *rnsp |= nsp;
                    rdat.extend_from_slice(&dat[..]);
                    it.next();
                }
            }
            Some(ret)
        })
        .collect()
}
