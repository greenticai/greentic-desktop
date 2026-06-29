use greentic_desktop_extension::ExtensionManifest;
use greentic_desktop_registry::{RegistryStage, SignedRunnerManifest, SigningKey};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentMode {
    Connected,
    Airgapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePackageType {
    Runtime,
    Extension,
    RunnerPackage,
    Policy,
    RevocationList,
}

impl UpdatePackageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Extension => "extension",
            Self::RunnerPackage => "runner_package",
            Self::Policy => "policy",
            Self::RevocationList => "revocation_list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdatePayload {
    Runtime {
        version: String,
        min_current_version: String,
    },
    Extension(ExtensionManifest),
    RunnerPackage(SignedRunnerManifest),
    Policy {
        policy_id: String,
        version: String,
    },
    RevocationList {
        revoked_package_ids: Vec<String>,
    },
}

impl UpdatePayload {
    pub fn package_type(&self) -> UpdatePackageType {
        match self {
            Self::Runtime { .. } => UpdatePackageType::Runtime,
            Self::Extension(_) => UpdatePackageType::Extension,
            Self::RunnerPackage(_) => UpdatePackageType::RunnerPackage,
            Self::Policy { .. } => UpdatePackageType::Policy,
            Self::RevocationList { .. } => UpdatePackageType::RevocationList,
        }
    }

    pub fn package_id(&self) -> String {
        match self {
            Self::Runtime { version, .. } => format!("runtime@{version}"),
            Self::Extension(manifest) => format!("{}@{}", manifest.id, manifest.version),
            Self::RunnerPackage(signed) => signed.manifest.package_ref(),
            Self::Policy { policy_id, version } => format!("{policy_id}@{version}"),
            Self::RevocationList {
                revoked_package_ids,
            } => {
                format!("revocations@{}", revoked_package_ids.len())
            }
        }
    }

    pub fn compatibility(&self) -> Vec<String> {
        match self {
            Self::Runtime { version, .. } => vec![format!("greentic-desktop<={version}")],
            Self::Extension(manifest) => {
                vec![format!(
                    "extension-runtime={}",
                    runtime_name(&manifest.runtime)
                )]
            }
            Self::RunnerPackage(signed) => signed.manifest.compatibility.clone(),
            Self::Policy { .. } | Self::RevocationList { .. } => Vec::new(),
        }
    }

    pub fn dependencies(&self) -> Vec<DeploymentDependency> {
        match self {
            Self::RunnerPackage(signed) => signed
                .manifest
                .required_adapters
                .iter()
                .map(|adapter| DeploymentDependency {
                    id: adapter.clone(),
                    package_type: UpdatePackageType::Extension,
                })
                .collect(),
            Self::Extension(manifest) => manifest
                .permissions
                .iter()
                .map(|permission| DeploymentDependency {
                    id: permission.clone(),
                    package_type: UpdatePackageType::Policy,
                })
                .collect(),
            Self::Runtime { .. } | Self::Policy { .. } | Self::RevocationList { .. } => Vec::new(),
        }
    }

    fn canonical(&self) -> String {
        match self {
            Self::Runtime {
                version,
                min_current_version,
            } => format!("runtime:{version}:{min_current_version}"),
            Self::Extension(manifest) => manifest.render_toml(),
            Self::RunnerPackage(signed) => signed.render_reviewable(),
            Self::Policy { policy_id, version } => format!("policy:{policy_id}:{version}"),
            Self::RevocationList {
                revoked_package_ids,
            } => format!("revocations:{}", revoked_package_ids.join(",")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentDependency {
    pub id: String,
    pub package_type: UpdatePackageType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateManifest {
    pub package_id: String,
    pub package_type: UpdatePackageType,
    pub checksum: String,
    pub tenant_id: Option<String>,
    pub compatibility: Vec<String>,
    pub dependencies: Vec<DeploymentDependency>,
    pub source_mode: DeploymentMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedUpdatePackage {
    pub manifest: UpdateManifest,
    pub payload: UpdatePayload,
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RevocationList {
    revoked: BTreeSet<String>,
}

impl RevocationList {
    pub fn new(ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            revoked: ids.into_iter().map(Into::into).collect(),
        }
    }

    pub fn is_revoked(&self, package_id: &str) -> bool {
        self.revoked.contains(package_id)
    }

    pub fn apply_package(&mut self, package: &SignedUpdatePackage) -> Result<(), DeploymentError> {
        if let UpdatePayload::RevocationList {
            revoked_package_ids,
        } = &package.payload
        {
            self.revoked.extend(revoked_package_ids.iter().cloned());
            Ok(())
        } else {
            Err(DeploymentError::WrongPackageType)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentTarget {
    pub tenant_id: String,
    pub current_runtime_version: String,
    pub installed_extensions: Vec<String>,
    pub installed_policies: Vec<String>,
    pub stage: RegistryStage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub mode: DeploymentMode,
    pub package_id: String,
    pub package_type: UpdatePackageType,
    pub actions: Vec<String>,
    pub rollback_actions: Vec<String>,
    pub audit_log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirgappedBundle {
    pub bundle_id: String,
    pub packages: Vec<SignedUpdatePackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRepository {
    packages: Vec<SignedUpdatePackage>,
}

impl LocalRepository {
    pub fn new(packages: Vec<SignedUpdatePackage>) -> Self {
        Self { packages }
    }

    pub fn find(&self, package_id: &str) -> Option<&SignedUpdatePackage> {
        self.packages
            .iter()
            .find(|package| package.manifest.package_id == package_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedRegistry {
    packages: Vec<SignedUpdatePackage>,
    mtls_enabled: bool,
}

impl ConnectedRegistry {
    pub fn new(packages: Vec<SignedUpdatePackage>, mtls_enabled: bool) -> Self {
        Self {
            packages,
            mtls_enabled,
        }
    }

    pub fn pull(&self, package_id: &str) -> Result<SignedUpdatePackage, DeploymentError> {
        if !self.mtls_enabled {
            return Err(DeploymentError::MutualTlsRequired);
        }
        self.packages
            .iter()
            .find(|package| package.manifest.package_id == package_id)
            .cloned()
            .ok_or_else(|| DeploymentError::NotFound(package_id.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentError {
    UnsignedPackage(String),
    SignatureMismatch(String),
    ChecksumMismatch(String),
    TenantMismatch { expected: String, actual: String },
    IncompatibleRuntime(String),
    MissingDependency(String),
    RevokedPackage(String),
    MutualTlsRequired,
    NotFound(String),
    WrongPackageType,
}

impl fmt::Display for DeploymentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsignedPackage(id) => write!(f, "unsigned package {id} rejected"),
            Self::SignatureMismatch(id) => write!(f, "signature mismatch for package {id}"),
            Self::ChecksumMismatch(id) => write!(f, "checksum mismatch for package {id}"),
            Self::TenantMismatch { expected, actual } => {
                write!(f, "tenant mismatch: expected {expected}, got {actual}")
            }
            Self::IncompatibleRuntime(requirement) => {
                write!(f, "runtime is incompatible with {requirement}")
            }
            Self::MissingDependency(id) => write!(f, "missing dependency {id}"),
            Self::RevokedPackage(id) => write!(f, "package {id} is revoked"),
            Self::MutualTlsRequired => write!(f, "connected pulls require mTLS"),
            Self::NotFound(id) => write!(f, "package {id} not found"),
            Self::WrongPackageType => write!(f, "wrong update package type"),
        }
    }
}

impl std::error::Error for DeploymentError {}

pub fn sign_update_package(
    payload: UpdatePayload,
    tenant_id: Option<String>,
    source_mode: DeploymentMode,
    key: &SigningKey,
) -> Result<SignedUpdatePackage, DeploymentError> {
    let manifest = UpdateManifest {
        package_id: payload.package_id(),
        package_type: payload.package_type(),
        checksum: checksum_for_payload(&payload),
        tenant_id,
        compatibility: payload.compatibility(),
        dependencies: payload.dependencies(),
        source_mode,
    };
    Ok(SignedUpdatePackage {
        signature: signature_for(&manifest, &payload, key),
        manifest,
        payload,
        key_id: key.key_id.clone(),
    })
}

pub fn install_from_connected_registry(
    registry: &ConnectedRegistry,
    package_id: &str,
    target: &DeploymentTarget,
    key: &SigningKey,
    revocations: &RevocationList,
) -> Result<InstallPlan, DeploymentError> {
    let package = registry.pull(package_id)?;
    verify_update_package(
        &package,
        DeploymentMode::Connected,
        target,
        key,
        revocations,
    )
}

pub fn import_airgapped_bundle(
    bundle: &AirgappedBundle,
    target: &DeploymentTarget,
    key: &SigningKey,
    revocations: &RevocationList,
) -> Result<Vec<InstallPlan>, DeploymentError> {
    bundle
        .packages
        .iter()
        .map(|package| {
            verify_update_package(package, DeploymentMode::Airgapped, target, key, revocations)
        })
        .collect()
}

pub fn install_from_local_bundle(
    repository: &LocalRepository,
    package_id: &str,
    target: &DeploymentTarget,
    key: &SigningKey,
    revocations: &RevocationList,
) -> Result<InstallPlan, DeploymentError> {
    let package = repository
        .find(package_id)
        .ok_or_else(|| DeploymentError::NotFound(package_id.to_owned()))?;
    verify_update_package(package, DeploymentMode::Airgapped, target, key, revocations)
}

pub fn verify_update_package(
    package: &SignedUpdatePackage,
    expected_mode: DeploymentMode,
    target: &DeploymentTarget,
    key: &SigningKey,
    revocations: &RevocationList,
) -> Result<InstallPlan, DeploymentError> {
    if package.signature.trim().is_empty() {
        return Err(DeploymentError::UnsignedPackage(
            package.manifest.package_id.clone(),
        ));
    }
    if package.key_id != key.key_id
        || package.signature != signature_for(&package.manifest, &package.payload, key)
    {
        return Err(DeploymentError::SignatureMismatch(
            package.manifest.package_id.clone(),
        ));
    }
    if package.manifest.checksum != checksum_for_payload(&package.payload) {
        return Err(DeploymentError::ChecksumMismatch(
            package.manifest.package_id.clone(),
        ));
    }
    if package.manifest.source_mode != expected_mode {
        return Err(DeploymentError::NotFound(format!(
            "{} for {:?}",
            package.manifest.package_id, expected_mode
        )));
    }
    if revocations.is_revoked(&package.manifest.package_id) {
        return Err(DeploymentError::RevokedPackage(
            package.manifest.package_id.clone(),
        ));
    }
    if let Some(actual) = &package.manifest.tenant_id {
        if actual != &target.tenant_id {
            return Err(DeploymentError::TenantMismatch {
                expected: target.tenant_id.clone(),
                actual: actual.clone(),
            });
        }
    }
    for requirement in &package.manifest.compatibility {
        if !runtime_satisfies(&target.current_runtime_version, requirement) {
            return Err(DeploymentError::IncompatibleRuntime(requirement.clone()));
        }
    }
    for dependency in &package.manifest.dependencies {
        if !dependency_satisfied(dependency, target) {
            return Err(DeploymentError::MissingDependency(dependency.id.clone()));
        }
    }

    Ok(install_plan_for(package, expected_mode, target))
}

fn install_plan_for(
    package: &SignedUpdatePackage,
    mode: DeploymentMode,
    target: &DeploymentTarget,
) -> InstallPlan {
    let verb = match package.manifest.package_type {
        UpdatePackageType::Runtime => "replace runtime",
        UpdatePackageType::Extension => "install extension",
        UpdatePackageType::RunnerPackage => "install runner package",
        UpdatePackageType::Policy => "apply policy",
        UpdatePackageType::RevocationList => "apply revocation list",
    };
    InstallPlan {
        mode,
        package_id: package.manifest.package_id.clone(),
        package_type: package.manifest.package_type,
        actions: vec![format!("{verb} {}", package.manifest.package_id)],
        rollback_actions: vec![format!(
            "restore previous {} on {}",
            package.manifest.package_type.as_str(),
            target.current_runtime_version
        )],
        audit_log: vec![
            format!("verified signature {}", package.key_id),
            format!("verified checksum {}", package.manifest.checksum),
            format!("tenant {}", target.tenant_id),
            format!("stage {}", target.stage.as_str()),
        ],
    }
}

fn dependency_satisfied(dependency: &DeploymentDependency, target: &DeploymentTarget) -> bool {
    match dependency.package_type {
        UpdatePackageType::Extension => target
            .installed_extensions
            .iter()
            .any(|extension| extension == &dependency.id),
        UpdatePackageType::Policy => target
            .installed_policies
            .iter()
            .any(|policy| policy == &dependency.id),
        UpdatePackageType::Runtime
        | UpdatePackageType::RunnerPackage
        | UpdatePackageType::RevocationList => true,
    }
}

fn runtime_satisfies(current: &str, requirement: &str) -> bool {
    if let Some(minimum) = requirement.strip_prefix("greentic-desktop>=") {
        return compare_semver(current, minimum) != std::cmp::Ordering::Less;
    }
    if let Some(maximum) = requirement.strip_prefix("greentic-desktop<=") {
        return compare_semver(current, maximum) != std::cmp::Ordering::Greater;
    }
    true
}

fn compare_semver(left: &str, right: &str) -> std::cmp::Ordering {
    semver_parts(left).cmp(&semver_parts(right))
}

fn semver_parts(value: &str) -> (u64, u64, u64) {
    let mut parts = value
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn checksum_for_payload(payload: &UpdatePayload) -> String {
    format!("sha256:{}", deterministic_hash(&payload.canonical()))
}

fn signature_for(manifest: &UpdateManifest, payload: &UpdatePayload, key: &SigningKey) -> String {
    deterministic_hash(&format!(
        "type:{}\nid:{}\nchecksum:{}\ntenant:{}\nmode:{:?}\npayload:{}\nkey:{}\nmaterial:{}",
        manifest.package_type.as_str(),
        manifest.package_id,
        manifest.checksum,
        manifest.tenant_id.clone().unwrap_or_default(),
        manifest.source_mode,
        payload.canonical(),
        key.key_id,
        key_material_fingerprint(key)
    ))
}

fn key_material_fingerprint(key: &SigningKey) -> String {
    deterministic_hash(&format!("{:?}", key))
}

fn deterministic_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

fn runtime_name(runtime: &greentic_desktop_extension::ExtensionRuntime) -> &'static str {
    match runtime {
        greentic_desktop_extension::ExtensionRuntime::Native => "native",
        greentic_desktop_extension::ExtensionRuntime::Sidecar => "sidecar",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_extension::{ExtensionManifest, ExtensionRuntime};
    use greentic_desktop_registry::{sign_manifest, RunnerLifecycle, RunnerManifest, TenantScope};

    fn key() -> SigningKey {
        SigningKey::new("deployment-root", "material")
    }

    fn target() -> DeploymentTarget {
        DeploymentTarget {
            tenant_id: "tenant_a".to_owned(),
            current_runtime_version: "0.1.0".to_owned(),
            installed_extensions: vec!["greentic.desktop.playwright".to_owned()],
            installed_policies: vec!["network.localhost".to_owned()],
            stage: RegistryStage::Prod,
        }
    }

    fn extension() -> ExtensionManifest {
        ExtensionManifest {
            id: "greentic.desktop.playwright".to_owned(),
            name: "Playwright Web Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Sidecar,
            command: Some("node".to_owned()),
            args: vec!["index.js".to_owned()],
            capabilities: vec!["web.fill".to_owned()],
            permissions: vec!["network.localhost".to_owned()],
            signed: true,
        }
    }

    fn runner_payload() -> UpdatePayload {
        let registry_key = key();
        let signed = sign_manifest(
            RunnerManifest {
                runner_id: "crm.create_customer".to_owned(),
                version: "1.2.0".to_owned(),
                lifecycle: RunnerLifecycle::Published,
                stage: RegistryStage::Prod,
                scope: TenantScope {
                    tenant_id: "tenant_a".to_owned(),
                    team_id: "sales".to_owned(),
                    private: true,
                },
                required_adapters: vec!["greentic.desktop.playwright".to_owned()],
                compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
                package_checksum:
                    "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                        .to_owned(),
            },
            &registry_key,
        )
        .expect("runner manifest signed");
        UpdatePayload::RunnerPackage(signed)
    }

    fn signed_runner(mode: DeploymentMode) -> SignedUpdatePackage {
        sign_update_package(runner_payload(), Some("tenant_a".to_owned()), mode, &key())
            .expect("signed update package")
    }

    #[test]
    fn runtime_can_install_from_online_registry() {
        let package = signed_runner(DeploymentMode::Connected);
        let registry = ConnectedRegistry::new(vec![package], true);

        let plan = install_from_connected_registry(
            &registry,
            "crm.create_customer@1.2.0",
            &target(),
            &key(),
            &RevocationList::default(),
        )
        .expect("connected install plan");

        assert_eq!(plan.mode, DeploymentMode::Connected);
        assert_eq!(
            plan.actions,
            vec!["install runner package crm.create_customer@1.2.0"]
        );
        assert!(plan
            .audit_log
            .iter()
            .any(|entry| entry.contains("verified signature")));
    }

    #[test]
    fn connected_registry_requires_mtls() {
        let registry =
            ConnectedRegistry::new(vec![signed_runner(DeploymentMode::Connected)], false);

        assert_eq!(
            install_from_connected_registry(
                &registry,
                "crm.create_customer@1.2.0",
                &target(),
                &key(),
                &RevocationList::default(),
            )
            .expect_err("mTLS is required"),
            DeploymentError::MutualTlsRequired
        );
    }

    #[test]
    fn runtime_can_install_from_local_airgapped_bundle() {
        let package = signed_runner(DeploymentMode::Airgapped);
        let repository = LocalRepository::new(vec![package]);

        let plan = install_from_local_bundle(
            &repository,
            "crm.create_customer@1.2.0",
            &target(),
            &key(),
            &RevocationList::default(),
        )
        .expect("local install plan");

        assert_eq!(plan.mode, DeploymentMode::Airgapped);
        assert!(plan
            .rollback_actions
            .contains(&"restore previous runner_package on 0.1.0".to_owned()));
    }

    #[test]
    fn airgapped_bundle_imports_and_verifies_all_packages() {
        let extension_package = sign_update_package(
            UpdatePayload::Extension(extension()),
            Some("tenant_a".to_owned()),
            DeploymentMode::Airgapped,
            &key(),
        )
        .expect("signed extension");
        let runner_package = signed_runner(DeploymentMode::Airgapped);
        let bundle = AirgappedBundle {
            bundle_id: "tenant-a-weekly".to_owned(),
            packages: vec![extension_package, runner_package],
        };

        let plans = import_airgapped_bundle(&bundle, &target(), &key(), &RevocationList::default())
            .expect("bundle import");

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].package_type, UpdatePackageType::Extension);
        assert_eq!(plans[1].package_type, UpdatePackageType::RunnerPackage);
    }

    #[test]
    fn unsigned_packages_are_rejected() {
        let mut package = signed_runner(DeploymentMode::Airgapped);
        package.signature.clear();

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &target(),
                &key(),
                &RevocationList::default(),
            )
            .expect_err("unsigned package rejected"),
            DeploymentError::UnsignedPackage("crm.create_customer@1.2.0".to_owned())
        );
    }

    #[test]
    fn tampered_checksums_are_rejected() {
        let mut package = signed_runner(DeploymentMode::Airgapped);
        package.manifest.checksum =
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned();
        package.signature = signature_for(&package.manifest, &package.payload, &key());

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &target(),
                &key(),
                &RevocationList::default(),
            )
            .expect_err("checksum mismatch"),
            DeploymentError::ChecksumMismatch("crm.create_customer@1.2.0".to_owned())
        );
    }

    #[test]
    fn tenant_scope_is_checked_for_imports() {
        let package = sign_update_package(
            runner_payload(),
            Some("tenant_b".to_owned()),
            DeploymentMode::Airgapped,
            &key(),
        )
        .expect("signed package");

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &target(),
                &key(),
                &RevocationList::default(),
            )
            .expect_err("tenant mismatch"),
            DeploymentError::TenantMismatch {
                expected: "tenant_a".to_owned(),
                actual: "tenant_b".to_owned()
            }
        );
    }

    #[test]
    fn revoked_packages_are_rejected() {
        let package = signed_runner(DeploymentMode::Airgapped);
        let revocations = RevocationList::new(["crm.create_customer@1.2.0"]);

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &target(),
                &key(),
                &revocations,
            )
            .expect_err("revoked"),
            DeploymentError::RevokedPackage("crm.create_customer@1.2.0".to_owned())
        );
    }

    #[test]
    fn runner_dependencies_are_checked() {
        let package = signed_runner(DeploymentMode::Airgapped);
        let mut missing_extension = target();
        missing_extension.installed_extensions.clear();

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &missing_extension,
                &key(),
                &RevocationList::default(),
            )
            .expect_err("dependency missing"),
            DeploymentError::MissingDependency("greentic.desktop.playwright".to_owned())
        );
    }

    #[test]
    fn extension_policy_dependencies_are_checked() {
        let package = sign_update_package(
            UpdatePayload::Extension(extension()),
            Some("tenant_a".to_owned()),
            DeploymentMode::Airgapped,
            &key(),
        )
        .expect("signed extension");
        let mut missing_policy = target();
        missing_policy.installed_policies.clear();

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &missing_policy,
                &key(),
                &RevocationList::default(),
            )
            .expect_err("policy dependency missing"),
            DeploymentError::MissingDependency("network.localhost".to_owned())
        );
    }

    #[test]
    fn incompatible_runtime_is_rejected() {
        let package = signed_runner(DeploymentMode::Airgapped);
        let mut old_runtime = target();
        old_runtime.current_runtime_version = "0.0.9".to_owned();

        assert_eq!(
            verify_update_package(
                &package,
                DeploymentMode::Airgapped,
                &old_runtime,
                &key(),
                &RevocationList::default(),
            )
            .expect_err("runtime incompatible"),
            DeploymentError::IncompatibleRuntime("greentic-desktop>=0.1.0".to_owned())
        );
    }
}
