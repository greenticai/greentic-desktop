export type ApiResponse<T> =
  | {
      ok: true;
      data: T;
    }
  | {
      ok: false;
      error: ApiError;
    };

export interface ApiError {
  code: string;
  message: string;
  details: Record<string, unknown>;
}

export interface RuntimeInfoDto {
  appVersion: string;
  platform: string;
  runtimeHome: string;
  evidenceStore: string;
  guiUrl: string;
  config: {
    mcpBind: string;
  };
  installedCoreAdapterIds: string[];
}

export interface SetupChecklistDto {
  items: SetupChecklistItemDto[];
}

export interface SetupChecklistItemDto {
  id: string;
  label: string;
  ok: boolean;
  status: "ready" | "warning" | "missing" | "unsupported" | string;
  help: string;
  action?: string;
}

export interface SetupFixResultDto {
  id: string;
  status: "opened" | "created" | "noop" | "manual" | "unsupported" | string;
  message: string;
}

export interface AdapterHealthDto {
  id: string;
  readiness: string;
  healthy: boolean;
  status?: string;
  statusLabel?: string;
  message: string;
  executableCapabilities: string[];
  recordableTargets: string[];
  logPath?: string | null;
}

export interface AdapterHealthResponseDto {
  adapters: AdapterHealthDto[];
}

export interface ActivityEventDto {
  id: string;
  kind: string;
  message: string;
  timestamp: string;
  relatedId?: string;
  target?: string;
}

export interface ActivityDto {
  events: ActivityEventDto[];
}

export interface ExtensionDto {
  id: string;
  name: string;
  status?: string;
  category?: string;
  description?: string;
  installed?: boolean;
  available?: boolean;
  enabled?: boolean;
  health?: string;
  version?: string;
  source?: string;
  publisher?: string;
  trust?: string;
  digest?: string | null;
  platformCompatible?: boolean;
  capabilities: string[];
  permissions: string[];
  permissionPrompts?: string[];
}

export interface ExtensionsDto {
  extensions: ExtensionDto[];
}

export interface RunnerSummaryDto {
  id: string;
  name: string;
  description?: string;
  status: string;
  risk: string;
  version: string;
  lastTest: string;
  updated?: string;
  adapters?: string[];
  inputs?: string[];
  outputs?: string[];
  secrets?: string[];
  inputFields?: RunnerFieldDto[];
  secretFields?: RunnerFieldDto[];
  outputFields?: RunnerOutputFieldDto[];
  published?: boolean;
  evidenceRefs?: string[];
  available?: boolean;
  availabilityMessage?: string | null;
}

export interface RunnersDto {
  runners: RunnerSummaryDto[];
}

export interface RunnerDetailDto {
  runner: RunnerSummaryDto | null;
  yamlPreview: string;
}

export interface RunnerEditModelDto {
  runnerId: string;
  name: string;
  description: string;
  risk: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
  secrets: string[];
  inputFields?: RunnerFieldDto[];
  secretFields?: RunnerFieldDto[];
  outputFields?: RunnerOutputFieldDto[];
  steps: Array<{ id: string; summary: string; editable?: boolean }>;
  assertions: string[];
  yamlPreview: string;
}

export interface RunnerFieldDto {
  name: string;
  valueType?: string | { Enum?: string[] };
  required?: boolean;
  defaultValue?: string | null;
  enumValues?: string[];
  validation?: string | null;
  secret?: boolean;
  hasValue?: boolean | null;
}

export interface RunnerOutputFieldDto extends RunnerFieldDto {
  extractor?: unknown;
  failureBehavior?: unknown;
  proof?: string;
}

export interface RunnerEditDraftDto {
  draftId: string;
  sourceRunnerId: string;
  sourceChecksum: string;
  instruction: string;
  mode: string;
  status: string;
  sourceRunner: RunnerEditModelDto;
  proposedRunner: RunnerEditModelDto;
  patch?: RunnerPatchPlanDto;
  openQuestions: string[];
  warnings: string[];
  changeSummary: string[];
  yamlPreview: string;
}

export interface RunnerPatchPlanDto {
  intentSummary: string;
  preserveBehavior: boolean;
  operations: RunnerPatchOperationDto[];
  requiredAdapters: string[];
  inputChanges: string[];
  outputChanges: string[];
  secretChanges: string[];
  stepChanges: string[];
  assertionChanges: string[];
  extractorChanges: string[];
  policyImpact: string;
  openQuestions: string[];
  warnings: string[];
  changeSummary: string[];
}

export interface RunnerPatchOperationDto {
  operation: string;
  target: string;
  after?: string;
  rationale: string;
  safety: string;
  requiresTest: boolean;
}

export interface RunnerEditApplyResultDto {
  runnerId: string;
  status: string;
  previousVersion: string;
  currentVersion: string;
  mcpTool: string;
  evidenceRef: string;
}

export interface RunnerVersionsDto {
  runnerId: string;
  versions: string[];
}

export interface RunnerActionResultDto {
  runnerId: string;
  action: string;
  status: string;
  evidenceRef: string;
  outputs: Record<string, string>;
  steps: Array<{ summary: string; status: string }>;
}

export interface EvidenceArtifactDto {
  id: string;
  kind: string;
  name: string;
  url: string;
  redacted: boolean;
}

export interface EvidenceBundleDto {
  bundleId: string;
  runId: string;
  runnerId: string;
  status: string;
  startedAt: string;
  completedAt: string;
  inputsHash: string;
  outputs: Record<string, string>;
  failureReason: string | null;
  artifacts: EvidenceArtifactDto[];
  steps: Array<{ summary: string; status: string }>;
}

export interface EvidenceBundlesDto {
  bundles: EvidenceBundleDto[];
}

export interface ApprovalDto {
  id: string;
  action: string;
  runnerId: string;
  risk: string;
  requestedBy: string;
  evidenceRef: string;
  policyReason: string;
  status: string;
}

export interface ApprovalsDto {
  approvals: ApprovalDto[];
}

export interface RefinementResultDto {
  refinementId: string;
  runnerId: string;
  status: string;
  applied: boolean;
  evidenceRef: string;
  diff: {
    stepId: string;
    before: string;
    after: string;
  };
}

export interface RecordingSummaryDto {
  sessionId: string;
  name: string;
  state: string;
  elapsedSeconds: number;
  profile: string;
  adapter: string;
  activeApp: string | null;
  captureState: string;
  captureBackend: string | null;
  captureHeartbeatAt: string | null;
  captureBlockedReasons: string[];
  rawEvents: number;
  observations: number;
  screenshots: number;
  lastEventSummary: string | null;
  markers: number;
  draftRunnerPath: string;
  normalizedStepSummaries: string[];
  evidenceRefs: string[];
}

export interface RecordingsDto {
  recordings: RecordingSummaryDto[];
}

export interface RecordingTargetDto {
  id: string;
  label: string;
  profile: string;
  adapter: string;
  available: boolean;
  status?: string;
  statusLabel?: string;
}

export interface RecordingTargetsDto {
  targets: RecordingTargetDto[];
}

export interface RecordingNormaliseResultDto {
  sessionId: string;
  runnerId: string;
  steps: string[];
  inputs: string[];
  outputs: string[];
  yamlPreview: string;
  warnings: string[];
}

export interface RecordingTestResultDto {
  sessionId: string;
  status: string;
  evidenceRef: string;
  outputs: Record<string, string>;
}

export interface RecordingFinaliseResultDto {
  sessionId: string;
  runnerId: string;
  path: string;
  saved: boolean;
}

export interface McpStatusDto {
  status: string;
  bind: string;
  tools: number;
}

export interface McpToolDto {
  id: string;
  name: string;
  runner: string;
  status: string;
  description: string;
  version?: string;
  lastCall: string;
  successRate?: number;
  risk?: string;
  inputSchema?: Record<string, unknown>;
  outputSchema?: Record<string, unknown>;
}

export interface McpToolsDto {
  tools: McpToolDto[];
}

export interface McpToolActionResultDto {
  toolId: string;
  toolName: string;
  action: string;
  status: string;
  evidenceRef: string;
  outputs: Record<string, string>;
}

export interface McpClientConfigDto {
  localUrl: string;
  clientJson: string;
  awsWorkSpacesDoc: string;
  awsForwardedConfigured: boolean;
}

export interface ExtensionInstallProgressDto {
  id?: string;
  status: string;
  phase?: string;
  version?: string;
  source?: string;
  digest?: string;
  publisher?: string;
  permissions?: string[];
  permissionPrompts?: string[];
  capabilities?: string[];
  phases?: Array<{ phase: string; status: string; message: string }>;
  needs_restart?: boolean;
  health?: string;
  message?: string;
}

export interface LlmSettingsDto {
  provider: string;
  model: string;
  endpoint: string | null;
  secretRef: string | null;
  mode: "heuristic" | "remote" | string;
  hasApiKey?: boolean;
  apiKey?: string;
  providers: LlmProviderDto[];
}

export interface LlmProviderDto {
  id: string;
  name: string;
  label?: string;
  defaultModel: string;
  endpoint: string | null;
  mode: "heuristic" | "remote" | string;
  secretName: string | null;
  requiresApiKey?: boolean;
  hasApiKey?: boolean;
}

export interface LlmTestResultDto {
  status: string;
  message: string;
}

export interface PlannerDraftDto {
  draftId: string;
  runnerId: string;
  name: string;
  description: string;
  risk: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
  secrets: string[];
  steps: PlannerDraftStepDto[];
  assertions: string[];
  openQuestions: string[];
  yamlPreview: string;
  policyWarnings: string[];
}

export interface PlannerDraftStepDto {
  id: string;
  summary: string;
  editable: boolean;
}

export interface PlannerTestResultDto {
  draftId: string;
  status: "passed" | "failed" | string;
  outputs: Record<string, string>;
  evidenceRef: string;
  steps: Array<{ summary: string; status: string }>;
}

export interface PlannerSaveResultDto {
  draftId: string;
  runnerId: string;
  path: string;
  saved: boolean;
}
