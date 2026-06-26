import { createFileRoute } from "@tanstack/react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import type { ExtensionDto, LlmSettingsDto, SetupChecklistItemDto } from "@/lib/types";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  CheckCircle2,
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Play,
  Download,
} from "lucide-react";

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "Settings · Greentic Desktop" }] }),
  component: SettingsPage,
});

const desktopSetup: SetupChecklistItemDto[] = [
  { id: "loading", label: "Loading setup", ok: false, status: "warning", help: "" },
];

function approvalForExtension(extension: ExtensionDto) {
  const permissions = new Set(extension.permissions ?? []);
  const riskyPrompts = extension.permissionPrompts?.length
    ? extension.permissionPrompts
    : extension.permissions;
  if (permissions.size === 0) {
    return {};
  }

  const requiresApproval =
    permissions.has("screen_capture") ||
    permissions.has("keyboard_mouse") ||
    permissions.has("filesystem.write");
  if (!requiresApproval) {
    return {};
  }

  const accepted = window.confirm(
    `Install ${extension.name} with these permissions?\n\n${riskyPrompts.join("\n")}`,
  );
  if (!accepted) {
    throw new Error("Install cancelled");
  }

  return {
    approveScreenCapture: permissions.has("screen_capture"),
    approveKeyboardMouse: permissions.has("keyboard_mouse"),
    approveFilesystemWrite: permissions.has("filesystem.write"),
  };
}

function Card({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
      <div className="mb-4">
        <div className="font-semibold">{title}</div>
        {description && <div className="text-xs text-muted-foreground mt-1">{description}</div>}
      </div>
      {children}
    </div>
  );
}

function SettingsPage() {
  const [advanced, setAdvanced] = useState(false);
  const [query, setQuery] = useState("");
  const [actionStatus, setActionStatus] = useState<string | null>(null);

  const runtime = useQuery({ queryKey: ["runtime-info"], queryFn: api.runtimeInfo });
  const setup = useQuery({ queryKey: ["setup-checklist"], queryFn: api.setupChecklist });
  const recommended = useQuery({
    queryKey: ["extensions-recommended", query],
    queryFn: () =>
      query.trim() ? api.searchExtensions(query.trim()) : api.recommendedExtensions(),
  });
  const installed = useQuery({
    queryKey: ["extensions-installed"],
    queryFn: api.installedExtensions,
  });
  const llm = useQuery({ queryKey: ["llm-settings"], queryFn: api.llmSettings });
  const [llmDraft, setLlmDraft] = useState<LlmSettingsDto | null>(null);

  useEffect(() => {
    if (llm.data) {
      setLlmDraft(llm.data);
    }
  }, [llm.data]);

  const extensionAction = useMutation({
    mutationFn: ({ extension, action }: { extension: ExtensionDto; action: string }) =>
      action === "install"
        ? api.installExtension(`store://${extension.id}`, "latest", approvalForExtension(extension))
        : api.extensionAction(extension.id, action),
    onSuccess: (result) => setActionStatus(`${result.id ?? "extension"}: ${result.status}`),
    onError: (error) => setActionStatus(error instanceof Error ? error.message : "Action failed"),
  });
  const testLlm = useMutation({
    mutationFn: api.testLlmSettings,
    onSuccess: (result) => setActionStatus(result.message),
    onError: (error) => setActionStatus(error instanceof Error ? error.message : "LLM test failed"),
  });
  const setupAction = useMutation({
    mutationFn: ({ id, action }: { id: string; action?: string }) => api.setupFix(id, action),
    onSuccess: (result) => setActionStatus(result.message),
    onError: (error) => setActionStatus(error instanceof Error ? error.message : "Setup failed"),
  });
  const saveLlm = useMutation({
    mutationFn: () => api.saveLlmSettings(llmDraft!),
    onSuccess: (settings) => {
      setLlmDraft(settings);
      setActionStatus("LLM settings saved");
    },
    onError: (error) => setActionStatus(error instanceof Error ? error.message : "Save failed"),
  });

  const setupItems = setup.data?.items ?? (setup.isLoading ? desktopSetup : []);
  const installedIds = new Set((installed.data?.extensions ?? []).map((extension) => extension.id));
  const extensions = (recommended.data?.extensions ?? []).map((extension) => ({
    ...extension,
    installed: extension.installed || installedIds.has(extension.id),
  }));
  const llmProviders = llmDraft?.providers ?? [];
  const selectedProvider = llmProviders.find((provider) => provider.id === llmDraft?.provider);

  return (
    <div className="p-8 md:p-12 max-w-4xl mx-auto space-y-6">
      <div>
        <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
        <p className="text-muted-foreground mt-2 text-sm">
          Get Greentic set up and configure how it talks to your apps.
        </p>
      </div>

      <Card title="Desktop setup" description="Greentic needs these to run your automations.">
        {setup.isError && (
          <div className="text-sm text-destructive">Could not load setup state.</div>
        )}
        {!setup.isError && setupItems.length === 0 && (
          <div className="text-sm text-muted-foreground">No setup checks are available.</div>
        )}
        <ul className="divide-y">
          {setupItems.map((s) => (
            <li key={s.id} className="flex items-start justify-between gap-4 py-3">
              <div className="flex items-start gap-3 min-w-0">
                {s.ok ? (
                  <CheckCircle2 className="h-5 w-5 text-success mt-0.5 shrink-0" />
                ) : (
                  <AlertTriangle className="h-5 w-5 text-warning mt-0.5 shrink-0" />
                )}
                <div className="min-w-0">
                  <div className="text-sm font-medium">{s.label}</div>
                  <div className="text-xs text-muted-foreground">{s.help}</div>
                </div>
              </div>
              {s.ok ? (
                <span className="text-xs text-muted-foreground shrink-0">Ready</span>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={setupAction.isPending}
                  onClick={() => setupAction.mutate({ id: s.id, action: s.action })}
                >
                  Fix
                </Button>
              )}
            </li>
          ))}
        </ul>
      </Card>

      <Card title="Extensions" description="Add support for the apps you want to automate.">
        <div className="mb-3">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search extensions"
          />
        </div>
        {recommended.isLoading && (
          <div className="text-sm text-muted-foreground">Loading extensions...</div>
        )}
        {recommended.isError && (
          <div className="text-sm text-destructive">Could not load extensions.</div>
        )}
        <ul className="divide-y">
          {extensions.map((e) => (
            <ExtensionRow
              key={e.id}
              extension={e}
              busy={extensionAction.isPending}
              onAction={(action) => extensionAction.mutate({ extension: e, action })}
            />
          ))}
        </ul>
        {actionStatus && <div className="mt-3 text-xs text-muted-foreground">{actionStatus}</div>}
      </Card>

      <Card
        title="LLM"
        description="Greentic uses the LLM to turn prompts into draft runners and to suggest fixes when tests fail."
      >
        <div className="grid sm:grid-cols-2 gap-4">
          <div>
            <Label>Provider</Label>
            <Select
              value={llmDraft?.provider ?? ""}
              disabled={!llmDraft || llmProviders.length === 0}
              onValueChange={(providerId) => {
                const provider = llmProviders.find((candidate) => candidate.id === providerId);
                if (!provider || !llmDraft) {
                  return;
                }
                setLlmDraft({
                  ...llmDraft,
                  provider: provider.id,
                  model: provider.defaultModel,
                  endpoint: provider.endpoint,
                  secretRef: provider.secretName ? `secret://${provider.secretName}` : null,
                  mode: provider.mode,
                });
              }}
            >
              <SelectTrigger className="mt-1.5">
                <SelectValue placeholder="Select provider" />
              </SelectTrigger>
              <SelectContent>
                {llmProviders.map((provider) => (
                  <SelectItem key={provider.id} value={provider.id}>
                    {llmProviderName(provider)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {!llm.isLoading && llmProviders.length === 0 && (
              <div className="mt-1.5 text-xs text-destructive">
                No LLM providers were returned by the runtime.
              </div>
            )}
          </div>
          <div>
            <Label>Model</Label>
            <Input className="mt-1.5" readOnly value={llmDraft?.model ?? ""} />
          </div>
        </div>
        {selectedProvider?.secretName && (
          <div className="mt-3 text-xs text-muted-foreground">
            API key is read from {selectedProvider.secretName}.
          </div>
        )}
        <div className="mt-4 flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={testLlm.isPending}
            onClick={() => testLlm.mutate()}
          >
            Test connection
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={!llmDraft || saveLlm.isPending}
            onClick={() => saveLlm.mutate()}
          >
            Save
          </Button>
        </div>
      </Card>

      <button
        onClick={() => setAdvanced(!advanced)}
        className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
      >
        {advanced ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}{" "}
        Advanced
      </button>

      {advanced && (
        <Card
          title="Advanced"
          description="For power users. Most people don't need to touch these."
        >
          <div className="space-y-4">
            <div>
              <Label>Runner storage path</Label>
              <Input
                className="mt-1.5 font-mono text-xs"
                readOnly
                value={runtime.data?.runtimeHome ?? ""}
              />
            </div>
            <div>
              <Label>MCP bind address</Label>
              <Input
                className="mt-1.5 font-mono text-xs"
                readOnly
                value={runtime.data?.config.mcpBind ?? ""}
              />
            </div>
            <div className="flex gap-2 pt-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  setActionStatus(`Logs are under ${runtime.data?.runtimeHome ?? "runtime home"}`)
                }
              >
                Open logs
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setActionStatus("Developer paths loaded")}
              >
                Developer settings
              </Button>
            </div>
          </div>
        </Card>
      )}
    </div>
  );
}

function ExtensionRow({
  extension,
  busy,
  onAction,
}: {
  extension: ExtensionDto;
  busy: boolean;
  onAction: (action: string) => void;
}) {
  return (
    <li className="flex items-start justify-between gap-4 py-3">
      <div className="min-w-0">
        <div className="text-sm font-medium">{extension.name}</div>
        <div className="text-xs text-muted-foreground mt-0.5">{extension.description}</div>
        <div className="mt-2 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
          {extension.publisher && <span>Publisher {extension.publisher}</span>}
          {extension.trust && <span>Trust {extension.trust}</span>}
          {extension.platformCompatible === false && <span>Unsupported on this platform</span>}
          {(extension.permissionPrompts?.length
            ? extension.permissionPrompts
            : extension.permissions
          ).map((permission) => (
            <span key={permission}>{permission}</span>
          ))}
        </div>
      </div>
      <div className="flex flex-wrap justify-end gap-2 shrink-0">
        {extension.installed ? (
          <>
            <span className="inline-flex items-center gap-1.5 text-xs text-success font-medium">
              <CheckCircle2 className="h-3.5 w-3.5" /> Installed
            </span>
            <Button
              size="sm"
              variant="outline"
              className="gap-1.5"
              disabled={busy}
              onClick={() => onAction("health")}
            >
              <Play className="h-3 w-3" /> Test
            </Button>
            <Button size="sm" variant="outline" disabled={busy} onClick={() => onAction("verify")}>
              Verify
            </Button>
            <Button size="sm" variant="outline" disabled={busy} onClick={() => onAction("update")}>
              Update
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={() => onAction(extension.enabled === false ? "enable" : "disable")}
            >
              {extension.enabled === false ? "Enable" : "Disable"}
            </Button>
            <Button size="sm" variant="outline" disabled={busy} onClick={() => onAction("remove")}>
              Remove
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            disabled={
              busy || extension.platformCompatible === false || extension.available === false
            }
            onClick={() => onAction("install")}
          >
            <Download className="h-3.5 w-3.5" /> Install
          </Button>
        )}
      </div>
    </li>
  );
}

function llmProviderName(provider: { name?: string; label?: string; id: string }) {
  return provider.name || provider.label || provider.id;
}
