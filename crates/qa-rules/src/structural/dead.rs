use proc_macro2::{TokenStream, TokenTree};
use qa_model::{DeadItem, Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};
use std::collections::{HashMap, HashSet};
use syn::visit::{self, Visit};

pub fn analyze(
    source: &WorkspaceSource,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) -> Vec<DeadItem> {
    let calls = call_counts(source);
    source
        .functions
        .iter()
        .filter_map(|function| classify(function, config, &calls, findings))
        .collect()
}

fn call_counts(source: &WorkspaceSource) -> HashMap<String, usize> {
    let known_functions =
        source.functions.iter().map(|function| function.name.clone()).collect::<HashSet<_>>();
    let mut references = ReferenceCounts::new(known_functions);
    for file in &source.files {
        let Ok(ast) = syn::parse_file(&file.text) else { continue };
        references.visit_file(&ast);
    }
    references.calls
}

struct ReferenceCounts {
    calls: HashMap<String, usize>,
    current_function: Option<String>,
    known_functions: HashSet<String>,
}

impl ReferenceCounts {
    fn new(known_functions: HashSet<String>) -> Self {
        Self { calls: HashMap::new(), current_function: None, known_functions }
    }

    fn record(&mut self, path: &str) {
        let name = path.rsplit("::").next().unwrap_or(path);
        let direct_self_reference =
            !path.contains("::") && self.current_function.as_deref() == Some(name);
        if !self.known_functions.contains(name) || direct_self_reference {
            return;
        }
        *self.calls.entry(name.to_owned()).or_default() += 1;
    }

    fn with_function(&mut self, name: String, visit: impl FnOnce(&mut Self)) {
        let previous = self.current_function.replace(name);
        visit(self);
        self.current_function = previous;
    }

    fn record_macro_tokens(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => self.record(&ident.to_string()),
                TokenTree::Group(group) => self.record_macro_tokens(group.stream()),
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for ReferenceCounts {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.with_function(node.sig.ident.to_string(), |visitor| {
            visitor.visit_block(&node.block);
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.with_function(node.sig.ident.to_string(), |visitor| {
            visitor.visit_block(&node.block);
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.with_function(node.sig.ident.to_string(), |visitor| {
            if let Some(block) = &node.default {
                visitor.visit_block(block);
            }
        });
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        let path = node
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        self.record(&path);
        visit::visit_expr_path(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.record(&node.method.to_string());
        visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if !node.path.is_ident("macro_rules") {
            self.record_macro_tokens(node.tokens.clone());
        }
    }
}

fn classify(
    function: &SourceFunction,
    config: &QaConfig,
    calls: &HashMap<String, usize>,
    findings: &mut Vec<Finding>,
) -> Option<DeadItem> {
    if excluded_root(function) {
        return None;
    }
    let references = *calls.get(&function.name).unwrap_or(&0);
    let private_dead = !function.is_public && references == 0;
    let exported_dead = config.dead_code.closed_world && function.is_public && references == 0;
    if !private_dead && !exported_dead {
        return None;
    }
    findings.push(dead_finding(function, private_dead));
    Some(DeadItem {
        path: function.path.display().to_string(),
        line: function.line,
        name: function.qualified_name.clone(),
        kind: "function".into(),
        confidence: if private_dead { "high" } else { "workspace-only" }.into(),
    })
}

fn excluded_root(function: &SourceFunction) -> bool {
    function.is_test
        || trait_impl_function(function)
        || ["main", "new", "default", "drop"].contains(&function.name.as_str())
}

fn trait_impl_function(function: &SourceFunction) -> bool {
    function.qualified_name.rsplit_once("::").is_some_and(|(owner, _)| owner.contains(" for "))
}

fn dead_finding(function: &SourceFunction, private_dead: bool) -> Finding {
    Finding {
        rule_id: if private_dead { "QA-DEAD-001" } else { "QA-DEAD-002" }.into(),
        severity: if private_dead { Severity::Medium } else { Severity::Low },
        message: format!("Function `{}` is unreferenced in source graph", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: Some(
            "Source-graph evidence can require exceptions for macros, FFI or external callers."
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests;
