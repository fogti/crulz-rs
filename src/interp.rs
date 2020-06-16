use crate::{
    ast::{ASTNode, CmdEvalArgs, GroupType, LiftAST, VAN},
    mangle_ast::MangleAST,
    parser::ParserOptions,
};
#[cfg(feature = "compile")]
use anyhow::Context;
use atoi::atoi;
use cfg_if::cfg_if;
use phf::phf_map;
use std::collections::hash_map::HashMap;

enum BuiltInFn {
    Manual(fn(&mut VAN, &mut EvalContext) -> Option<ASTNode>),
    Automatic(fn(VAN) -> Option<ASTNode>),
}

type DefinesMap = HashMap<Vec<u8>, (usize, ASTNode)>;
type CompilatesMap<'a> = HashMap<&'a [u8], &'a [u8]>;

struct EvalContext<'a> {
    defs: DefinesMap,
    opts: ParserOptions,
    #[cfg_attr(not(feature = "compile"), allow(unused))]
    comp_map: &'a CompilatesMap<'a>,
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

fn eval_foreach(
    mut elems: impl Iterator<Item = CmdEvalArgs>,
    fecmd: &ASTNode,
    ctx: &mut EvalContext<'_>,
) -> Option<ASTNode> {
    Some(
        if let ASTNode::Constant(is_dat, _) = &fecmd {
            debug_assert!(is_dat);

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
    (($args:pat | $ac:expr, $ctx:pat) $body:ident) => {{
        /* fn blti($args: &mut VAN, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body */
        (Some($ac), BuiltInFn::Manual($body))
    }};
    (($args:pat | $ac:expr) $body:ident) => {{
        /* fn blti($args: VAN) -> Option<ASTNode> $body */
        (Some($ac), BuiltInFn::Automatic($body))
    }};
    (($args:pat, $ctx:pat) $body:ident) => {{
        /* fn blti($args: &mut VAN, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body */
        (None, BuiltInFn::Manual($body))
    }};
    (($args:pat) $body:ident) => {{
        /* fn blti($args: VAN) -> Option<ASTNode> $body */
        (None, BuiltInFn::Automatic($body))
    }};
}

macro_rules! define_bltins {
    ($($name:expr => $a2:tt $body:tt,)*) => {
        phf_map! { $($name => define_blti!($a2 $body),)* }
    }
}

fn unpack(x: &mut ASTNode, ctx: &mut EvalContext<'_>) -> Option<Vec<u8>> {
    x.eval(ctx);
    x.conv_to_constant().map(|y| y.into_owned())
}

fn uneg(mut arg: ASTNode) -> ASTNode {
    if let ASTNode::Grouped(ref mut gt, _) = arg {
        *gt = GroupType::Dissolving;
    }
    arg
}

fn fe_elems(x: &ASTNode) -> Option<VAN> {
    match x {
        ASTNode::Grouped(_, ref elems) => Some(elems.clone()),
        _ => None,
    }
}

static BUILTINS: phf::Map<&'static [u8], (Option<usize>, BuiltInFn)> = define_bltins! {
    b"add"         => (args | 2     ) blti_add,
    b"def"         => (args    , ctx) blti_def,
    b"def-lazy"    => (args    , ctx) blti_def_lazy,
    b"foreach"     => (args | 2, ctx) blti_foreach,
    b"foreach-raw" => (args | 2, ctx) blti_foreach_raw,
    b"fseq"        => (args    , ctx) blti_fseq,
    b"include"     => (args | 1, ctx) blti_include,
    b"pass"        => (args         ) blti_pass,
    b"suppress"    => (_args        ) blti_suppress,
    b"undef"       => (args | 1, ctx) blti_undef,
    b"une"         => (args         ) blti_une,
    b"unee"        => (args         ) blti_unee,
};

fn blti_suppress(_args: VAN) -> Option<ASTNode> {
    Some(ASTNode::NullNode)
}
fn blti_une(args: VAN) -> Option<ASTNode> {
    Some(args.into_iter().map(uneg).collect::<Vec<_>>().lift_ast())
}
fn blti_pass(args: VAN) -> Option<ASTNode> {
    Some(args.lift_ast())
}
fn blti_include(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    args[0].eval(ctx);
    let filename = args[0].conv_to_constant()?;
    let filename: &str = std::str::from_utf8(&filename).expect("got invalid include filename");
    Some(
        { crate::parser::file2ast(filename.as_ref(), ctx.opts) }
            .expect("expected valid file")
            .lift_ast(),
    )
}
fn blti_foreach_raw(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    {
        let x = &mut args[0];
        x.eval(ctx);
    }
    let elems = fe_elems(&args[0])?.into_iter().map(|i| {
        CmdEvalArgs(if let ASTNode::Grouped(_, tmp_args) = i {
            tmp_args
        } else {
            i.lift_ast()
        })
    });
    eval_foreach(elems, &args[1], ctx)
}
fn blti_foreach(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    {
        let x = &mut args[0];
        x.eval(ctx);
    }
    let elems = CmdEvalArgs::from_wsdelim(fe_elems(&args[0])?)
        .into_iter()
        .map(|i| {
            if let ASTNode::Grouped(_, tmp_args) = i {
                CmdEvalArgs::from_wsdelim(tmp_args)
            } else {
                CmdEvalArgs(i.lift_ast())
            }
        });
    eval_foreach(elems, &args[1], ctx)
}
fn blti_unee(args: VAN) -> Option<ASTNode> {
    Some(
        CmdEvalArgs::from_wsdelim(args.into_iter().map(uneg).collect::<Vec<_>>().simplify())
            .0
            .lift_ast(),
    )
}
fn blti_def_lazy(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    if args.len() < 3 {
        None
    } else {
        let varname = unpack(&mut args[0], ctx)?;
        let argc: usize = atoi(&unpack(&mut args[1], ctx)?).expect("expected number as argc");
        ctx.defs
            .insert(varname, (argc, args[2..].to_vec().lift_ast().simplify()));
        Some(ASTNode::NullNode)
    }
}
fn blti_def(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    if args.len() >= 3 {
        let varname = unpack(&mut args[0], ctx)?;
        let argc: usize = atoi(&unpack(&mut args[1], ctx)?).expect("expected number as argc");
        let mut value = args[2..].to_vec().lift_ast();
        if value.eval(ctx) {
            ctx.defs.insert(varname, (argc, value.simplify()));
            return Some(ASTNode::NullNode);
        }
    }
    None
}
fn blti_add(args: VAN) -> Option<ASTNode> {
    let unpacked = args
        .into_iter()
        .filter_map(|x| Some(atoi::<i64>(x.as_constant()?).expect("expected number as @param")))
        .collect::<Vec<_>>();
    if unpacked.len() != 2 {
        None
    } else {
        Some(ASTNode::Constant(
            true,
            (unpacked[0] + unpacked[1]).to_string().into(),
        ))
    }
}
fn blti_fseq(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    if args.iter_mut().all(|i| i.eval(ctx)) {
        Some(args.take().lift_ast())
    } else {
        None
    }
}
fn blti_undef(args: &mut VAN, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let varname = unpack(&mut args[0], ctx)?;
    ctx.defs.remove(&varname);
    Some(ASTNode::NullNode)
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
    if let Some((a, x)) = BUILTINS.get(&**cmd) {
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
        let (n, mut x) = ctx.defs.get(&*cmd)?.clone();
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
    comp_map: &CompilatesMap<'_>,
    comp_out: Option<&str>,
) {
    use crate::mangle_ast::MangleASTExt;
    let mut ctx = EvalContext {
        defs: HashMap::new(),
        opts,
        comp_map,
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
