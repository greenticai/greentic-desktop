import type {
  ActivityDto,
  ApiResponse,
  ApprovalsDto,
  ApprovalDto,
  EvidenceBundleDto,
  EvidenceBundlesDto,
  ExtensionsDto,
  ExtensionInstallProgressDto,
  LlmSettingsDto,
  LlmTestResultDto,
  McpClientConfigDto,
  McpStatusDto,
  McpToolActionResultDto,
  McpToolsDto,
  PlannerDraftDto,
  PlannerSaveResultDto,
  PlannerTestResultDto,
  RecordingFinaliseResultDto,
  RecordingNormaliseResultDto,
  RecordingsDto,
  RecordingSummaryDto,
  RecordingTargetsDto,
  RecordingTestResultDto,
  RefinementResultDto,
  RunnerActionResultDto,
  RunnersDto,
  RuntimeInfoDto,
  SetupChecklistDto,
  SetupFixResultDto,
} from "./types";
import { toast } from "sonner";

const API_BASE = "/api/v1";
const GUI_TOKEN_STORAGE_KEY = "greentic.gui.token";

function guiToken(): string {
  if (typeof window === "undefined") {
    return "";
  }

  const token = new URLSearchParams(window.location.search).get("token");
  if (token) {
    window.sessionStorage.setItem(GUI_TOKEN_STORAGE_KEY, token);
    return token;
  }

  return window.sessionStorage.getItem(GUI_TOKEN_STORAGE_KEY) ?? "";
}

export class ApiClientError extends Error {
  readonly code: string;
  readonly details: Record<string, unknown>;

  constructor(code: string, message: string, details: Record<string, unknown> = {}) {
    super(message);
    this.name = "ApiClientError";
    this.code = code;
    this.details = details;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    const token = guiToken();
    response = await fetch(`${API_BASE}${path}`, {
      ...init,
      headers: {
        ...(init?.headers ?? {}),
        accept: "application/json",
        ...(token ? { "x-greentic-gui-token": token } : {}),
      },
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : "The local GUI API is unavailable.";
    toast.error(message);
    throw error;
  }

  const payload = (await response.json()) as ApiResponse<T>;
  if (!payload.ok) {
    const error = new ApiClientError(
      payload.error.code,
      payload.error.message,
      payload.error.details,
    );
    toast.error(error.message, { description: error.code });
    throw error;
  }

  return payload.data;
}

function jsonInit(method: "POST" | "PUT" | "PATCH", body?: unknown): RequestInit {
  return {
    method,
    headers: body == null ? undefined : { "content-type": "application/json" },
    body: body == null ? undefined : JSON.stringify(body),
  };
}

async function getWithDevFallback<T>(path: string, fallback: () => T): Promise<T> {
  try {
    return await request<T>(path);
  } catch (error) {
    if (import.meta.env.DEV && error instanceof TypeError) {
      return fallback();
    }
    throw error;
  }
}

export const api = {
  health: () => request<{ apiVersion: string; status: string }>("/health"),
  runtimeInfo: () =>
    getWithDevFallback<RuntimeInfoDto>("/runtime/info", () => ({
      appVersion: "dev",
      platform: navigator.platform || "browser",
      runtimeHome: "~/.greentic/desktop",
      evidenceStore: "~/.greentic/desktop/evidence",
      guiUrl: window.location.origin,
      config: { mcpBind: "127.0.0.1:8799" },
      installedCoreAdapterIds: ["greentic.desktop.core"],
    })),
  setupChecklist: () =>
    getWithDevFallback<SetupChecklistDto>("/setup/checklist", () => ({
      items: [
        {
          id: "runtime_home",
          label: "Runtime home exists",
          ok: true,
          status: "ready",
          help: "Runtime home is available.",
        },
        {
          id: "browser_automation",
          label: "Browser automation extension installed",
          ok: true,
          status: "ready",
          help: "Browser automation is installed.",
        },
        {
          id: "screen_capture_permission",
          label: "Screen capture permission",
          ok: false,
          status: "warning",
          help: "Your operating system may ask for screen capture permission.",
        },
        {
          id: "mcp_server",
          label: "MCP server configured",
          ok: true,
          status: "ready",
          help: "MCP bind address is configured.",
        },
      ],
    })),
  setupFix: (itemId: string, action?: string) =>
    request<SetupFixResultDto>("/setup/fix", jsonInit("POST", { id: itemId, action })),
  activity: () => request<ActivityDto>("/activity"),
  evidence: () => request<EvidenceBundlesDto>("/evidence"),
  evidenceBundle: (bundleId: string) =>
    request<EvidenceBundleDto>(`/evidence/${encodeURIComponent(bundleId)}`),
  approvals: () => request<ApprovalsDto>("/approvals"),
  approvalAction: (approvalId: string, action: "approve" | "reject") =>
    request<ApprovalDto>(
      `/approvals/${encodeURIComponent(approvalId)}/${action}`,
      jsonInit("POST"),
    ),
  recommendedExtensions: () => request<ExtensionsDto>("/extensions/recommended"),
  installedExtensions: () => request<ExtensionsDto>("/extensions/installed"),
  searchExtensions: (query: string) =>
    request<ExtensionsDto>(`/extensions/search?q=${encodeURIComponent(query)}`),
  installExtension: (
    source: string,
    version = "latest",
    approvals: {
      approveScreenCapture?: boolean;
      approveKeyboardMouse?: boolean;
      approveFilesystemWrite?: boolean;
    } = {},
  ) =>
    request<ExtensionInstallProgressDto>(
      "/extensions/install",
      jsonInit("POST", { source, version, ...approvals }),
    ),
  extensionAction: (id: string, action: string) =>
    request<ExtensionInstallProgressDto>(
      `/extensions/${encodeURIComponent(id)}/${action}`,
      jsonInit("POST"),
    ),
  runners: () => request<RunnersDto>("/runners"),
  runnerAction: (id: string, action: string, inputs?: Record<string, string>) =>
    request<RunnerActionResultDto>(
      `/runners/${encodeURIComponent(id)}/${action}`,
      jsonInit("POST", inputs ?? {}),
    ),
  createRefinement: (runnerId: string, correction: string) =>
    request<RefinementResultDto>(
      `/runners/${encodeURIComponent(runnerId)}/refinement`,
      jsonInit("POST", { correction }),
    ),
  applyRefinement: (runnerId: string, refinementId: string) =>
    request<RefinementResultDto>(
      `/runners/${encodeURIComponent(runnerId)}/refinement/${encodeURIComponent(refinementId)}/apply`,
      jsonInit("POST"),
    ),
  recordings: () => request<RecordingsDto>("/recordings"),
  recordingTargets: () => request<RecordingTargetsDto>("/recording-targets"),
  startRecording: (name: string, target: string) =>
    request<RecordingSummaryDto>("/recordings", jsonInit("POST", { name, target })),
  recording: (sessionId: string) =>
    request<RecordingSummaryDto>(`/recordings/${encodeURIComponent(sessionId)}`),
  recordingAction: (sessionId: string, action: string, value?: string) =>
    request<RecordingSummaryDto>(
      `/recordings/${encodeURIComponent(sessionId)}/${action}`,
      jsonInit("POST", value ? { value } : undefined),
    ),
  normaliseRecording: (sessionId: string) =>
    request<RecordingNormaliseResultDto>(
      `/recordings/${encodeURIComponent(sessionId)}/normalise`,
      jsonInit("POST"),
    ),
  testRecording: (sessionId: string) =>
    request<RecordingTestResultDto>(
      `/recordings/${encodeURIComponent(sessionId)}/test`,
      jsonInit("POST"),
    ),
  finaliseRecording: (sessionId: string) =>
    request<RecordingFinaliseResultDto>(
      `/recordings/${encodeURIComponent(sessionId)}/finalise`,
      jsonInit("POST"),
    ),
  mcpStatus: () => request<McpStatusDto>("/mcp/status"),
  mcpTools: () => request<McpToolsDto>("/mcp/tools"),
  mcpClientConfig: () => request<McpClientConfigDto>("/mcp/client-config"),
  mcpLifecycle: (action: "start" | "stop" | "restart") =>
    request<McpStatusDto>(`/mcp/${action}`, jsonInit("POST")),
  mcpToolAction: (id: string, action: "test" | "enable" | "disable" | "delete") =>
    request<McpToolActionResultDto>(
      `/mcp/tools/${encodeURIComponent(id)}/${action}`,
      jsonInit("POST"),
    ),
  llmSettings: () => request<LlmSettingsDto>("/settings/llm"),
  saveLlmSettings: (settings: LlmSettingsDto) =>
    request<LlmSettingsDto>("/settings/llm", jsonInit("PUT", settings)),
  testLlmSettings: () => request<LlmTestResultDto>("/settings/llm/test", jsonInit("POST")),
  createPlannerDraft: (prompt: string, profile = "default") =>
    request<PlannerDraftDto>("/planner/drafts", jsonInit("POST", { prompt, profile })),
  getPlannerDraft: (draftId: string) =>
    request<PlannerDraftDto>(`/planner/drafts/${encodeURIComponent(draftId)}`),
  patchPlannerDraft: (draftId: string, patch: Partial<PlannerDraftDto>) =>
    request<PlannerDraftDto>(
      `/planner/drafts/${encodeURIComponent(draftId)}`,
      jsonInit("PATCH", patch),
    ),
  testPlannerDraft: (draftId: string, sampleInputs: Record<string, string>) =>
    request<PlannerTestResultDto>(
      `/planner/drafts/${encodeURIComponent(draftId)}/test`,
      jsonInit("POST", { sampleInputs }),
    ),
  savePlannerDraft: (draftId: string) =>
    request<PlannerSaveResultDto>(
      `/planner/drafts/${encodeURIComponent(draftId)}/save`,
      jsonInit("POST"),
    ),
};
