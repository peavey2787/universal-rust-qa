use crate::util::{has_attr, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.hardware.enabled {
        return;
    }
    for function in &source.functions {
        analyze_function(function, config, findings);
    }
}

fn analyze_function(function: &SourceFunction, config: &QaConfig, findings: &mut Vec<Finding>) {
    let attrs = function.attributes.join(" ");
    let code = sanitize(&function.source);
    check_mmio(function, &code, findings);
    if is_interrupt(function) {
        check_interrupt(function, config, &code, findings);
    }
    check_dma(function, &attrs, &code, findings);
}

fn check_mmio(function: &SourceFunction, code: &str, findings: &mut Vec<Finding>) {
    let marked = has_attr(&function.attributes, "mmio") || looks_like_mmio(code);
    if marked && raw_access_without_volatile(code) {
        emit(
            function,
            findings,
            "QA-HW-001",
            Severity::Critical,
            "MMIO-like raw pointer access lacks a recognized volatile access primitive",
        );
    }
}

fn check_interrupt(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    check_interrupt_stack(function, config, code, findings);
    check_interrupt_heap(function, config, code, findings);
    check_interrupt_blocking(function, config, code, findings);
    check_interrupt_panic(function, config, code, findings);
}

fn check_interrupt_stack(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    let stack = estimated_stack_bytes(code);
    if stack <= config.hardware.interrupt_stack_budget_bytes {
        return;
    }
    let mut item = finding(
        function,
        "QA-HW-002",
        Severity::High,
        "Interrupt handler estimated local stack exceeds configured interrupt budget",
    );
    item.detail = Some(format!(
        "estimated fixed local arrays: {stack} bytes; budget: {} bytes",
        config.hardware.interrupt_stack_budget_bytes
    ));
    findings.push(item);
}

fn check_interrupt_heap(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.hardware.deny_heap_in_interrupts {
        return;
    }
    let heap =
        ["Vec ::", "Vec::", "Box ::", "Box::", "String ::", "String::", "format !", "format!"]
            .iter()
            .any(|token| code.contains(token));
    if heap {
        emit(
            function,
            findings,
            "QA-HW-004",
            Severity::High,
            "Interrupt handler performs or may perform heap allocation/formatting",
        );
    }
}

fn check_interrupt_blocking(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.hardware.deny_blocking_in_interrupts {
        return;
    }
    let blocking = ["sleep (", "sleep(", "lock (", "lock(", "read_to_end", "std :: fs", "std::fs"]
        .iter()
        .any(|token| code.contains(token));
    if blocking {
        emit(
            function,
            findings,
            "QA-HW-004",
            Severity::High,
            "Interrupt handler contains a blocking operation",
        );
    }
}

fn check_interrupt_panic(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.hardware.deny_panic_in_interrupts {
        return;
    }
    let panics = ["panic !", "panic!", ". unwrap (", ".unwrap(", ". expect (", ".expect("]
        .iter()
        .any(|token| code.contains(token));
    if panics {
        emit(
            function,
            findings,
            "QA-HW-004",
            Severity::Critical,
            "Interrupt handler contains a panic-capable operation",
        );
    }
}

fn check_dma(function: &SourceFunction, attrs: &str, code: &str, findings: &mut Vec<Finding>) {
    if !has_attr(&function.attributes, "dma_buffer") {
        return;
    }
    let aligned =
        ["repr", "align"].iter().any(|token| attrs.contains(token) || code.contains(token));
    if !aligned {
        emit(
            function,
            findings,
            "QA-HW-006",
            Severity::Medium,
            "DMA-marked operation has no recognizable alignment/layout contract",
        );
    }
}

fn is_interrupt(function: &SourceFunction) -> bool {
    ["interrupt", "exception"].iter().any(|name| has_attr(&function.attributes, name))
}

fn looks_like_mmio(source: &str) -> bool {
    let raw_address = source.contains("0x") && source.contains(" as *");
    raw_address || ["MMIO", "mmio"].iter().any(|token| source.contains(token))
}

fn raw_access_without_volatile(source: &str) -> bool {
    let raw = ["* mut", "*mut", "* const", "*const"].iter().any(|token| source.contains(token));
    let volatile = ["read_volatile", "write_volatile", "volatile_register", "VolatileCell"]
        .iter()
        .any(|token| source.contains(token));
    raw && !volatile
}

fn estimated_stack_bytes(source: &str) -> usize {
    source.split('[').skip(1).filter_map(array_length).fold(0usize, usize::saturating_add)
}

fn array_length(fragment: &str) -> Option<usize> {
    let (_, after) = fragment.split_once(';')?;
    let digits = after.trim_start().chars().take_while(char::is_ascii_digit).collect::<String>();
    digits.parse().ok()
}

fn emit(
    function: &SourceFunction,
    findings: &mut Vec<Finding>,
    id: &str,
    severity: Severity,
    message: &str,
) {
    findings.push(finding(function, id, severity, message));
}

fn finding(function: &SourceFunction, id: &str, severity: Severity, message: &str) -> Finding {
    Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message}: `{}`", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: None,
    }
}

#[cfg(test)]
mod tests;
