use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// A planned desktop-runner capability declared by an adapter or module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub adapter: String,
    pub risk: RiskLevel,
}

/// Coarse risk level for a capability exposed by the desktop runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Validation error returned when a capability declaration is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    EmptyName,
    EmptyAdapter,
    DuplicateName(String),
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "capability name must not be empty"),
            Self::EmptyAdapter => write!(f, "capability adapter must not be empty"),
            Self::DuplicateName(name) => write!(f, "duplicate capability name: {name}"),
        }
    }
}

impl std::error::Error for CapabilityError {}

/// Validate and normalize capability declarations.
pub fn normalize_capabilities(
    capabilities: impl IntoIterator<Item = Capability>,
) -> Result<Vec<Capability>, CapabilityError> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for capability in capabilities {
        let name = capability.name.trim();
        let adapter = capability.adapter.trim();

        if name.is_empty() {
            return Err(CapabilityError::EmptyName);
        }

        if adapter.is_empty() {
            return Err(CapabilityError::EmptyAdapter);
        }

        if !seen.insert(name.to_owned()) {
            return Err(CapabilityError::DuplicateName(name.to_owned()));
        }

        normalized.push(Capability {
            name: name.to_owned(),
            adapter: adapter.to_owned(),
            risk: capability.risk,
        });
    }

    normalized.sort_by(|left, right| {
        left.risk
            .cmp(&right.risk)
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(normalized)
}

/// Deterministic CPU-bound workload used by fast performance and concurrency checks.
pub fn checksum_workload(iterations: u64) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325u64;

    for value in 0..iterations {
        state ^= value.wrapping_mul(0x1000_0000_01b3);
        state = state.rotate_left(13).wrapping_mul(0xff51_afd7_ed55_8ccd);
    }

    state
}

/// Reference to a local runner package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPackageRef {
    pub path: PathBuf,
    pub signed: bool,
    pub draft: bool,
}

impl RunnerPackageRef {
    pub fn local(path: impl AsRef<Path>, signed: bool, draft: bool) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            signed,
            draft,
        }
    }
}

/// Security decision returned when a runner package is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageDecision {
    Allowed,
    RejectedUnsignedPublished,
}

/// Decide whether a runner package may be loaded under the configured signing policy.
pub fn evaluate_runner_package(
    package: &RunnerPackageRef,
    require_signed_runners: bool,
    allow_unsigned_drafts: bool,
) -> PackageDecision {
    if package.signed || !require_signed_runners || (package.draft && allow_unsigned_drafts) {
        PackageDecision::Allowed
    } else {
        PackageDecision::RejectedUnsignedPublished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_sorts_capabilities() {
        let capabilities = normalize_capabilities([
            Capability {
                name: " replay ".to_owned(),
                adapter: " web ".to_owned(),
                risk: RiskLevel::Medium,
            },
            Capability {
                name: "info".to_owned(),
                adapter: "core".to_owned(),
                risk: RiskLevel::Low,
            },
        ])
        .expect("capabilities should normalize");

        assert_eq!(capabilities[0].name, "info");
        assert_eq!(capabilities[1].adapter, "web");
    }

    #[test]
    fn rejects_duplicate_capability_names() {
        let err = normalize_capabilities([
            Capability {
                name: "replay".to_owned(),
                adapter: "web".to_owned(),
                risk: RiskLevel::Medium,
            },
            Capability {
                name: "replay".to_owned(),
                adapter: "terminal".to_owned(),
                risk: RiskLevel::High,
            },
        ])
        .expect_err("duplicate names must fail");

        assert_eq!(err, CapabilityError::DuplicateName("replay".to_owned()));
    }

    #[test]
    fn checksum_workload_is_deterministic() {
        assert_eq!(checksum_workload(1_000), checksum_workload(1_000));
        assert_ne!(checksum_workload(1_000), checksum_workload(1_001));
    }

    #[test]
    fn rejects_unsigned_published_package_when_signatures_are_required() {
        let package = RunnerPackageRef::local("runner.gtpack", false, false);
        assert_eq!(
            evaluate_runner_package(&package, true, true),
            PackageDecision::RejectedUnsignedPublished
        );
    }

    #[test]
    fn allows_unsigned_drafts_when_policy_allows_them() {
        let package = RunnerPackageRef::local("draft.gtpack", false, true);
        assert_eq!(
            evaluate_runner_package(&package, true, true),
            PackageDecision::Allowed
        );
    }
}
