use super::super::model::CoveragePackage;
use super::PackageState;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(super) struct CoverageScope {
    pub(super) eligible: Vec<CoveragePackage>,
    pub(super) covered: Vec<CoveragePackage>,
    pub(super) runtime_not_applicable: Vec<CoveragePackage>,
    pub(super) incomplete_baseline: bool,
}

pub(super) fn coverage_scope(
    packages: &[CoveragePackage],
    states: &BTreeMap<String, PackageState>,
    required_baselines: usize,
) -> CoverageScope {
    let runtime_not_applicable = packages
        .iter()
        .filter(|package| states.get(&package.name).is_some_and(|state| state.host_not_applicable))
        .cloned()
        .collect::<Vec<_>>();
    let eligible = packages
        .iter()
        .filter(|package| {
            !runtime_not_applicable.iter().any(|candidate| candidate.name == package.name)
        })
        .cloned()
        .collect::<Vec<_>>();
    let covered = eligible
        .iter()
        .filter(|package| {
            states.get(&package.name).is_some_and(|state| state.baseline_successes > 0)
        })
        .cloned()
        .collect::<Vec<_>>();
    let incomplete_baseline = eligible.iter().any(|package| {
        states.get(&package.name).is_none_or(|state| state.baseline_successes < required_baselines)
    });
    CoverageScope { eligible, covered, runtime_not_applicable, incomplete_baseline }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str) -> CoveragePackage {
        CoveragePackage {
            name: name.into(),
            root: format!("/ws/{name}"),
            source_loc: 10,
            default_member: false,
        }
    }

    #[test]
    fn one_successful_target_preserves_package_evidence_while_incomplete_targets_degrade_scope() {
        let packages = vec![package("wallet")];
        let states = BTreeMap::from([(
            "wallet".into(),
            PackageState { baseline_successes: 1, ..PackageState::default() },
        )]);
        let scope = coverage_scope(&packages, &states, 2);
        assert_eq!(scope.covered.len(), 1);
        assert!(scope.incomplete_baseline);
    }
}
