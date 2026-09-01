use crate::util::{has_attr, policy_severity, sanitize, strip_comments_preserve_strings};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    analyze_build_surfaces(source, config, findings);
    analyze_layout_types(source, config, findings);
    for function in source.functions.iter().filter(|function| function.abi.is_some()) {
        analyze_ffi(function, config, findings);
    }
}

fn analyze_build_surfaces(
    source: &WorkspaceSource,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    for file in &source.files {
        let name = file.path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        let code = sanitize(&file.text);
        if name == "build.rs" {
            analyze_build_rs(file, config, &code, findings);
        }
        if ["proc_macro", "proc_macro2"].iter().any(|needle| code.contains(needle)) {
            analyze_proc_macro(file, config, &code, findings);
        }
    }
}

fn analyze_layout_types(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    for ty in source.types.iter().filter(|ty| has_attr(&ty.attributes, "critical_layout")) {
        check_layout_repr(ty, config, findings);
        check_packed_layout(ty, config, findings);
        check_raw_layout_casts(source, ty, config, findings);
    }
}

fn check_layout_repr(ty: &qa_syntax::SourceType, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.layout.critical_requires_repr {
        return;
    }
    let attrs = ty.attributes.join(" ");
    let compact = attrs.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let stable =
        ["repr(C", "repr(transparent", "repr(packed"].iter().any(|needle| compact.contains(needle));
    if !stable {
        findings.push(Finding {
            rule_id: "QA-LAYOUT-001".into(),
            severity: Severity::High,
            message: format!(
                "Critical layout type `{}` lacks an ABI/layout-stable repr contract",
                ty.name
            ),
            path: Some(ty.path.display().to_string()),
            line: Some(ty.line),
            detail: Some(
                "Use repr(C)/repr(transparent), or document a target-specific packed layout contract."
                    .into(),
            ),
        });
    }
}

fn check_packed_layout(ty: &qa_syntax::SourceType, config: &QaConfig, findings: &mut Vec<Finding>) {
    let attrs = ty.attributes.join(" ");
    if config.layout.deny_packed_references && attrs.contains("packed") {
        findings.push(Finding {
            rule_id: "QA-LAYOUT-008".into(),
            severity: Severity::Medium,
            message: format!("Packed critical type `{}` requires explicit unaligned-access audit", ty.name),
            path: Some(ty.path.display().to_string()),
            line: Some(ty.line),
            detail: Some(
                "Creating references to unaligned packed fields can be invalid; use raw pointers/read_unaligned where appropriate."
                    .into(),
            ),
        });
    }
}

fn check_raw_layout_casts(
    source: &WorkspaceSource,
    ty: &qa_syntax::SourceType,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.layout.deny_raw_padded_byte_casts {
        return;
    }
    let leaf = ty.name.rsplit("::").next().unwrap_or(&ty.name);
    for function in &source.functions {
        let body = sanitize(&function.source);
        if body.contains(leaf) && raw_byte_cast(&body) {
            findings.push(Finding {
                rule_id: "QA-LAYOUT-006".into(),
                severity: Severity::High,
                message: format!(
                    "Raw byte reinterpretation of critical layout type `{}` requires padding/validity proof",
                    ty.name
                ),
                path: Some(function.path.display().to_string()),
                line: Some(function.line),
                detail: None,
            });
        }
    }
}

fn raw_byte_cast(code: &str) -> bool {
    ["transmute", "from_raw_parts", "slice::from_raw_parts", "as *const u8", "as *mut u8"]
        .iter()
        .any(|needle| code.contains(needle))
}

fn analyze_build_rs(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    check_build_network(file, config, code, findings);
    check_build_process(file, config, code, findings);
    check_build_writes(file, config, code, findings);
    check_build_inputs(file, config, code, findings);
}

fn check_build_network(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let network = ["TcpStream", "UdpSocket", "reqwest", "ureq", "curl", "hyper::Client"]
        .iter()
        .any(|needle| code.contains(needle));
    if config.build.deny_network && network {
        emit_file(
            findings,
            file,
            "QA-BUILD-003",
            Severity::High,
            "build.rs contains network-capable API; strict builds must be network-independent",
        );
    }
}

fn check_build_process(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let process =
        ["Command::new", "std::process::Command"].iter().any(|needle| code.contains(needle));
    if !config.build.process_spawn.eq_ignore_ascii_case("allow") && process {
        emit_file(
            findings,
            file,
            "QA-BUILD-004",
            policy_severity(&config.build.process_spawn),
            "build.rs spawns a subprocess; hermetic tool allowlisting is required",
        );
    }
}

fn check_build_writes(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let writes =
        ["fs::write", "File::create", "OpenOptions"].iter().any(|needle| code.contains(needle));
    let build_source = strip_comments_preserve_strings(&file.text);
    let outside = !build_source.contains("OUT_DIR");
    if !config.build.writes_outside_out_dir.eq_ignore_ascii_case("allow") && writes && outside {
        emit_file(
            findings,
            file,
            "QA-BUILD-002",
            policy_severity(&config.build.writes_outside_out_dir),
            "build.rs contains a write path without a recognized OUT_DIR anchor",
        );
    }
}

fn check_build_inputs(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let reads =
        ["fs::read", "read_to_string", "env::var"].iter().any(|needle| code.contains(needle));
    let rerun = file.text.contains("rerun-if-");
    if config.build.require_rerun_directives && reads && !rerun {
        emit_file(
            findings,
            file,
            "QA-BUILD-006",
            Severity::Medium,
            "build.rs consumes file/environment inputs without recognized cargo::rerun-if-* declaration",
        );
    }
}

fn analyze_proc_macro(
    file: &qa_syntax::SourceFile,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.build.deny_network
        && ["TcpStream", "reqwest", "ureq", "curl"].iter().any(|t| code.contains(t))
    {
        emit_file(
            findings,
            file,
            "QA-BUILD-007",
            Severity::High,
            "Procedural-macro code contains network-capable API",
        );
    }
    if code.contains("Command::new") && !config.build.process_spawn.eq_ignore_ascii_case("allow") {
        emit_file(
            findings,
            file,
            "QA-BUILD-008",
            policy_severity(&config.build.process_spawn),
            "Procedural-macro code spawns subprocesses",
        );
    }
    if code.contains("std::env::var") || code.contains("env::var(") {
        emit_file(
            findings,
            file,
            "QA-BUILD-009",
            Severity::Medium,
            "Procedural-macro expansion depends on process environment; review determinism",
        );
    }
}
fn analyze_ffi(function: &SourceFunction, config: &QaConfig, findings: &mut Vec<Finding>) {
    let code = sanitize(&function.source);
    check_ffi_safety_docs(function, config, findings);
    check_ffi_panic(function, config, &code, findings);
    check_ffi_signature(function, findings);
    check_ffi_raw_pointer(function, findings);
}

fn check_ffi_safety_docs(
    function: &SourceFunction,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.ffi.require_safety_docs || !function.is_unsafe {
        return;
    }
    let attrs = function.attributes.join(" ");
    let documented = ["# Safety", "Safety"].iter().any(|needle| attrs.contains(needle))
        || function.source.contains("SAFETY");
    if !documented {
        emit_fn(
            findings,
            function,
            "QA-FFI-003",
            Severity::High,
            "Unsafe FFI function lacks a recognized # Safety/SAFETY contract",
        );
    }
}

fn check_ffi_panic(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.ffi.deny_panic_across_boundary {
        return;
    }
    let panic = ["panic!(", ".unwrap()", ".expect(", "unreachable!("]
        .iter()
        .any(|needle| code.contains(needle));
    if panic {
        emit_fn(
            findings,
            function,
            "QA-FFI-002",
            Severity::Critical,
            "FFI function may unwind/panic across the ABI boundary",
        );
    }
}

fn check_ffi_signature(function: &SourceFunction, findings: &mut Vec<Finding>) {
    let unstable = ["String", "Vec <", "Vec<", "& str", "&str", "dyn ", "Box < dyn", "Box<dyn"]
        .iter()
        .any(|needle| function.source.contains(needle));
    if unstable {
        emit_fn(
            findings,
            function,
            "QA-FFI-001",
            Severity::High,
            "FFI signature appears to expose a Rust-layout/ownership type that is not C ABI-stable",
        );
    }
}

fn check_ffi_raw_pointer(function: &SourceFunction, findings: &mut Vec<Finding>) {
    let raw_pointer = ["* const", "*const", "* mut", "*mut"]
        .iter()
        .any(|needle| function.source.contains(needle));
    if raw_pointer && !function.is_unsafe {
        emit_fn(
            findings,
            function,
            "QA-FFI-004",
            Severity::Medium,
            "Safe FFI function accepts raw pointers; caller validity contract should be explicit",
        );
    }
}

fn emit_file(
    f: &mut Vec<Finding>,
    file: &qa_syntax::SourceFile,
    id: &str,
    severity: Severity,
    message: &str,
) {
    f.push(Finding {
        rule_id: id.into(),
        severity,
        message: message.into(),
        path: Some(file.path.display().to_string()),
        line: Some(1),
        detail: None,
    })
}
fn emit_fn(
    f: &mut Vec<Finding>,
    function: &SourceFunction,
    id: &str,
    severity: Severity,
    message: &str,
) {
    f.push(Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message}: `{}`", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: None,
    })
}

#[cfg(test)]
mod tests;
