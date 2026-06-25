use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RunnerLifecycle {
    Draft,
    Tested,
    Approved,
    Published,
    Deprecated,
    Archived,
}

impl RunnerLifecycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Tested => "tested",
            Self::Approved => "approved",
            Self::Published => "published",
            Self::Deprecated => "deprecated",
            Self::Archived => "archived",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Tested)
                | (Self::Tested, Self::Approved)
                | (Self::Approved, Self::Published)
                | (Self::Published, Self::Deprecated)
                | (Self::Deprecated, Self::Archived)
                | (Self::Published, Self::Archived)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegistryStage {
    Dev,
    Staging,
    Prod,
}

impl RegistryStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VersionSelector {
    Exact(String),
    Channel(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantScope {
    pub tenant_id: String,
    pub team_id: String,
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerManifest {
    pub runner_id: String,
    pub version: String,
    pub lifecycle: RunnerLifecycle,
    pub stage: RegistryStage,
    pub scope: TenantScope,
    pub required_adapters: Vec<String>,
    pub compatibility: Vec<String>,
    pub package_checksum: String,
}

impl RunnerManifest {
    pub fn package_ref(&self) -> String {
        format!("{}@{}", self.runner_id, self.version)
    }

    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.runner_id.trim().is_empty() {
            return Err(RegistryError::InvalidManifest(
                "runner id is empty".to_owned(),
            ));
        }
        if !is_semver(&self.version) {
            return Err(RegistryError::InvalidManifest(format!(
                "runner version {} is not semantic",
                self.version
            )));
        }
        if self.required_adapters.is_empty() {
            return Err(RegistryError::InvalidManifest(
                "at least one required adapter must be declared".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn render_reviewable(&self) -> String {
        let adapters = self
            .required_adapters
            .iter()
            .map(|adapter| format!("  - {adapter}"))
            .collect::<Vec<_>>()
            .join("\n");
        let compatibility = self
            .compatibility
            .iter()
            .map(|item| format!("  - {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "runner_id: {}\nversion: {}\nlifecycle: {}\nstage: {}\ntenant: {}\nteam: {}\nprivate: {}\nrequired_adapters:\n{}\ncompatibility:\n{}\npackage_checksum: {}\n",
            self.runner_id,
            self.version,
            self.lifecycle.as_str(),
            self.stage.as_str(),
            self.scope.tenant_id,
            self.scope.team_id,
            self.scope.private,
            adapters,
            compatibility,
            self.package_checksum,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningKey {
    pub key_id: String,
    key_material: String,
}

impl SigningKey {
    pub fn new(key_id: impl Into<String>, key_material: impl Into<String>) -> Self {
        Self {
            key_id: key_id.into(),
            key_material: key_material.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRunnerManifest {
    pub manifest: RunnerManifest,
    pub key_id: String,
    pub signature: String,
}

impl SignedRunnerManifest {
    pub fn verify(&self, key: &SigningKey) -> Result<(), RegistryError> {
        self.manifest.validate()?;
        if self.key_id != key.key_id {
            return Err(RegistryError::InvalidSignature);
        }
        let expected = signature_for(&self.manifest, key);
        if self.signature == expected {
            Ok(())
        } else {
            Err(RegistryError::TamperedPackage)
        }
    }

    pub fn render_reviewable(&self) -> String {
        format!(
            "{}signature:\n  key_id: {}\n  value: {}\n",
            self.manifest.render_reviewable(),
            self.key_id,
            self.signature
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidManifest(String),
    InvalidLifecycleTransition {
        from: RunnerLifecycle,
        to: RunnerLifecycle,
    },
    InvalidSignature,
    TamperedPackage,
    NotFound(String),
    PublishedRunnerMustBeSigned,
    ScopeMismatch,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest(message) => write!(f, "{message}"),
            Self::InvalidLifecycleTransition { from, to } => {
                write!(
                    f,
                    "invalid lifecycle transition from {} to {}",
                    from.as_str(),
                    to.as_str()
                )
            }
            Self::InvalidSignature => write!(f, "invalid runner manifest signature"),
            Self::TamperedPackage => write!(f, "runner package manifest was tampered"),
            Self::NotFound(reference) => write!(f, "runner {reference} not found"),
            Self::PublishedRunnerMustBeSigned => write!(f, "published runners must be signed"),
            Self::ScopeMismatch => write!(f, "runner scope does not match requester"),
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, Clone, Default)]
pub struct RunnerRegistry {
    manifests: BTreeMap<String, SignedRunnerManifest>,
    channels: BTreeMap<(String, String, RegistryStage), String>,
}

impl RunnerRegistry {
    pub fn publish(
        &mut self,
        signed: SignedRunnerManifest,
        key: &SigningKey,
    ) -> Result<(), RegistryError> {
        if signed.manifest.lifecycle == RunnerLifecycle::Published {
            signed.verify(key)?;
        } else if signed.signature.is_empty() {
            return Err(RegistryError::PublishedRunnerMustBeSigned);
        }
        let registry_key = registry_key(&signed.manifest);
        self.channels.insert(
            (
                signed.manifest.runner_id.clone(),
                signed.manifest.scope.tenant_id.clone(),
                signed.manifest.stage,
            ),
            signed.manifest.version.clone(),
        );
        self.manifests.insert(registry_key, signed);
        Ok(())
    }

    pub fn resolve(
        &self,
        runner_id: &str,
        tenant_id: &str,
        selector: VersionSelector,
        stage: RegistryStage,
    ) -> Result<&SignedRunnerManifest, RegistryError> {
        let version = match selector {
            VersionSelector::Exact(version) => version,
            VersionSelector::Channel(channel)
                if channel == stage.as_str() || channel == "stable" =>
            {
                self.channels
                    .get(&(runner_id.to_owned(), tenant_id.to_owned(), stage))
                    .cloned()
                    .ok_or_else(|| RegistryError::NotFound(format!("{runner_id}@{channel}")))?
            }
            VersionSelector::Channel(channel) => {
                return Err(RegistryError::NotFound(format!("{runner_id}@{channel}")));
            }
        };
        self.manifests
            .get(&format!("{tenant_id}/{runner_id}@{version}"))
            .ok_or_else(|| RegistryError::NotFound(format!("{runner_id}@{version}")))
    }

    pub fn transition(
        mut signed: SignedRunnerManifest,
        next: RunnerLifecycle,
        key: &SigningKey,
    ) -> Result<SignedRunnerManifest, RegistryError> {
        if !signed.manifest.lifecycle.can_transition_to(next) {
            return Err(RegistryError::InvalidLifecycleTransition {
                from: signed.manifest.lifecycle,
                to: next,
            });
        }
        signed.manifest.lifecycle = next;
        sign_manifest(signed.manifest, key)
    }

    pub fn promote(
        mut signed: SignedRunnerManifest,
        to: RegistryStage,
        key: &SigningKey,
    ) -> Result<SignedRunnerManifest, RegistryError> {
        signed.manifest.stage = to;
        sign_manifest(signed.manifest, key)
    }
}

pub fn sign_manifest(
    manifest: RunnerManifest,
    key: &SigningKey,
) -> Result<SignedRunnerManifest, RegistryError> {
    manifest.validate()?;
    if manifest.lifecycle == RunnerLifecycle::Published && key.key_id.trim().is_empty() {
        return Err(RegistryError::PublishedRunnerMustBeSigned);
    }
    Ok(SignedRunnerManifest {
        signature: signature_for(&manifest, key),
        manifest,
        key_id: key.key_id.clone(),
    })
}

fn signature_for(manifest: &RunnerManifest, key: &SigningKey) -> String {
    deterministic_hash(&format!(
        "{}\nkey:{}\nmaterial:{}",
        manifest.render_reviewable(),
        key.key_id,
        key.key_material
    ))
}

fn registry_key(manifest: &RunnerManifest) -> String {
    format!(
        "{}/{}@{}",
        manifest.scope.tenant_id, manifest.runner_id, manifest.version
    )
}

fn is_semver(version: &str) -> bool {
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn deterministic_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SigningKey {
        SigningKey::new("local-dev", "test-material")
    }

    fn manifest() -> RunnerManifest {
        RunnerManifest {
            runner_id: "crm.create_customer".to_owned(),
            version: "1.2.0".to_owned(),
            lifecycle: RunnerLifecycle::Published,
            stage: RegistryStage::Staging,
            scope: TenantScope {
                tenant_id: "tenant_a".to_owned(),
                team_id: "sales_ops".to_owned(),
                private: true,
            },
            required_adapters: vec!["greentic.desktop.playwright".to_owned()],
            compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
            package_checksum: "sha256:abc123".to_owned(),
        }
    }

    #[test]
    fn published_runners_are_signed_and_verify() {
        let signed = sign_manifest(manifest(), &key()).expect("signed manifest");

        assert_eq!(signed.manifest.package_ref(), "crm.create_customer@1.2.0");
        assert!(signed.verify(&key()).is_ok());
    }

    #[test]
    fn tampered_runner_packages_are_refused() {
        let mut signed = sign_manifest(manifest(), &key()).expect("signed manifest");
        signed.manifest.package_checksum = "sha256:tampered".to_owned();

        assert_eq!(signed.verify(&key()), Err(RegistryError::TamperedPackage));
    }

    #[test]
    fn registry_promotes_and_resolves_versions() {
        let signed = sign_manifest(manifest(), &key()).expect("signed manifest");
        let promoted = RunnerRegistry::promote(signed, RegistryStage::Prod, &key())
            .expect("promoted manifest");
        let mut registry = RunnerRegistry::default();
        registry
            .publish(promoted, &key())
            .expect("published manifest");

        let resolved = registry
            .resolve(
                "crm.create_customer",
                "tenant_a",
                VersionSelector::Channel("stable".to_owned()),
                RegistryStage::Prod,
            )
            .expect("resolved stable");
        assert_eq!(resolved.manifest.version, "1.2.0");
    }

    #[test]
    fn reviewable_manifest_is_git_friendly() {
        let signed = sign_manifest(manifest(), &key()).expect("signed manifest");
        let rendered = signed.render_reviewable();

        assert!(rendered.contains("runner_id: crm.create_customer"));
        assert!(rendered.contains("required_adapters:\n  - greentic.desktop.playwright"));
        assert!(rendered.contains("signature:"));
    }
}
