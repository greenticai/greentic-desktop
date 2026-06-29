use greentic_desktop_core::RiskLevel;
use greentic_desktop_registry::{RunnerLifecycle, SignedRunnerManifest};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPolicy {
    pub read_screen: bool,
    pub type_text: bool,
    pub submit_forms: bool,
    pub delete_records: bool,
    pub payments: bool,
    pub bulk_update: bool,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            read_screen: true,
            type_text: true,
            submit_forms: true,
            delete_records: false,
            payments: false,
            bulk_update: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalPolicy {
    pub production_required: bool,
    pub high_risk_approvals: u8,
    pub critical_risk_approvals: u8,
    pub bulk_required: bool,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            production_required: true,
            high_risk_approvals: 1,
            critical_risk_approvals: 2,
            bulk_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentPolicy {
    allowed: BTreeSet<String>,
}

impl EnvironmentPolicy {
    pub fn new(allowed: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed: allowed.into_iter().map(Into::into).collect(),
        }
    }

    pub fn allows(&self, environment: &str) -> bool {
        self.allowed.contains(environment)
    }
}

impl Default for EnvironmentPolicy {
    fn default() -> Self {
        Self::new(["dev", "staging", "production"])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActionRequest {
    pub read_screen: bool,
    pub type_text: bool,
    pub submit_forms: bool,
    pub delete_records: bool,
    pub payments: bool,
    pub bulk_update: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityPolicy {
    pub risk_level: RiskLevel,
    pub permissions: PermissionPolicy,
    pub approval: ApprovalPolicy,
    pub environments: EnvironmentPolicy,
    pub require_signed_published_runners: bool,
}

impl SecurityPolicy {
    pub fn medium_default() -> Self {
        Self {
            risk_level: RiskLevel::Medium,
            permissions: PermissionPolicy::default(),
            approval: ApprovalPolicy::default(),
            environments: EnvironmentPolicy::default(),
            require_signed_published_runners: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyContext {
    pub environment: String,
    pub approvals: u8,
    pub actions: ActionRequest,
    pub signed_published_runner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allowed,
    Denied { code: String, message: String },
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

pub fn enforce_policy(policy: &SecurityPolicy, context: &PolicyContext) -> PolicyDecision {
    if !policy.environments.allows(&context.environment) {
        return denied("environment_denied", "environment is not allowed");
    }
    if policy.require_signed_published_runners && !context.signed_published_runner {
        return denied("signature_required", "published runner must be signed");
    }
    if !permission_allows(&policy.permissions, &context.actions) {
        return denied(
            "permission_denied",
            "requested desktop action is blocked by policy",
        );
    }
    if policy.approval.production_required
        && context.environment == "production"
        && policy.risk_level >= RiskLevel::Medium
        && context.approvals == 0
    {
        return denied("approval_required", "production runner requires approval");
    }
    if policy.risk_level == RiskLevel::High
        && context.approvals < policy.approval.high_risk_approvals
    {
        return denied("approval_required", "high-risk runner requires approval");
    }
    if policy.risk_level == RiskLevel::Critical
        && context.approvals < policy.approval.critical_risk_approvals
    {
        return denied(
            "multi_approval_required",
            "critical runner requires multiple approvals",
        );
    }
    if policy.approval.bulk_required && context.actions.bulk_update && context.approvals == 0 {
        return denied("approval_required", "bulk update requires approval");
    }
    PolicyDecision::Allowed
}

pub fn verify_published_runner_is_signed(signed: &SignedRunnerManifest) -> PolicyDecision {
    if signed.manifest.lifecycle == RunnerLifecycle::Published && signed.signature.trim().is_empty()
    {
        denied("signature_required", "published runner must be signed")
    } else {
        PolicyDecision::Allowed
    }
}

#[derive(Debug, Clone, Default)]
pub struct SecretsManager {
    values: BTreeMap<String, String>,
}

impl SecretsManager {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn resolve_reference(&self, reference: &str) -> Option<String> {
        let key = reference
            .strip_prefix("{{secrets.")
            .and_then(|value| value.strip_suffix("}}"))?;
        self.values.get(key).cloned()
    }
}

pub fn redact_sensitive_text(value: &str) -> String {
    redact_sensitive_text_with_values(value, &[])
}

pub fn redact_sensitive_text_with_values(value: &str, known_secret_values: &[String]) -> String {
    let redacted = redact_known_secret_values(value, known_secret_values);
    redact_secretish_tokens(&redact_bearer_segments(&redacted))
}

pub fn redact_known_secret_values(value: &str, known_secret_values: &[String]) -> String {
    let mut redacted = value.to_owned();
    for secret in known_secret_values {
        let secret = secret.trim();
        if secret.len() >= 4 {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
    }
    redacted
}

pub fn command_display(command: &str, args: &[String], known_secret_values: &[String]) -> String {
    redact_sensitive_text_with_values(
        &std::iter::once(command.to_owned())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" "),
        known_secret_values,
    )
}

fn redact_bearer_segments(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let remaining = &value[index..];
        if remaining
            .get(..7)
            .map(|prefix| prefix.eq_ignore_ascii_case("bearer "))
            .unwrap_or(false)
        {
            output.push_str("Bearer [REDACTED]");
            index += 7;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
        } else {
            let ch = remaining.chars().next().expect("non-empty remaining text");
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

fn redact_secretish_tokens(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut token = String::new();
    for ch in value.chars() {
        if ch.is_whitespace() {
            push_redacted_token(&mut output, &token);
            token.clear();
            output.push(ch);
        } else {
            token.push(ch);
        }
    }
    push_redacted_token(&mut output, &token);
    output
}

fn push_redacted_token(output: &mut String, token: &str) {
    if token.is_empty() {
        return;
    }
    let lower = token.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.starts_with("{{secrets.")
    {
        output.push_str("[REDACTED]");
    } else {
        output.push_str(token);
    }
}

fn permission_allows(policy: &PermissionPolicy, actions: &ActionRequest) -> bool {
    (!actions.read_screen || policy.read_screen)
        && (!actions.type_text || policy.type_text)
        && (!actions.submit_forms || policy.submit_forms)
        && (!actions.delete_records || policy.delete_records)
        && (!actions.payments || policy.payments)
        && (!actions.bulk_update || policy.bulk_update)
}

fn denied(code: &str, message: &str) -> PolicyDecision {
    PolicyDecision::Denied {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_registry::{
        sign_manifest, RegistryStage, RunnerManifest, SigningKey, TenantScope,
    };

    fn context(environment: &str) -> PolicyContext {
        PolicyContext {
            environment: environment.to_owned(),
            approvals: 0,
            actions: ActionRequest {
                read_screen: true,
                type_text: true,
                submit_forms: true,
                ..ActionRequest::default()
            },
            signed_published_runner: true,
        }
    }

    #[test]
    fn medium_risk_requires_approval_in_production() {
        let decision = enforce_policy(&SecurityPolicy::medium_default(), &context("production"));

        assert_eq!(
            decision,
            PolicyDecision::Denied {
                code: "approval_required".to_owned(),
                message: "production runner requires approval".to_owned()
            }
        );
    }

    #[test]
    fn dangerous_actions_can_be_blocked() {
        let mut context = context("dev");
        context.actions.delete_records = true;

        assert!(!enforce_policy(&SecurityPolicy::medium_default(), &context).is_allowed());
    }

    #[test]
    fn critical_risk_requires_multiple_approvals() {
        let mut policy = SecurityPolicy::medium_default();
        policy.risk_level = RiskLevel::Critical;
        let mut context = context("dev");
        context.approvals = 1;

        assert_eq!(
            enforce_policy(&policy, &context),
            PolicyDecision::Denied {
                code: "multi_approval_required".to_owned(),
                message: "critical runner requires multiple approvals".to_owned()
            }
        );
    }

    #[test]
    fn secrets_manager_resolves_only_secret_references() {
        let mut manager = SecretsManager::default();
        manager.insert("crm_password", "swordfish");

        assert_eq!(
            manager.resolve_reference("{{secrets.crm_password}}"),
            Some("swordfish".to_owned())
        );
        assert_eq!(manager.resolve_reference("swordfish"), None);
    }

    #[test]
    fn sensitive_text_is_redacted_for_logs_and_ltm() {
        assert_eq!(
            redact_sensitive_text("username bob password=swordfish {{secrets.crm_token}}"),
            "username bob [REDACTED] [REDACTED]"
        );
    }

    #[test]
    fn known_secret_values_and_bearer_headers_are_redacted() {
        let raw = "request failed Authorization: Bearer sk-live-abc123 body=topsecret";

        let redacted = redact_sensitive_text_with_values(raw, &["topsecret".to_owned()]);

        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(!redacted.contains("sk-live-abc123"));
        assert!(!redacted.contains("topsecret"));
    }

    #[test]
    fn command_display_redacts_secret_arguments() {
        let rendered = command_display(
            "curl",
            &[
                "-H".to_owned(),
                "Authorization: Bearer sk-test-1234".to_owned(),
                "--data".to_owned(),
                "api_key=cleartext".to_owned(),
            ],
            &["cleartext".to_owned()],
        );

        assert!(!rendered.contains("sk-test-1234"));
        assert!(!rendered.contains("cleartext"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn published_runner_signature_policy_is_explicit() {
        let key = SigningKey::new("local-dev", "material");
        let signed = sign_manifest(
            RunnerManifest {
                runner_id: "crm.create_customer".to_owned(),
                version: "1.2.0".to_owned(),
                lifecycle: RunnerLifecycle::Published,
                stage: RegistryStage::Prod,
                scope: TenantScope {
                    tenant_id: "tenant_a".to_owned(),
                    team_id: "sales_ops".to_owned(),
                    private: true,
                },
                required_adapters: vec!["greentic.desktop.playwright".to_owned()],
                compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
                package_checksum:
                    "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                        .to_owned(),
            },
            &key,
        )
        .expect("signed manifest");

        assert!(verify_published_runner_is_signed(&signed).is_allowed());
    }
}
