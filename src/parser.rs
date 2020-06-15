use crate::ast::{ASTNode, CmdEvalArgs, GroupType, VAN};

// === parser utils

fn get_offset_of(whole_buffer: &str, part: &str) -> usize {
    // NOTE: use offset_from() once it's stable
    part.as_ptr() as usize - whole_buffer.as_ptr() as usize
}

fn str_slice_between<'a>(whole_buffer_start: &'a str, post_part: &'a str) -> &'a str {
    &whole_buffer_start[..get_offset_of(whole_buffer_start, post_part)]
}

fn is_scope_end(x: char) -> bool {
    match x {
        /* '(' */ ')' => true,
        /* '{' */ '}' => true,
        _ => false,
    }
}

/// 1. part while f(x) == true, then 2. part
fn str_split_at_while(x: &str, f: impl FnMut(&char) -> bool) -> (&str, &str) {
    x.split_at(x.chars().take_while(f).map(char::len_utf8).sum::<usize>())
}

/// escaped escape symbol or other escaped code: optional passthrough
fn parse_escaped_const(i: char, opts: ParserOptions) -> Option<ASTNode> {
    Some(ASTNode::Constant(
        true,
        match i {
            _ if i == opts.escc && !opts.pass_escc => {
                let mut tmp = [0; 4];
                let tmp = opts.escc.encode_utf8(&mut tmp);
                (*tmp).into()
            }
            '{' => crulst_atom!("{"),
            '}' => crulst_atom!("}"),
            '\n' => crulst_atom!(""),
            '$' => crulst_atom!("$"),
            _ => return None,
        },
    ))
}

fn str_split_at_ctrl(
    data: &str,
    opts: ParserOptions,
    f_do_cont_at: impl Fn(&char) -> bool,
) -> (&str, &str) {
    str_split_at_while(data, |i| match i {
        '$' | '(' | ')' | '{' | '}' => false,
        _ if i == &opts.escc => false,
        _ => f_do_cont_at(i),
    })
}

// === parser options

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParserOptions {
    escc: char,
    pass_escc: bool,
}

impl ParserOptions {
    #[inline]
    pub fn new(escc: char, pass_escc: bool) -> Self {
        Self { escc, pass_escc }
    }
}

// === parse trait

trait Parse: Sized {
    type ErrorDesc: std::error::Error;
    /// # Return value
    /// * `Ok(rest, parsed_obj)`
    /// * `Err(offending_code, description)`
    fn parse(data: &str, opts: ParserOptions) -> Result<(&str, Self), (&str, Self::ErrorDesc)>;
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum ParserErrorDetail {
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("got empty/invalid eval statement")]
    InvalidEval,
    #[error("expected '{0}' instead")]
    ExpectedInstead(char),

    /// Escaped end-of-scope markers are dangerous, because they probably don't
    /// do, what you would naively expect. The correct way to escape them is
    /// to *not* escape the corresponding begin-of-scope marker, which thus
    /// constitutes a Group.
    #[error("dangerous escaped end-of-scope marker ('{0}')")]
    DangerousEos(char),
    #[error("unexpected unbalanced end-of-scope marker ('{0}')")]
    UnbalancedEos(char),
}

type PED = ParserErrorDetail;

impl Parse for ASTNode {
    type ErrorDesc = PED;

    fn parse(data: &str, opts: ParserOptions) -> Result<(&str, Self), (&str, PED)> {
        let escc = opts.escc;
        let mut iter = data.chars();

        let i = iter.next().ok_or_else(|| (data, PED::UnexpectedEof))?;
        match i {
            _ if i == escc => {
                let d_after_escc = iter.as_str();
                let i = iter.next().ok_or_else(|| (data, PED::UnexpectedEof))?;
                if i == '(' {
                    // got begin of cmdeval block
                    let (rest, mut vanx) = VAN::parse(iter.as_str(), opts)?;
                    if vanx.is_empty() {
                        return Err((&data[..std::cmp::min(data.len(), 3)], PED::InvalidEval));
                    }
                    let mut iter = rest.chars();
                    if iter.next() != Some(')') {
                        return Err((data, PED::ExpectedInstead(/* '(' */ ')')));
                    }

                    // extract command
                    assert!(!vanx.is_empty());
                    let split_point = vanx
                        .iter()
                        .enumerate()
                        .filter_map(|y| if y.1.is_space() { Some(y.0 + 1) } else { None })
                        .next()
                        .unwrap_or(1);
                    let van = vanx.split_off(split_point);
                    let mut cmd = vanx;
                    if cmd.last().map(ASTNode::is_space).unwrap() {
                        cmd.pop();
                    }
                    Ok((
                        iter.as_str(),
                        ASTNode::CmdEval(cmd, CmdEvalArgs::from_wsdelim(van)),
                    ))
                } else if let Some(c) = parse_escaped_const(i, opts) {
                    Ok((iter.as_str(), c))
                } else if is_scope_end(i) {
                    Err((str_slice_between(data, iter.as_str()), PED::DangerousEos(i)))
                } else {
                    // interpret it as a command (LaTeX-alike)
                    let (cmd, mut rest) =
                        str_split_at_ctrl(d_after_escc, opts, |x| !x.is_whitespace());
                    if cmd.is_empty() {
                        return Err((str_slice_between(data, iter.as_str()), PED::InvalidEval));
                    }
                    let vanx = if rest.starts_with('(') {
                        let (tmp_rest, tmp) = ASTNode::parse(rest, opts)?;
                        if let ASTNode::Grouped(GroupType::Strict, van) = tmp {
                            rest = tmp_rest;
                            CmdEvalArgs::from_wsdelim(van)
                        } else {
                            unreachable!()
                        }
                    } else {
                        Default::default()
                    };
                    Ok((
                        rest,
                        ASTNode::CmdEval(vec![ASTNode::Constant(true, cmd.into())], vanx),
                    ))
                }
            }
            '(' => {
                let (rest, van) = VAN::parse(iter.as_str(), opts)?;
                let mut iter = rest.chars();
                if iter.next() != Some(')') {
                    return Err((rest, PED::ExpectedInstead(/* '(' */ ')')));
                }
                Ok((iter.as_str(), ASTNode::Grouped(GroupType::Strict, van)))
            }
            '{' => {
                let (rest, van) = VAN::parse(iter.as_str(), opts)?;
                let mut iter = rest.chars();
                if iter.next() != Some('}') {
                    return Err((rest, PED::ExpectedInstead(/* '{' */ '}')));
                }
                Ok((iter.as_str(), ASTNode::Grouped(GroupType::Loose, van)))
            }
            '$' => {
                let (cdat, rest) = str_split_at_while(iter.as_str(), |i| *i == '$');
                let (idxs, rest) = str_split_at_while(rest, |i| i.is_digit(10));
                Ok((
                    rest,
                    ASTNode::Argument {
                        indirection: cdat.len(),
                        index: idxs.parse().ok(),
                    },
                ))
            }
            _ if is_scope_end(i) => Err((
                str_slice_between(data, iter.as_str()),
                PED::UnbalancedEos(i),
            )),
            _ => {
                let is_whitespace = i.is_whitespace();
                let (cdat, rest) =
                    str_split_at_ctrl(data, opts, |x| x.is_whitespace() == is_whitespace);
                Ok((rest, ASTNode::Constant(!is_whitespace, cdat.into())))
            }
        }
    }
}

impl Parse for VAN {
    type ErrorDesc = PED;
    fn parse(mut data: &str, opts: ParserOptions) -> Result<(&str, Self), (&str, PED)> {
        let mut ret = VAN::new();
        while data.chars().next().map(is_scope_end) == Some(false) {
            let (rest, node) = ASTNode::parse(data, opts)?;
            ret.push(node);
            data = rest;
        }
        Ok((data, ret))
    }
}

// === main parser

/// At top level, only parse things inside CmdEval's
pub fn parse_toplevel(mut data: &str, opts: ParserOptions) -> Result<VAN, (&str, PED)> {
    let mut ret = VAN::new();
    while !data.is_empty() {
        let mut cstp_has_nws = false;
        let (cstp, rest) = str_split_at_while(data, |i| {
            cstp_has_nws |= !i.is_whitespace();
            i != &opts.escc
        });
        if !cstp.is_empty() {
            ret.push(ASTNode::Constant(cstp_has_nws, cstp.into()));
        }
        if rest.is_empty() {
            break;
        }
        let (rest, node) = ASTNode::parse(rest, opts)?;
        ret.push(node);
        data = rest;
    }
    Ok(ret)
}

pub fn file2ast(filename: &str, opts: ParserOptions) -> Result<VAN, anyhow::Error> {
    use anyhow::Context;

    let fh = readfilez::read_from_file(std::fs::File::open(filename))
        .with_context(|| format!("unable to read file '{}'", filename))?;
    let input = std::str::from_utf8(fh.as_slice())
        .with_context(|| format!("file '{}' contains non-UTF-8 data", filename))?;

    parse_toplevel(input, opts).map_err(|(offending, descr)| {
        use codespan_reporting::{
            diagnostic::{Diagnostic, Label},
            term,
        };
        use std::str::FromStr;

        let mut files = codespan::Files::new();
        let fileid = files.add(filename, input);
        let start_pos = get_offset_of(input, offending);

        term::emit(
            &mut term::termcolor::StandardStream::stderr(
                term::ColorArg::from_str("auto").unwrap().into(),
            )
            .lock(),
            &term::Config::default(),
            &files,
            &Diagnostic::error()
                .with_message(descr.to_string())
                .with_labels(vec![Label::primary(
                    fileid,
                    start_pos..(start_pos + offending.len()),
                )]),
        )
        .unwrap();
        anyhow::anyhow!("{}", descr)
    })
}
