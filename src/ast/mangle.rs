use super::{CmdEvalArgs, GroupType, Node as ASTNode, VAN};
use delegate_attr::delegate;
use itertools::Itertools;

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

pub trait Mangle: Default {
    /// transform this AST into a byte string, outputs into `$f`
    fn fmt(&self, f: &mut Vec<u8>, escc: u8);

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
    fn fmt(&self, f: &mut Vec<u8>, escc: u8) {
        use ASTNode::*;
        match self {
            NullNode => {}
            Constant { data, .. } => f.extend_from_slice(&data[..]),
            Grouped { typ, elems } => {
                let parens = *typ == GroupType::Strict;
                if parens {
                    f.push(b'(');
                }
                elems.fmt(f, escc);
                if parens {
                    f.push(b')');
                }
            }
            Argument { indirection, index } => {
                f.extend(std::iter::repeat(b'$').take(indirection + 1));
                if let Some(i) = index {
                    f.extend_from_slice(i.to_string().as_bytes());
                }
            }
            CmdEval { cmd, args } => {
                f.push(escc);
                f.push(b'(');
                cmd.fmt(f, escc);
                args.fmt(f, escc);
                f.push(b')');
            }
            Lambda { argc, body } => {
                f.push(escc);
                f.extend_from_slice(b"(lambda ");
                f.extend_from_slice(argc.to_string().as_bytes());
                f.push(b' ');
                body.fmt(f, escc);
                f.push(b')');
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
                            } else if y == NullNode {
                                elems.clear();
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
                Constant { data, .. } if data.is_empty() => *this = NullNode,
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
    fn fmt(&self, f: &mut Vec<u8>, escc: u8) {
        for i in self {
            i.fmt(f, escc);
        }
    }

    #[inline]
    fn get_complexity(&self) -> usize {
        self.iter().map(Mangle::get_complexity).sum()
    }

    fn simplify(self) -> Self {
        let mut ret = VAN::with_capacity(self.len());
        let mut it = self.into_iter().map(Mangle::simplify);

        use ASTNode::*;
        let mut litem = match it.next() {
            Some(x) => x,
            None => return VAN::new(),
        };
        for mut citem in it {
            match (&mut litem, &mut citem) {
                (_, NullNode) => {}
                (
                    Constant {
                        non_space,
                        ref mut data,
                    },
                    Constant {
                        non_space: ins2,
                        data: ref y,
                    },
                ) if non_space == ins2 => {
                    data.extend_from_slice(&y[..]);
                }
                (
                    Grouped {
                        typ: GroupType::Dissolving,
                        ref mut elems,
                    },
                    Grouped {
                        typ: GroupType::Dissolving,
                        elems: ref mut y,
                    },
                ) => {
                    elems.append(y);
                }
                (a, b) => {
                    // ret <<- litem <- citem
                    ret.push(std::mem::replace(a, b.take()));
                }
            }
        }
        ret.push(litem);
        ret
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
    fn fmt(&self, f: &mut Vec<u8>, escc: u8) {
        for i in &self.0 {
            f.push(b' ');
            i.fmt(f, escc);
        }
    }

    fn simplify(self) -> Self {
        self.into_iter()
            .map(Mangle::simplify)
            .flat_map(|i| match i {
                ASTNode::NullNode => vec![],
                ASTNode::Grouped {
                    typ: GroupType::Dissolving,
                    elems,
                } => elems,
                _ => vec![i],
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
    x.into_iter()
        // 1. simplify
        .map(Mangle::simplify)
        // 1. inline non-strict groups
        .flat_map(|i| match i {
            ASTNode::NullNode => vec![],
            ASTNode::Grouped { typ, elems } if typ != GroupType::Strict => compact_toplevel(elems),
            _ => vec![i],
        })
        // 2. aggressive concat constant-after-constants
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
                        *non_space |= ins2;
                        data.extend_from_slice(&y[..]);
                        it.next();
                    }
                }
                Grouped {
                    typ: GroupType::Dissolving,
                    ref mut elems,
                } => {
                    while let Some(Grouped {
                        typ: GroupType::Dissolving,
                        elems: ref y,
                    }) = it.peek()
                    {
                        elems.extend_from_slice(&y[..]);
                        it.next();
                    }
                    return Some(std::mem::take(elems));
                }
                _ => {}
            }
            Some(vec![base])
        })
        .flatten()
        .collect()
}
