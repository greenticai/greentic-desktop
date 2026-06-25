use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStatus {
    Success,
    Failed,
}

impl EvidenceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceArtifactKind {
    Screenshot,
    AnnotatedScreenshot,
    DomSnapshot,
    WindowTreeSnapshot,
    TerminalScreenBuffer,
    ToolTrace,
    Log,
    ErrorDialog,
    OutputExtractionProof,
}

impl EvidenceArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::AnnotatedScreenshot => "annotated_screenshot",
            Self::DomSnapshot => "dom_snapshot",
            Self::WindowTreeSnapshot => "window_tree_snapshot",
            Self::TerminalScreenBuffer => "terminal_screen_buffer",
            Self::ToolTrace => "tool_trace",
            Self::Log => "log",
            Self::ErrorDialog => "error_dialog",
            Self::OutputExtractionProof => "output_extraction_proof",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceArtifact {
    pub kind: EvidenceArtifactKind,
    pub name: String,
    pub uri: String,
    pub redacted: bool,
}

impl EvidenceArtifact {
    pub fn new(
        kind: EvidenceArtifactKind,
        name: impl Into<String>,
        uri: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            uri: uri.into(),
            redacted: false,
        }
    }

    pub fn redacted(mut self) -> Self {
        self.redacted = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolTraceEntry {
    pub step_id: String,
    pub capability: String,
    pub status: EvidenceStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub run_id: String,
    pub runner_id: String,
    pub runner_version: String,
    pub status: EvidenceStatus,
    pub inputs_hash: String,
    pub outputs: BTreeMap<String, String>,
    pub artifacts: Vec<EvidenceArtifact>,
    pub tool_trace: Vec<ToolTraceEntry>,
    pub started_at: String,
    pub completed_at: String,
}

impl EvidenceBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: impl Into<String>,
        runner_id: impl Into<String>,
        runner_version: impl Into<String>,
        status: EvidenceStatus,
        inputs: &BTreeMap<String, String>,
        secret_keys: &[String],
        outputs: BTreeMap<String, String>,
        artifacts: Vec<EvidenceArtifact>,
        tool_trace: Vec<ToolTraceEntry>,
        started_at: impl Into<String>,
        completed_at: impl Into<String>,
    ) -> Self {
        let redacted_inputs = redact_inputs(inputs, secret_keys);
        Self {
            run_id: run_id.into(),
            runner_id: runner_id.into(),
            runner_version: runner_version.into(),
            status,
            inputs_hash: deterministic_hash(&render_map(&redacted_inputs)),
            outputs,
            artifacts,
            tool_trace,
            started_at: started_at.into(),
            completed_at: completed_at.into(),
        }
    }

    pub fn reference(&self) -> EvidenceRef {
        EvidenceRef {
            run_id: self.run_id.clone(),
            uri: format!("evidence://{}/bundle.json", self.run_id),
        }
    }

    pub fn to_json(&self) -> String {
        let screenshots = self
            .artifacts
            .iter()
            .filter(|artifact| matches!(artifact.kind, EvidenceArtifactKind::Screenshot))
            .map(|artifact| format!("\"{}\"", escape_json(&artifact.name)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"run_id\":\"{}\",\"runner_id\":\"{}\",\"runner_version\":\"{}\",\"status\":\"{}\",\"inputs_hash\":\"{}\",\"outputs\":{},\"screenshots\":[{}],\"tool_trace\":{},\"started_at\":\"{}\",\"completed_at\":\"{}\"}}",
            escape_json(&self.run_id),
            escape_json(&self.runner_id),
            escape_json(&self.runner_version),
            self.status.as_str(),
            self.inputs_hash,
            render_json_map(&self.outputs),
            screenshots,
            render_tool_trace(&self.tool_trace),
            escape_json(&self.started_at),
            escape_json(&self.completed_at),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub run_id: String,
    pub uri: String,
}

impl EvidenceRef {
    pub fn mcp_result_reference(&self) -> String {
        format!(
            "{{\"evidence_run_id\":\"{}\",\"evidence_uri\":\"{}\"}}",
            escape_json(&self.run_id),
            escape_json(&self.uri)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStoreError {
    AlreadyExists(String),
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryEvidenceStore {
    bundles: BTreeMap<String, EvidenceBundle>,
}

impl InMemoryEvidenceStore {
    pub fn insert(&mut self, bundle: EvidenceBundle) -> Result<EvidenceRef, EvidenceStoreError> {
        if self.bundles.contains_key(&bundle.run_id) {
            return Err(EvidenceStoreError::AlreadyExists(bundle.run_id));
        }
        let reference = bundle.reference();
        self.bundles.insert(bundle.run_id.clone(), bundle);
        Ok(reference)
    }

    pub fn get(&self, run_id: &str) -> Option<&EvidenceBundle> {
        self.bundles.get(run_id)
    }
}

pub fn redact_inputs(
    inputs: &BTreeMap<String, String>,
    secret_keys: &[String],
) -> BTreeMap<String, String> {
    inputs
        .iter()
        .map(|(key, value)| {
            let sensitive = secret_keys.iter().any(|secret| secret == key)
                || key.contains("password")
                || key.contains("secret")
                || key.contains("token");
            (
                key.clone(),
                if sensitive {
                    "[REDACTED]".to_owned()
                } else {
                    value.clone()
                },
            )
        })
        .collect()
}

fn deterministic_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

fn render_map(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_json_map(values: &BTreeMap<String, String>) -> String {
    let body = values
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", escape_json(key), escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn render_tool_trace(trace: &[ToolTraceEntry]) -> String {
    let body = trace
        .iter()
        .map(|entry| {
            let message = entry
                .message
                .as_deref()
                .map(|message| format!("\"{}\"", escape_json(message)))
                .unwrap_or_else(|| "null".to_owned());
            format!(
                "{{\"step_id\":\"{}\",\"capability\":\"{}\",\"status\":\"{}\",\"message\":{message}}}",
                escape_json(&entry.step_id),
                escape_json(&entry.capability),
                entry.status.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> EvidenceBundle {
        EvidenceBundle::new(
            "run_123",
            "crm.create_customer",
            "1.2.0",
            EvidenceStatus::Success,
            &BTreeMap::from([
                ("email".to_owned(), "user@example.test".to_owned()),
                ("password".to_owned(), "super-secret".to_owned()),
            ]),
            &["password".to_owned()],
            BTreeMap::from([("customer_id".to_owned(), "CUST-49281".to_owned())]),
            vec![EvidenceArtifact::new(
                EvidenceArtifactKind::Screenshot,
                "after_success.png",
                "object://evidence/run_123/after_success.png",
            )],
            vec![ToolTraceEntry {
                step_id: "submit".to_owned(),
                capability: "web.click".to_owned(),
                status: EvidenceStatus::Success,
                message: None,
            }],
            "2026-06-25T10:00:00Z",
            "2026-06-25T10:00:02Z",
        )
    }

    #[test]
    fn bundle_references_can_be_returned_from_mcp_results() {
        let reference = bundle().reference();

        assert_eq!(reference.uri, "evidence://run_123/bundle.json");
        assert!(reference.mcp_result_reference().contains("evidence_uri"));
    }

    #[test]
    fn sensitive_inputs_are_redacted_before_hashing() {
        let plain = BTreeMap::from([("password".to_owned(), "first".to_owned())]);
        let changed = BTreeMap::from([("password".to_owned(), "second".to_owned())]);

        let left = EvidenceBundle::new(
            "run_a",
            "runner",
            "1",
            EvidenceStatus::Success,
            &plain,
            &["password".to_owned()],
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            "start",
            "end",
        );
        let right = EvidenceBundle::new(
            "run_b",
            "runner",
            "1",
            EvidenceStatus::Success,
            &changed,
            &["password".to_owned()],
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            "start",
            "end",
        );

        assert_eq!(left.inputs_hash, right.inputs_hash);
    }

    #[test]
    fn evidence_store_is_immutable_by_run_id() {
        let mut store = InMemoryEvidenceStore::default();
        let first = bundle();
        let second = bundle();

        assert!(store.insert(first).is_ok());
        assert_eq!(
            store.insert(second),
            Err(EvidenceStoreError::AlreadyExists("run_123".to_owned()))
        );
    }

    #[test]
    fn bundle_json_matches_audit_shape() {
        let json = bundle().to_json();

        assert!(json.contains("\"run_id\":\"run_123\""));
        assert!(json.contains("\"screenshots\":[\"after_success.png\"]"));
        assert!(json.contains("\"customer_id\":\"CUST-49281\""));
    }
}
