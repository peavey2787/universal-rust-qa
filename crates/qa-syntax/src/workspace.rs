use qa_model::Finding;
use quote::ToTokens;
use std::{
    fs,
    path::{Path, PathBuf},
};
use syn::{Attribute, FnArg, ImplItem, Item, ItemFn, TraitItem, Visibility, spanned::Spanned};
use walkdir::WalkDir;
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    pub module_depth: usize,
}
#[derive(Debug, Clone)]
pub struct SourceFunction {
    pub path: PathBuf,
    pub name: String,
    pub qualified_name: String,
    pub line: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub is_test: bool,
    pub is_public: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub abi: Option<String>,
    pub parameters: usize,
    pub generic_parameters: usize,
    pub statements: usize,
    pub attributes: Vec<String>,
    pub calls: Vec<String>,
    pub source: String,
}
#[derive(Debug, Clone)]
pub struct SourceType {
    pub path: PathBuf,
    pub name: String,
    pub line: usize,
    pub kind: String,
    pub field_count: usize,
    pub variant_count: usize,
    pub variant_names: Vec<String>,
    pub terminal_variants: Vec<String>,
    pub field_types: Vec<String>,
    pub is_public: bool,
    pub attributes: Vec<String>,
    pub source: String,
}
#[derive(Debug, Clone)]
pub struct SourceInterface {
    pub path: PathBuf,
    pub name: String,
    pub line: usize,
    pub kind: String,
    pub item_count: usize,
    pub source: String,
}
#[derive(Debug, Clone)]
pub struct SourceModule {
    pub path: PathBuf,
    pub name: String,
    pub line: usize,
    pub depth: usize,
}
#[derive(Debug, Default)]
pub struct WorkspaceSource {
    pub root: PathBuf,
    pub files: Vec<SourceFile>,
    pub functions: Vec<SourceFunction>,
    pub types: Vec<SourceType>,
    pub interfaces: Vec<SourceInterface>,
    pub modules: Vec<SourceModule>,
    pub parse_findings: Vec<Finding>,
}
pub fn discover(workspace: &Path) -> WorkspaceSource {
    let mut out = WorkspaceSource { root: workspace.to_path_buf(), ..Default::default() };
    for e in WalkDir::new(workspace).into_iter().filter_map(Result::ok) {
        let p = e.path();
        if !e.file_type().is_file()
            || p.extension().and_then(|s| s.to_str()) != Some("rs")
            || excluded(p)
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(p) else { continue };
        match syn::parse_file(&text) {
            Ok(ast) => {
                let test_context = path_is_test(p) || has_cfg_test(&ast.attrs);
                collect(p, &text, &ast.items, "", 0, test_context, &mut out);
                if !test_context {
                    out.files.push(SourceFile {
                        path: p.to_path_buf(),
                        text,
                        module_depth: depth(workspace, p),
                    });
                }
            }
            Err(err) => out.parse_findings.push(Finding {
                rule_id: "QA-SYNTAX-001".into(),
                severity: qa_model::Severity::High,
                message: format!("Rust source could not be parsed: {err}"),
                path: Some(p.display().to_string()),
                line: None,
                detail: None,
            }),
        }
    }
    out
}
fn excluded(p: &Path) -> bool {
    p.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("target" | "qa-out" | "mutants.out" | "vendor" | "fixtures" | ".git")
        )
    })
}
fn depth(root: &Path, p: &Path) -> usize {
    p.strip_prefix(root).unwrap_or(p).components().count().saturating_sub(2)
}
fn collect(
    path: &Path,
    text: &str,
    items: &[Item],
    prefix: &str,
    depth: usize,
    test_context: bool,
    out: &mut WorkspaceSource,
) {
    for item in items {
        if test_context {
            collect_test_item(path, text, item, prefix, out);
        } else {
            collect_item(path, text, item, prefix, depth, out);
        }
    }
}

fn collect_test_item(
    path: &Path,
    text: &str,
    item: &Item,
    prefix: &str,
    out: &mut WorkspaceSource,
) {
    match item {
        Item::Fn(function) if has_test(&function.attrs) => {
            push_fn(path, text, function, prefix, true, out);
        }
        Item::Mod(module) => {
            if let Some((_, nested)) = &module.content {
                let qualified = qual(prefix, &module.ident.to_string());
                collect(path, text, nested, &qualified, 0, true, out);
            }
        }
        _ => {}
    }
}

fn collect_item(
    path: &Path,
    text: &str,
    item: &Item,
    prefix: &str,
    depth: usize,
    out: &mut WorkspaceSource,
) {
    match item {
        Item::Fn(function) => push_fn(path, text, function, prefix, false, out),
        Item::Mod(module) => collect_module(path, text, module, prefix, depth, out),
        Item::Impl(item_impl) => collect_impl(path, text, item_impl, prefix, out),
        Item::Trait(item_trait) => collect_trait(path, text, item_trait, prefix, out),
        Item::Struct(item_struct) => collect_struct(path, text, item_struct, prefix, out),
        Item::Enum(item_enum) => collect_enum(path, text, item_enum, prefix, out),
        _ => {}
    }
}

fn collect_module(
    path: &Path,
    text: &str,
    module: &syn::ItemMod,
    prefix: &str,
    depth: usize,
    out: &mut WorkspaceSource,
) {
    let qualified = qual(prefix, &module.ident.to_string());
    let nested_test = has_cfg_test(&module.attrs);
    if !nested_test {
        out.modules.push(SourceModule {
            path: path.into(),
            name: qualified.clone(),
            line: start(module.mod_token.span),
            depth: depth + 1,
        });
    }
    if let Some((_, nested)) = &module.content {
        collect(path, text, nested, &qualified, depth + 1, nested_test, out);
    }
}

fn collect_impl(
    path: &Path,
    text: &str,
    item_impl: &syn::ItemImpl,
    prefix: &str,
    out: &mut WorkspaceSource,
) {
    let self_type = item_impl.self_ty.to_token_stream().to_string();
    let type_name = item_impl
        .trait_
        .as_ref()
        .map(|(_, path, _)| format!("{} for {self_type}", path.to_token_stream()))
        .unwrap_or(self_type);
    let start_line = start(item_impl.impl_token.span);
    let end_line = item_impl.brace_token.span.close().end().line;
    out.interfaces.push(SourceInterface {
        path: path.into(),
        name: type_name.clone(),
        line: start_line,
        kind: "impl".into(),
        item_count: item_impl.items.iter().filter(|item| matches!(item, ImplItem::Fn(_))).count(),
        source: lines(text, start_line, end_line),
    });
    for item in &item_impl.items {
        if let ImplItem::Fn(function) = item {
            let context = FunctionSourceContext {
                path,
                text,
                qualified_name: qual(prefix, &format!("{type_name}::{}", function.sig.ident)),
                test_context: false,
            };
            push_sig(context, &function.sig, &function.attrs, &function.vis, &function.block, out);
        }
    }
}

fn collect_trait(
    path: &Path,
    text: &str,
    item_trait: &syn::ItemTrait,
    prefix: &str,
    out: &mut WorkspaceSource,
) {
    let start_line = start(item_trait.trait_token.span);
    let end_line = item_trait.brace_token.span.close().end().line;
    out.interfaces.push(SourceInterface {
        path: path.into(),
        name: qual(prefix, &item_trait.ident.to_string()),
        line: start_line,
        kind: "trait".into(),
        item_count: item_trait.items.iter().filter(|item| matches!(item, TraitItem::Fn(_))).count(),
        source: lines(text, start_line, end_line),
    });
}

fn collect_struct(
    path: &Path,
    text: &str,
    item_struct: &syn::ItemStruct,
    prefix: &str,
    out: &mut WorkspaceSource,
) {
    let start_line = start(item_struct.struct_token.span);
    out.types.push(SourceType {
        path: path.into(),
        name: qual(prefix, &item_struct.ident.to_string()),
        line: start_line,
        kind: "struct".into(),
        field_count: item_struct.fields.len(),
        variant_count: 0,
        variant_names: vec![],
        terminal_variants: vec![],
        field_types: item_struct
            .fields
            .iter()
            .map(|field| field.ty.to_token_stream().to_string())
            .collect(),
        is_public: matches!(item_struct.vis, Visibility::Public(_)),
        attributes: attrs(&item_struct.attrs),
        source: lines(text, start_line, struct_end_line(item_struct, start_line)),
    });
}

fn struct_end_line(item_struct: &syn::ItemStruct, start_line: usize) -> usize {
    match &item_struct.fields {
        syn::Fields::Named(fields) => fields.brace_token.span.close().end().line,
        syn::Fields::Unnamed(fields) => item_struct
            .semi_token
            .as_ref()
            .map(|semi| semi.span().end().line)
            .unwrap_or_else(|| fields.paren_token.span.close().end().line),
        syn::Fields::Unit => {
            item_struct.semi_token.as_ref().map(|semi| semi.span().end().line).unwrap_or(start_line)
        }
    }
}

fn collect_enum(
    path: &Path,
    text: &str,
    item_enum: &syn::ItemEnum,
    prefix: &str,
    out: &mut WorkspaceSource,
) {
    let start_line = start(item_enum.enum_token.span);
    out.types.push(SourceType {
        path: path.into(),
        name: qual(prefix, &item_enum.ident.to_string()),
        line: start_line,
        kind: "enum".into(),
        field_count: 0,
        variant_count: item_enum.variants.len(),
        variant_names: item_enum.variants.iter().map(|variant| variant.ident.to_string()).collect(),
        terminal_variants: item_enum
            .variants
            .iter()
            .filter(|variant| has_terminal(&variant.attrs))
            .map(|variant| variant.ident.to_string())
            .collect(),
        field_types: item_enum
            .variants
            .iter()
            .flat_map(|variant| {
                variant.fields.iter().map(|field| field.ty.to_token_stream().to_string())
            })
            .collect(),
        is_public: matches!(item_enum.vis, Visibility::Public(_)),
        attributes: attrs(&item_enum.attrs),
        source: lines(text, start_line, item_enum.brace_token.span.close().end().line),
    });
}

fn has_terminal(a: &[Attribute]) -> bool {
    a.iter().any(|x| x.path().segments.last().is_some_and(|s| s.ident == "terminal"))
}
struct FunctionSourceContext<'a> {
    path: &'a Path,
    text: &'a str,
    qualified_name: String,
    test_context: bool,
}

fn push_fn(
    path: &Path,
    text: &str,
    f: &ItemFn,
    prefix: &str,
    test_context: bool,
    out: &mut WorkspaceSource,
) {
    let context = FunctionSourceContext {
        path,
        text,
        qualified_name: qual(prefix, &f.sig.ident.to_string()),
        test_context,
    };
    push_sig(context, &f.sig, &f.attrs, &f.vis, &f.block, out)
}
fn push_sig(
    context: FunctionSourceContext<'_>,
    sig: &syn::Signature,
    at: &[Attribute],
    vis: &Visibility,
    block: &syn::Block,
    out: &mut WorkspaceSource,
) {
    let path = context.path;
    let text = context.text;
    let st = start(sig.fn_token.span);
    let en = block.brace_token.span.close().end().line;
    let src = lines(text, st, en);
    out.functions.push(SourceFunction {
        path: path.into(),
        name: sig.ident.to_string(),
        qualified_name: context.qualified_name,
        line: st,
        start_line: st,
        end_line: en,
        is_test: context.test_context || has_test(at) || path_is_test(path),
        is_public: matches!(vis, Visibility::Public(_)),
        is_async: sig.asyncness.is_some(),
        is_unsafe: sig.unsafety.is_some(),
        abi: sig
            .abi
            .as_ref()
            .map(|a| a.name.as_ref().map(|s| s.value()).unwrap_or_else(|| "C".into())),
        parameters: sig.inputs.iter().filter(|a| matches!(a, FnArg::Typed(_))).count(),
        generic_parameters: sig.generics.params.len(),
        statements: block.stmts.len(),
        attributes: attrs(at),
        calls: calls(&src),
        source: src,
    })
}
fn start(s: proc_macro2::Span) -> usize {
    s.start().line
}
fn qual(p: &str, n: &str) -> String {
    if p.is_empty() { n.into() } else { format!("{p}::{n}") }
}
fn attrs(a: &[Attribute]) -> Vec<String> {
    a.iter().map(|x| x.to_token_stream().to_string()).collect()
}
fn has_cfg_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg") && attribute.to_token_stream().to_string().contains("test")
    })
}
fn path_is_test(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str().to_str() == Some("tests"))
        || path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| {
            stem == "tests" || stem == "test_support" || stem.ends_with("_tests")
        })
}
fn has_test(a: &[Attribute]) -> bool {
    a.iter().any(|x| {
        x.path().is_ident("test") || x.path().segments.last().is_some_and(|s| s.ident == "test")
    })
}
fn lines(t: &str, s: usize, e: usize) -> String {
    t.lines().skip(s.saturating_sub(1)).take(e.saturating_sub(s) + 1).collect::<Vec<_>>().join("\n")
}
mod calls;
use calls::*;

#[cfg(test)]
mod tests;
