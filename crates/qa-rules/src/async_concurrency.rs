use crate::util::{has_attr, policy_severity, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.async_rules.enabled {
        return;
    }
    for function in &source.functions {
        analyze_function(function, config, findings);
    }
    analyze_send_sync(source, findings);
    analyze_static_mut(source, config, findings);
}

fn analyze_function(function: &SourceFunction, config: &QaConfig, findings: &mut Vec<Finding>) {
    let code = sanitize(&function.source);
    if function.is_async {
        analyze_async_function(function, config, &code, findings);
    }
    check_drop_panic(function, &code, findings);
    check_relaxed_atomic(function, config, &code, findings);
}

fn analyze_async_function(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    check_blocking(function, config, code, findings);
    check_detached(function, config, code, findings);
    check_await_lock(function, config, code, findings);
    check_cancellation_contract(function, config, findings);
}

fn check_blocking(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if blocking_call(code) {
        emit(
            findings,
            function,
            "QA-ASYNC-003",
            policy_severity(&config.async_rules.blocking_calls),
            "Blocking operation appears in async context",
        );
    }
}

fn check_detached(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if detached_spawn(code) {
        emit(
            findings,
            function,
            "QA-ASYNC-004",
            policy_severity(&config.async_rules.detached_tasks),
            "Spawned task has no recognized retained/supervised handle",
        );
    }
}

fn check_await_lock(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if guard_may_cross_await(code) {
        emit(
            findings,
            function,
            "QA-ASYNC-005",
            policy_severity(&config.async_rules.await_holding_lock),
            "Blocking lock/borrow guard may be held across await",
        );
    }
}

fn check_cancellation_contract(
    function: &SourceFunction,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    let critical = has_attr(&function.attributes, "critical_async");
    let required = config.async_rules.critical_requires_cancellation_contract;
    let declared =
        ["cancel_safe", "cancel_unsafe"].iter().any(|name| has_attr(&function.attributes, name));
    if critical && required && !declared {
        emit(
            findings,
            function,
            "QA-ASYNC-001",
            Severity::High,
            "Critical async function lacks explicit cancellation-safety contract",
        );
    }
}

fn check_drop_panic(function: &SourceFunction, code: &str, findings: &mut Vec<Finding>) {
    let panic_capable = [".unwrap()", ".expect(", "panic!(", "unreachable!("]
        .iter()
        .any(|token| code.contains(token));
    if is_drop(function) && panic_capable {
        emit(
            findings,
            function,
            "QA-ASYNC-002",
            Severity::Critical,
            "Drop implementation contains a panic-capable path",
        );
    }
}

fn check_relaxed_atomic(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let critical = has_attr(&function.attributes, "critical_concurrency");
    let relaxed = code.contains("Ordering::Relaxed");
    let allowed = config.async_rules.relaxed_atomics.eq_ignore_ascii_case("allow");
    if critical && relaxed && !allowed {
        emit(
            findings,
            function,
            "QA-CONC-006",
            policy_severity(&config.async_rules.relaxed_atomics),
            "Relaxed atomic ordering in critical concurrency code requires an explicit memory-ordering rationale",
        );
    }
}

fn analyze_send_sync(source: &WorkspaceSource, findings: &mut Vec<Finding>) {
    for interface in &source.interfaces {
        let unsafe_impl = interface.source.contains("unsafe impl");
        let marker_trait =
            ["Send for", "Sync for"].iter().any(|needle| interface.source.contains(needle));
        if unsafe_impl && marker_trait && !interface.source.contains("SAFETY") {
            findings.push(Finding {
                rule_id: "QA-CONC-003".into(),
                severity: Severity::High,
                message: "unsafe Send/Sync implementation lacks SAFETY rationale".into(),
                path: Some(interface.path.display().to_string()),
                line: Some(interface.line),
                detail: None,
            });
        }
    }
}

fn analyze_static_mut(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if config.async_rules.static_mut.eq_ignore_ascii_case("allow") {
        return;
    }
    for file in &source.files {
        let code = sanitize(&file.text);
        for (offset, _) in code.lines().enumerate().filter(|(_, line)| line.contains("static mut "))
        {
            findings.push(Finding {
                rule_id: "QA-CONC-004".into(),
                severity: policy_severity(&config.async_rules.static_mut),
                message: "Shared mutable static requires explicit synchronization/exception".into(),
                path: Some(file.path.display().to_string()),
                line: Some(offset + 1),
                detail: None,
            });
        }
    }
}

fn blocking_call(code: &str) -> bool {
    [
        "std::thread::sleep(",
        "std::fs::",
        "std::net::TcpStream",
        "std::net::UdpSocket",
        "Command::output(",
        "Command::status(",
    ]
    .iter()
    .any(|t| code.contains(t))
}
fn detached_spawn(code: &str) -> bool {
    const TOKENS: [&str; 3] = ["tokio::spawn(", "async_std::task::spawn(", "spawn(async"];
    code.split(';').any(|statement| {
        let statement = statement.trim();
        let spawn = TOKENS.iter().any(|token| statement.contains(token));
        let method_spawn = statement.contains(".spawn(async");
        let retained = statement.starts_with("let ") || statement.contains("JoinSet");
        spawn && !method_spawn && !retained
    })
}

fn guard_may_cross_await(code: &str) -> bool {
    let Some(await_at) = code.find(".await") else { return false };
    let before = &code[..await_at];
    let lock_at = before
        .rfind(".lock()")
        .or_else(|| before.rfind(".read()"))
        .or_else(|| before.rfind(".write()"));
    let Some(lock_at) = lock_at else { return false };
    let after_lock = &before[lock_at..];
    !after_lock.contains("drop(")
}
fn is_drop(function: &SourceFunction) -> bool {
    function.name == "drop"
        && (function.qualified_name.contains("Drop for") || has_attr(&function.attributes, "drop"))
}
fn emit(
    findings: &mut Vec<Finding>,
    function: &SourceFunction,
    id: &str,
    severity: Severity,
    message: &str,
) {
    findings.push(Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message}: `{}`", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: Some(
            "Source-level heuristic; compiler/MIR evidence can refine this result in Phase 14."
                .into(),
        ),
    })
}

#[cfg(test)]
mod tests;
