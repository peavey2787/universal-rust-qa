use qa_policy::QaConfig;
use std::{
    io::{self, Write},
    path::Path,
    str::FromStr,
};

type SettingsResult = Result<(), Box<dyn std::error::Error>>;
type SettingsAction = fn(&Path, &mut QaConfig) -> SettingsResult;

const SETTINGS_ACTIONS: [(&str, SettingsAction); 11] = [
    ("1", action_summary),
    ("2", action_structural),
    ("3", action_state_async),
    ("4", action_security),
    ("5", action_dynamic),
    ("6", action_platform),
    ("7", action_systems),
    ("8", action_release),
    ("9", action_viewer),
    ("10", action_exceptions),
    ("a", action_open_config),
];

pub fn menu(workspace: &Path) -> SettingsResult {
    loop {
        let mut config = QaConfig::load(workspace)?;
        println!(
            "\nSettings\n 1 Summary thresholds (LOC / CC / CRAP / coverage / duplicate / dead)\n 2 Structural & test policy\n 3 State / async / concurrency\n 4 Errors / secrets / constant-time\n 5 Sanitizers / differential / fault / MIR\n 6 Platform / build / layout / FFI\n 7 Hardware / performance / binary hardening\n 8 Release engineering / reproducibility\n 9 Viewer / report opener\n10 Exceptions\n A Open complete qa.toml\n B Back"
        );
        let choice = prompt("settings> ")?.to_ascii_lowercase();
        if is_back(&choice) {
            break;
        }
        if let Some(action) = settings_action(&choice) {
            action(workspace, &mut config)?;
        }
    }
    Ok(())
}

fn settings_action(choice: &str) -> Option<SettingsAction> {
    SETTINGS_ACTIONS.iter().find(|(key, _)| *key == choice).map(|(_, action)| *action)
}

fn action_summary(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    summary(config)?;
    save(workspace, config)
}

fn action_structural(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    structural(config)?;
    save(workspace, config)
}

fn action_state_async(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    state_async(config)?;
    save(workspace, config)
}

fn action_security(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    security(config)?;
    save(workspace, config)
}

fn action_dynamic(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    dynamic(config)?;
    save(workspace, config)
}

fn action_platform(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    platform(config)?;
    save(workspace, config)
}

fn action_systems(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    systems(config)?;
    save(workspace, config)
}

fn action_release(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    release(config)?;
    save(workspace, config)
}

fn action_viewer(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    viewer(config)?;
    save(workspace, config)
}

fn action_exceptions(workspace: &Path, _: &mut QaConfig) -> SettingsResult {
    crate::exceptions::menu(workspace)
}

fn action_open_config(workspace: &Path, config: &mut QaConfig) -> SettingsResult {
    crate::editor::open(&config.viewer, &workspace.join("qa.toml"), 1)
}

fn save(workspace: &Path, config: &QaConfig) -> SettingsResult {
    config.save(&workspace.join("qa.toml"))?;
    Ok(())
}

fn is_back(choice: &str) -> bool {
    choice.is_empty() || choice.eq_ignore_ascii_case("b")
}

fn summary(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nSummary thresholds\n 1 File LOC                {}\n 2 Function LOC            {}\n 3 Cyclomatic complexity   {}\n 4 Cognitive complexity    {}\n 5 CRAP                    {:.1}\n 6 Coverage %              {:.1}\n 7 Duplicate %             {:.1}\n 8 Dead/unreachable %      {:.1}\n B Back",
            c.metrics.file_loc,
            c.metrics.function_loc,
            c.metrics.cyclomatic,
            c.metrics.cognitive,
            c.metrics.crap,
            c.metrics.coverage_percent,
            c.metrics.duplicate_percent,
            c.metrics.dead_code_percent
        );
        match prompt("summary> ")?.as_str() {
            "1" => set(&mut c.metrics.file_loc, "file LOC")?,
            "2" => set(&mut c.metrics.function_loc, "function LOC")?,
            "3" => set(&mut c.metrics.cyclomatic, "CC")?,
            "4" => set(&mut c.metrics.cognitive, "cognitive")?,
            "5" => set(&mut c.metrics.crap, "CRAP")?,
            "6" => set(&mut c.metrics.coverage_percent, "coverage")?,
            "7" => set(&mut c.metrics.duplicate_percent, "duplicate %")?,
            "8" => set(&mut c.metrics.dead_code_percent, "dead %")?,
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
fn structural(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nStructural & tests\n 1 Function statements      {}\n 2 Parameters               {}\n 3 Generic parameters       {}\n 4 Duplicate min LOC        {}\n 5 Near clone similarity    {:.2}\n 6 Dead-code closed world   {}\n 7 Test production reach    {}\n B Back",
            c.sprawl.function_statements,
            c.sprawl.parameters,
            c.sprawl.generic_parameters,
            c.duplicates.minimum_loc,
            c.duplicates.near_clone_similarity,
            c.dead_code.closed_world,
            c.tests.require_production_reachability
        );
        match prompt("structural> ")?.as_str() {
            "1" => set(&mut c.sprawl.function_statements, "statements")?,
            "2" => set(&mut c.sprawl.parameters, "parameters")?,
            "3" => set(&mut c.sprawl.generic_parameters, "generics")?,
            "4" => set(&mut c.duplicates.minimum_loc, "duplicate min LOC")?,
            "5" => set(&mut c.duplicates.near_clone_similarity, "similarity")?,
            "6" => toggle(&mut c.dead_code.closed_world),
            "7" => toggle(&mut c.tests.require_production_reachability),
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
fn state_async(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nState / async / concurrency\n 1 State analysis               {}\n 2 State round-trip required    {}\n 3 State restart required       {}\n 4 Async analysis               {}\n 5 Cancellation contract        {}\n 6 Blocking async policy        {}\n 7 Detached task policy         {}\n 8 Relaxed atomic policy        {}\n B Back",
            c.state.enabled,
            c.state.require_roundtrip_contract,
            c.state.require_restart_contract,
            c.async_rules.enabled,
            c.async_rules.critical_requires_cancellation_contract,
            c.async_rules.blocking_calls,
            c.async_rules.detached_tasks,
            c.async_rules.relaxed_atomics
        );
        match prompt("state/async> ")?.as_str() {
            "1" => toggle(&mut c.state.enabled),
            "2" => toggle(&mut c.state.require_roundtrip_contract),
            "3" => toggle(&mut c.state.require_restart_contract),
            "4" => toggle(&mut c.async_rules.enabled),
            "5" => toggle(&mut c.async_rules.critical_requires_cancellation_contract),
            "6" => text(&mut c.async_rules.blocking_calls, "policy")?,
            "7" => text(&mut c.async_rules.detached_tasks, "policy")?,
            "8" => text(&mut c.async_rules.relaxed_atomics, "policy")?,
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
fn security(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nError / secret / constant-time\n 1 Discarded Result policy  {}\n 2 Secret logging policy    {}\n 3 Broken source policy     {}\n 4 Require Zeroize          {}\n 5 Deny Debug/Display       {}\n 6 Constant-time static     {}\n B Back",
            c.errors.discarded_results,
            c.errors.secret_logging,
            c.errors.broken_sources,
            c.secrets.require_zeroize,
            c.secrets.deny_debug_display,
            c.constant_time.enabled
        );
        match prompt("security> ")?.as_str() {
            "1" => text(&mut c.errors.discarded_results, "policy")?,
            "2" => text(&mut c.errors.secret_logging, "policy")?,
            "3" => text(&mut c.errors.broken_sources, "policy")?,
            "4" => toggle(&mut c.secrets.require_zeroize),
            "5" => toggle(&mut c.secrets.deny_debug_display),
            "6" => toggle(&mut c.constant_time.enabled),
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
#[qa_attr::allow(cc = 20, reason = "Interactive dynamic-analysis settings dispatcher")]
fn dynamic(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nDynamic/compiler assurance\n 1 Sanitizer mode             {}\n 2 Sanitizer toolchain        {}\n 3 MSan complete instrument   {}\n 4 Differential enabled      {}\n 5 Differential seed         {}\n 6 Fault enabled             {}\n 7 Fault seed                {}\n 8 Fault max points          {}\n 9 MIR mode                  {}\n10 MIR toolchain             {}\n B Back",
            c.sanitizers.mode,
            c.sanitizers.toolchain,
            c.sanitizers.msan_complete_instrumentation,
            c.differential.enabled,
            c.differential.seed,
            c.fault.enabled,
            c.fault.seed,
            c.fault.max_fail_points,
            c.mir.mode,
            c.mir.toolchain
        );
        match prompt("dynamic> ")?.as_str() {
            "1" => text(&mut c.sanitizers.mode, "mode")?,
            "2" => text(&mut c.sanitizers.toolchain, "toolchain")?,
            "3" => toggle(&mut c.sanitizers.msan_complete_instrumentation),
            "4" => toggle(&mut c.differential.enabled),
            "5" => set(&mut c.differential.seed, "seed")?,
            "6" => toggle(&mut c.fault.enabled),
            "7" => set(&mut c.fault.seed, "seed")?,
            "8" => set(&mut c.fault.max_fail_points, "points")?,
            "9" => text(&mut c.mir.mode, "mode")?,
            "10" => text(&mut c.mir.toolchain, "toolchain")?,
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
#[qa_attr::allow(cc = 20, reason = "Interactive platform settings dispatcher")]
fn platform(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nPlatform / build / layout / FFI\n 1 Default features        {}\n 2 No-default features     {}\n 3 All features            {}\n 4 Each feature            {}\n 5 MSRV                    {}\n 6 Build network denied    {}\n 7 Build process policy    {}\n 8 Critical repr required  {}\n 9 FFI safety docs         {}\n10 FFI panic denied        {}\n B Back",
            c.platform.check_default,
            c.platform.check_no_default,
            c.platform.check_all_features,
            c.platform.check_each_feature,
            c.platform.check_msrv,
            c.build.deny_network,
            c.build.process_spawn,
            c.layout.critical_requires_repr,
            c.ffi.require_safety_docs,
            c.ffi.deny_panic_across_boundary
        );
        match prompt("platform> ")?.as_str() {
            "1" => toggle(&mut c.platform.check_default),
            "2" => toggle(&mut c.platform.check_no_default),
            "3" => toggle(&mut c.platform.check_all_features),
            "4" => toggle(&mut c.platform.check_each_feature),
            "5" => toggle(&mut c.platform.check_msrv),
            "6" => toggle(&mut c.build.deny_network),
            "7" => text(&mut c.build.process_spawn, "policy")?,
            "8" => toggle(&mut c.layout.critical_requires_repr),
            "9" => toggle(&mut c.ffi.require_safety_docs),
            "10" => toggle(&mut c.ffi.deny_panic_across_boundary),
            "b" | "B" | "" => break,
            _ => {}
        }
    }
    Ok(())
}

fn systems(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nHardware / performance / hardening\n 1 Hardware enabled             {}\n 2 ISR stack budget bytes       {}\n 3 Deny heap in ISR             {}\n 4 Performance enabled          {}\n 5 False-sharing policy         {}\n 6 Instruction warn %           {:.1}\n 7 Instruction deny %           {:.1}\n 8 Binary bloat max growth %    {:.1}\n 9 Binary bloat max bytes       {}\n10 Binary hardening enabled     {}\n11 Release overflow checks      {}\n12 Require PIE                  {}\n13 Require full RELRO           {}\n B Back",
            c.hardware.enabled,
            c.hardware.interrupt_stack_budget_bytes,
            c.hardware.deny_heap_in_interrupts,
            c.performance.enabled,
            c.performance.false_sharing,
            c.performance.instruction_warn_percent,
            c.performance.instruction_deny_percent,
            c.bloat.max_percent_growth,
            c.bloat.max_absolute_growth_bytes,
            c.hardening.enabled,
            c.hardening.release_overflow_checks,
            c.hardening.require_pie,
            c.hardening.require_full_relro
        );
        let choice = prompt("systems> ")?;
        if is_back(&choice) {
            break;
        }
        if !systems_primary(c, &choice)? {
            let _ = systems_secondary(c, &choice)?;
        }
    }
    Ok(())
}

fn systems_primary(c: &mut QaConfig, choice: &str) -> io::Result<bool> {
    let handled = match choice {
        "1" => {
            toggle(&mut c.hardware.enabled);
            true
        }
        "2" => {
            set(&mut c.hardware.interrupt_stack_budget_bytes, "ISR stack bytes")?;
            true
        }
        "3" => {
            toggle(&mut c.hardware.deny_heap_in_interrupts);
            true
        }
        "4" => {
            toggle(&mut c.performance.enabled);
            true
        }
        "5" => {
            text(&mut c.performance.false_sharing, "policy")?;
            true
        }
        "6" => {
            set(&mut c.performance.instruction_warn_percent, "warn %")?;
            true
        }
        "7" => {
            set(&mut c.performance.instruction_deny_percent, "deny %")?;
            true
        }
        _ => false,
    };
    Ok(handled)
}

fn systems_secondary(c: &mut QaConfig, choice: &str) -> io::Result<bool> {
    let handled = match choice {
        "8" => {
            set(&mut c.bloat.max_percent_growth, "bloat max %")?;
            true
        }
        "9" => {
            set(&mut c.bloat.max_absolute_growth_bytes, "bloat max bytes")?;
            true
        }
        "10" => {
            toggle(&mut c.hardening.enabled);
            true
        }
        "11" => {
            toggle(&mut c.hardening.release_overflow_checks);
            true
        }
        "12" => {
            toggle(&mut c.hardening.require_pie);
            true
        }
        "13" => {
            toggle(&mut c.hardening.require_full_relro);
            true
        }
        _ => false,
    };
    Ok(handled)
}

fn release(c: &mut QaConfig) -> io::Result<()> {
    loop {
        println!(
            "\nRelease engineering / reproducibility\n 1 Snapshot auto-updates        {}\n 2 Pending snapshots            {}\n 3 Critical docs require example {}\n 4 Run doctests                 {}\n 5 Check examples               {}\n 6 Run cargo-deny               {}\n 7 Run unused-deps check        {}\n 8 Deny wildcard deps           {}\n 9 Run SemVer checks            {}\n10 Verify generated outputs     {}\n11 Reproducible builds          {}\n12 Repro runs                   {}\n13 Self-hardening               {}\n B Back",
            c.snapshots.ci_updates,
            c.snapshots.pending,
            c.documentation.critical_requires_example,
            c.documentation.run_doctests,
            c.documentation.check_examples,
            c.dependencies.run_cargo_deny,
            c.dependencies.run_unused,
            c.dependencies.deny_wildcards,
            c.api.run_semver_checks,
            c.generated.verify,
            c.reproducibility.enabled,
            c.reproducibility.runs,
            c.self_hardening.enabled
        );
        let choice = prompt("release> ")?;
        if is_back(&choice) {
            break;
        }
        if !release_primary(c, &choice)? {
            release_secondary(c, &choice)?;
        }
    }
    Ok(())
}

fn release_primary(c: &mut QaConfig, choice: &str) -> io::Result<bool> {
    let handled = match choice {
        "1" => {
            text(&mut c.snapshots.ci_updates, "snapshot update policy")?;
            true
        }
        "2" => {
            text(&mut c.snapshots.pending, "pending policy")?;
            true
        }
        "3" => {
            toggle(&mut c.documentation.critical_requires_example);
            true
        }
        "4" => {
            toggle(&mut c.documentation.run_doctests);
            true
        }
        "5" => {
            toggle(&mut c.documentation.check_examples);
            true
        }
        "6" => {
            toggle(&mut c.dependencies.run_cargo_deny);
            true
        }
        "7" => {
            toggle(&mut c.dependencies.run_unused);
            true
        }
        _ => false,
    };
    Ok(handled)
}

fn release_secondary(c: &mut QaConfig, choice: &str) -> io::Result<bool> {
    let handled = match choice {
        "8" => {
            toggle(&mut c.dependencies.deny_wildcards);
            true
        }
        "9" => {
            toggle(&mut c.api.run_semver_checks);
            true
        }
        "10" => {
            toggle(&mut c.generated.verify);
            true
        }
        "11" => {
            toggle(&mut c.reproducibility.enabled);
            true
        }
        "12" => {
            set(&mut c.reproducibility.runs, "repro runs")?;
            true
        }
        "13" => {
            toggle(&mut c.self_hardening.enabled);
            true
        }
        _ => false,
    };
    Ok(handled)
}

fn viewer(c: &mut QaConfig) -> io::Result<()> {
    println!("\nViewer\n command: {}\n args: {:?}", c.viewer.command, c.viewer.args);
    let v = prompt("New command (blank keeps current): ")?;
    if !v.is_empty() {
        c.viewer.command = v
    }
    let a = prompt("New args separated by | (use {path} and {line}; blank keeps): ")?;
    if !a.is_empty() {
        c.viewer.args = a.split('|').map(|x| x.trim().to_string()).collect()
    }
    Ok(())
}
fn set<T: FromStr + std::fmt::Display>(value: &mut T, name: &str) -> io::Result<()> {
    let s = prompt(&format!("New {name} [{}]: ", value))?;
    if let Ok(v) = s.parse() {
        *value = v
    }
    Ok(())
}
fn text(value: &mut String, name: &str) -> io::Result<()> {
    let s = prompt(&format!("New {name} [{}]: ", value))?;
    if !s.is_empty() {
        *value = s
    }
    Ok(())
}
fn toggle(v: &mut bool) {
    *v = !*v
}
fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

#[cfg(test)]
mod tests;
