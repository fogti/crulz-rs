use crate::{
    ast::{ASTNode, Atom, CmdEvalArgs, GroupType, VAN},
    mangle_ast::MangleAST,
    parser::ParserOptions,
};
#[cfg(feature = "compile")]
use anyhow::Context;
use cfg_if::cfg_if;
use phf::phf_map;
use std::collections::hash_map::HashMap;

enum BuiltInFn {
    Manual(fn(&mut VAN, &mut EvalContext) -> Option<ASTNode>),
    Automatic(fn(VAN) -> Option<ASTNode>),
}

type DefinesMap = HashMap<Atom, (usize, ASTNode)>;
type CompilatesMap<'a> = HashMap<&'a str, &'a str>;

struct EvalContext<'a> {
    defs: DefinesMap,
    opts: ParserOptions,
    #[cfg(feature = "compile")]
    comp_map: &'a CompilatesMap<'a>,
    #[cfg(not(feature = "compile"))]
    comp_map: std::marker::PhantomData<&'a str>,
}

#[cfg(feature = "compile")]
impl EvalContext<'_> {
    fn load_from_compfile(&mut self, compf: &str) -> Result<VAN, anyhow::Error> {
        let fh = readfilez::read_from_file(std::fs::File::open(compf))
            .with_context(|| format!("Unable to open compfile '{}'", compf))?;
        let mut z = flate2::read::DeflateDecoder::new(fh.as_slice());
        let content: VAN = bincode::deserialize_from(&mut z)
            .with_context(|| format!("Unable to read compfile '{}'", compf))?;
        let ins_defs: DefinesMap = bincode::deserialize_from(&mut z)
            .with_context(|| format!("Unable to read compfile '{}'", compf))?;
        self.defs.extend(ins_defs.into_iter());
        Ok(content)
    }

    #[cold]
    fn save_to_compfile(&self, compf: &str, content: &VAN) -> Result<(), anyhow::Error> {
        let fh = std::fs::File::create(compf)
            .with_context(|| format!("Failed to create compfile '{}'", compf))?;
        let mut z = flate2::write::DeflateEncoder::new(fh, flate2::Compression::default());
        bincode::serialize_into(&mut z, content)
            .with_context(|| format!("Failed to write compfile '{}'", compf))?;
        bincode::serialize_into(&mut z, &self.defs)
            .with_context(|| format!("Failed to write compfile '{}'", compf))?;
        Ok(())
    }
}

fn conv_to_constant(arg: &ASTNode) -> Option<Atom> {
    Some(match arg {
        ASTNode::Constant(_, x) => x.clone(),
        ASTNode::Grouped(gt, x) if *gt != GroupType::Strict => {
            let mut impc = x.iter().map(conv_to_constant);
            if x.len() == 1 {
                impc.next().unwrap()?
            } else {
                impc.try_fold(String::new(), |acc, i| i.map(|i| acc + &i))?
                    .into()
            }
        }
        _ => return None,
    })
}

fn eval_foreach(
    mut elems: impl Iterator<Item = CmdEvalArgs>,
    fecmd: &ASTNode,
    ctx: &mut EvalContext<'_>,
) -> Option<ASTNode> {
    Some(
        if if let ASTNode::Constant(is_dat, _) = &fecmd {
            debug_assert!(is_dat);
            true
        } else {
            false
        } {
            // construct a function call
            let mut tmp_cmd = vec![fecmd.clone()];
            elems.fold(Vec::new(), |mut acc, mut tmp_args| {
                acc.push(
                    if let Some(x) = eval_cmd(&mut tmp_cmd, &mut tmp_args, ctx) {
                        x
                    } else {
                        ASTNode::CmdEval(tmp_cmd.clone(), tmp_args)
                    },
                );
                acc
            })
        } else {
            elems.try_fold(Vec::new(), |mut acc, i| {
                let mut cur: ASTNode = fecmd.clone();
                cur.apply_arguments_inplace(&i).ok()?;
                cur.eval(ctx);
                acc.push(cur);
                Some(acc)
            })?
        }
        .lift_ast(),
    )
}

macro_rules! define_blti {
    (($args:pat | $ac:expr, $ctx:pat) $body:tt) => {{
        fn blti($args: &mut VAN, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body
        (Some($ac), BuiltInFn::Manual(blti))
    }};
    (($args:pat | $ac:expr) $body:tt) => {{
        fn blti($args: VAN) -> Option<ASTNode> $body
        (Some($ac), BuiltInFn::Automatic(blti))
    }};
    (($args:pat, $ctx:pat) $body:tt) => {{
        fn blti($args: &mut VAN, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body
        (None, BuiltInFn::Manual(blti))
    }};
    (($args:pat) $body:tt) => {{
        fn blti($args: VAN) -> Option<ASTNode> $body
        (None, BuiltInFn::Automatic(blti))
    }};
}

macro_rules! define_bltins {
    ($($name:expr => $a2:tt $body:tt,)*) => {
        static BUILTINS: phf::Map<&'static str, (Option<usize>, BuiltInFn)> = phf_map! {
            $($name => define_blti!($a2 $body),)*
        };
    }
}

define_bltins! {
    "add" => (args | 2) {
        let unpacked = args.into_iter().filter_map(|x| Some(x
            .as_constant()?
            .parse::<i64>()
            .expect("expected number as @param"))).collect::<Vec<_>>();
        if unpacked.len() != 2 {
            // if any argument wasn't evaluated --> dropped --> different len()
            return None;
        }
        Some(ASTNode::Constant(true, (unpacked[0] + unpacked[1]).to_string().into()))
    },
    "def" => (args, ctx) {
        if args.len() < 3 {
            return None;
        }
        let mut unpack = |x: &mut ASTNode, ctx: &mut EvalContext<'_>| {
            x.eval(ctx);
            conv_to_constant(x)
        };
        let varname = unpack(&mut args[0], ctx)?;
        let argc: usize = unpack(&mut args[1], ctx)?
            .parse()
            .expect("expected number as argc");
        let mut value = args[2..].to_vec().lift_ast();
        if value.eval(ctx) {
            ctx.defs
                .insert(varname, (argc, value.simplify()));
            Some(ASTNode::NullNode)
        } else {
            None
        }
    },
    "def-lazy" => (args, ctx) {
        if args.len() < 3 {
            return None;
        }
        let mut unpack = |x: &mut ASTNode, ctx: &mut EvalContext<'_>| {
            x.eval(ctx);
            conv_to_constant(x)
        };
        let varname = unpack(&mut args[0], ctx)?;
        let argc: usize = unpack(&mut args[1], ctx)?
            .parse()
            .expect("expected number as argc");
        ctx.defs
            .insert(varname, (argc, args[2..].to_vec().lift_ast().simplify()));
        Some(ASTNode::NullNode)
    },
    "foreach" => (args | 2, mut ctx) {
        {
            let x = &mut args[0];
            x.eval(ctx);
        }
        let elems = crate::parser::args2unspaced(match &args[0] {
            ASTNode::Grouped(_, ref elems) => Some(elems),
            _ => None,
        }?.clone()).into_iter().map(|i| if let ASTNode::Grouped(_, tmp_args) = i {
            crate::parser::args2unspaced(tmp_args)
        } else {
            CmdEvalArgs(i.lift_ast())
        });
        eval_foreach(elems, &args[1], ctx)
    },
    "foreach-raw" => (args | 2, mut ctx) {
        {
            let x = &mut args[0];
            x.eval(ctx);
        }
        let mut elems = match &args[0] {
            ASTNode::Grouped(_, ref elems) => Some(elems),
            _ => None,
        }?.clone().into_iter().map(|i| CmdEvalArgs(if let ASTNode::Grouped(_, tmp_args) = i {
            tmp_args
        } else {
            i.lift_ast()
        }));
        eval_foreach(elems, &args[1], ctx)
    },
    "fseq" => (mut args, ctx) {
        // force sequential evaluation
        if args.iter_mut().all(|i| i.eval(ctx)) {
            Some(args.take().lift_ast())
        } else {
            None
        }
    },
    "include" => (args | 1, ctx) {
        args[0].eval(ctx);
        let filename = conv_to_constant(&args[0])?;
        let filename: &str = &filename;
        Some(
            { cfg_if! {
                if #[cfg(feature = "compile")] {
                    match ctx.comp_map.get(filename) {
                        None => crate::parser::file2ast(filename, ctx.opts),
                        Some(compf) => ctx.load_from_compfile(compf),
                    }
                } else {
                    crate::parser::file2ast(filename, ctx.opts)
                }
            }}.expect("expected valid file").lift_ast()
        )
    },
    "pass" => (args) {
        Some(args.lift_ast())
    },
    "suppress" => (_args) {
        // suppress all direct output from the code section, but evaluate it
        Some(ASTNode::NullNode)
    },
    "undef" => (args | 1, ctx) {
        let unpack = |x: &mut ASTNode, ctx: &mut EvalContext<'_>| {
            x.eval(ctx);
            conv_to_constant(x)
        };
        let varname = unpack(&mut args[0], ctx)?;
        ctx.defs
            .remove(&varname);
        Some(ASTNode::NullNode)
    },
    "une" => (args) {
        // un-escape
        Some(args.into_iter().map(|mut x| {
            if let ASTNode::Grouped(ref mut gt, _) = x {
                *gt = GroupType::Dissolving;
            }
            x
        }).collect::<Vec<_>>().lift_ast())
    },
    "unee" => (args) {
        // un-escape for eval
        Some(crate::parser::args2unspaced(args.into_iter().map(|mut x| {
            if let ASTNode::Grouped(ref mut gt, _) = x {
                *gt = GroupType::Dissolving
            }
            x
        }).collect::<Vec<_>>().simplify()).lift_ast())
    },
}

fn eval_cmd(cmd: &mut VAN, args: &mut CmdEvalArgs, mut ctx: &mut EvalContext) -> Option<ASTNode> {
    use crate::mangle_ast::MangleASTExt;

    // evaluate command name
    for i in cmd.iter_mut() {
        i.eval(ctx);
    }
    // allow partial evaluation of command name
    *cmd = cmd.take().simplify().compact_toplevel();
    let cmd = match cmd.clone().lift_ast().simplify() {
        ASTNode::Constant(true, cmd) => cmd,
        _ => return None,
    };

    // evaluate command
    if let Some((a, x)) = BUILTINS.get(&*cmd) {
        match a {
            Some(n) if args.len() != *n => None,
            _ => match x {
                BuiltInFn::Manual(y) => y(&mut args.0, &mut ctx),
                BuiltInFn::Automatic(y) => {
                    for i in args.iter_mut() {
                        i.eval(ctx);
                    }
                    y(args.0.clone())
                }
            },
        }
    } else {
        let (n, mut x) = ctx.defs.get(&cmd)?.clone();
        *args = CmdEvalArgs(
            args.take()
                .into_iter()
                .flat_map(|mut i| {
                    i.eval(ctx);
                    if let ASTNode::Grouped(GroupType::Dissolving, elems) = i {
                        elems
                    } else {
                        i.lift_ast()
                    }
                })
                .collect(),
        );
        if args.len() != n || x.apply_arguments_inplace(&args).is_err() {
            None
        } else {
            Some(x)
        }
    }
}

trait Eval: MangleAST {
    /// if (return value): fully evaluated
    fn eval(&mut self, ctx: &mut EvalContext) -> bool;
}

impl Eval for ASTNode {
    fn eval(mut self: &mut Self, ctx: &mut EvalContext) -> bool {
        use ASTNode::*;
        match &mut self {
            CmdEval(cmd, args) => {
                if let Some(x) = eval_cmd(cmd, args, ctx) {
                    *self = x;
                    true
                } else {
                    false
                }
            }
            Grouped(_, x) => x.eval(ctx),
            _ => true,
        }
    }
}

impl Eval for VAN {
    fn eval(&mut self, ctx: &mut EvalContext) -> bool {
        let mut ret = true;
        for i in self {
            ret &= i.eval(ctx);
        }
        ret
    }
}

impl Eval for CmdEvalArgs {
    fn eval(&mut self, ctx: &mut EvalContext) -> bool {
        self.0.eval(ctx)
    }
}

pub fn eval(
    data: &mut VAN,
    opts: ParserOptions,
    _comp_map: &CompilatesMap<'_>,
    comp_out: Option<&str>,
) {
    use crate::mangle_ast::MangleASTExt;
    let mut ctx = EvalContext {
        defs: HashMap::new(),
        opts,
        #[cfg(feature = "compile")]
        comp_map: _comp_map,
        #[cfg(not(feature = "compile"))]
        comp_map: std::marker::PhantomData,
    };
    let mut cplx = data.get_complexity();
    loop {
        data.eval(&mut ctx);
        *data = data.take().simplify().compact_toplevel();
        let new_cplx = data.get_complexity();
        if new_cplx == cplx {
            break;
        }
        cplx = new_cplx;
    }
    cfg_if! {
        if #[cfg(feature = "compile")] {
            if let Some(comp_out) = comp_out {
                ctx.save_to_compfile(comp_out, &*data)
                    .expect("save failed");
            }
        } else {
            let _ = comp_out;
        }
    }
}
