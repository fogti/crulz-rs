use crate::hlparser::{ASTNode, MangleAST};
use std::{borrow::Cow, collections::HashMap};

type VAN = Vec<ASTNode>;

#[derive(Clone)]
enum InterpValue {
    BuiltIn(fn(&VAN, &mut EvalContext) -> Option<VAN>),
    Data(usize, VAN),
}

type DefinesMap = HashMap<Cow<'static, str>, InterpValue>;

struct EvalContext {
    defs: DefinesMap,
}

mod builtin {
    use super::*;
    pub(super) fn def(args: &VAN, ctx: &mut EvalContext) -> Option<VAN> {
        None
    }
}

fn register_builtin_(
    defs: &mut DefinesMap,
    name: &'static str,
    fnx: fn(&VAN, &mut EvalContext) -> Option<VAN>,
) {
    defs.insert(Cow::from(name), InterpValue::BuiltIn(fnx));
}

macro_rules! register_builtin {
    ($defs:ident, $name:expr, $fn:ident) => {
        register_builtin_(&mut $defs, $name, builtin::$fn);
    };
}

impl EvalContext {
    fn new() -> Self {
        let mut defs = DefinesMap::new();
        register_builtin!(defs, "def", def);
        Self { defs }
    }
}

fn eval_cmd(val: InterpValue, args: &VAN, mut ctx: &mut EvalContext) -> Option<VAN> {
    use InterpValue::*;
    match val {
        BuiltIn(x) => x(&args, &mut ctx),
        Data(0, x) => Some(x),
        Data(n, x) => {
            if args.len() < n {
                return None;
            }
            let mut tmp = x;
            for i in (0..n).rev() {
                tmp.replace(format!("${}", i).as_bytes(), &args[i].clone().eval(ctx));
            }
            Some(tmp)
        }
    }
}

trait Eval {
    fn eval(self, ctx: &mut EvalContext) -> VAN;
}

impl Eval for ASTNode {
    fn eval(self, mut ctx: &mut EvalContext) -> VAN {
        if let ASTNode::CmdEval(cmd, args) = &self {
            if let Some(x) = ctx.defs.get(&Cow::from(cmd)) {
                if let Some(y) = eval_cmd(x.clone(), args, &mut ctx) {
                    return y;
                }
            }
        }
        vec![self]
    }
}

impl Eval for VAN {
    fn eval(self, mut ctx: &mut EvalContext) -> VAN {
        self.into_iter()
            .map(|x| x.eval(&mut ctx))
            .flatten()
            .collect()
    }
}

pub fn eval(data: VAN) -> VAN {
    let mut ctx = EvalContext::new();
    data.eval(&mut ctx)
}
