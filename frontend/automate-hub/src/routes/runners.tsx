import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { RefinementResultDto, RunnerFieldDto, RunnerSummaryDto } from "@/lib/types";
import {
  Play,
  Pencil,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  FileCog,
  Plus,
  Power,
  RotateCw,
  Trash2,
} from "lucide-react";

export const Route = createFileRoute("/runners")({
  head: () => ({ meta: [{ title: "My Runners · Greentic Desktop" }] }),
  component: RunnersPage,
});

const friendlyStatus: Record<string, { label: string; tone: "draft" | "tested" | "fix" }> = {
  draft: { label: "Draft", tone: "draft" },
  validated: { label: "Tested", tone: "tested" },
  approved: { label: "Tested", tone: "tested" },
  published: { label: "Ready", tone: "tested" },
  failed: { label: "Needs fixing", tone: "fix" },
};

function StatusPill({ tone, label }: { tone: string; label: string }) {
  const map: Record<string, string> = {
    draft: "bg-muted text-muted-foreground",
    tested: "bg-info/15 text-info",
    fix: "bg-warning/20 text-foreground",
  };
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${map[tone]}`}
    >
      {tone === "tested" && <CheckCircle2 className="h-3 w-3" />}
      {tone === "fix" && <AlertTriangle className="h-3 w-3" />}
      {label}
    </span>
  );
}

function runnerInputFields(runner: RunnerSummaryDto): RunnerFieldDto[] {
  const typed = runner.inputFields ?? [];
  if (typed.length > 0) return typed;
  return (runner.inputs ?? []).map((name) => ({ name, valueType: "String", required: true }));
}

function runnerSecretFields(runner: RunnerSummaryDto): RunnerFieldDto[] {
  const typed = runner.secretFields ?? [];
  if (typed.length > 0) return typed;
  return (runner.secrets ?? []).map((name) => ({
    name,
    valueType: "String",
    required: true,
    secret: true,
    hasValue: false,
  }));
}

function fieldTypeLabel(field: RunnerFieldDto): string {
  if (typeof field.valueType === "string") return field.valueType;
  if (field.valueType && "Enum" in field.valueType) return "Enum";
  return "String";
}

function RunnersPage() {
  const queryClient = useQueryClient();
  const [refineRunnerId, setRefineRunnerId] = useState<string | null>(null);
  const [runRunner, setRunRunner] = useState<RunnerSummaryDto | null>(null);
  const [runInputs, setRunInputs] = useState<Record<string, string>>({});
  const [renameRunner, setRenameRunner] = useState<RunnerSummaryDto | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [correction, setCorrection] = useState("");
  const [refinement, setRefinement] = useState<RefinementResultDto | null>(null);
  const autoStartAttempted = useRef(false);
  const runnersQuery = useQuery({ queryKey: ["runners"], queryFn: api.runners });
  const mcpStatus = useQuery({ queryKey: ["mcp-status"], queryFn: api.mcpStatus });
  const mcpTools = useQuery({ queryKey: ["mcp-tools"], queryFn: api.mcpTools });
  const evidence = useQuery({ queryKey: ["evidence"], queryFn: api.evidence });
  const approvals = useQuery({ queryKey: ["approvals"], queryFn: api.approvals });
  const mcpLifecycle = useMutation({
    mutationFn: (action: "start" | "stop" | "restart") => api.mcpLifecycle(action),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["mcp-status"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-tools"] });
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
  });
  const runnerAction = useMutation({
    mutationFn: ({
      id,
      action,
      inputs,
    }: {
      id: string;
      action: string;
      inputs?: Record<string, string>;
    }) => api.runnerAction(id, action, inputs),
    onSuccess: (result) => {
      if (result.action === "run") {
        setRunRunner(null);
      }
      if (result.action === "rename") {
        setRenameRunner(null);
        setRenameValue("");
      }
      if (result.action === "delete") {
        setRunRunner((current) => (current?.id === result.runnerId ? null : current));
        setRefineRunnerId((current) => (current === result.runnerId ? null : current));
        setRenameRunner((current) => (current?.id === result.runnerId ? null : current));
      }
      void queryClient.invalidateQueries({ queryKey: ["runners"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-status"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-tools"] });
      void queryClient.invalidateQueries({ queryKey: ["evidence"] });
      void queryClient.invalidateQueries({ queryKey: ["approvals"] });
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
  });
  const approvalAction = useMutation({
    mutationFn: ({ id, action }: { id: string; action: "approve" | "reject" }) =>
      api.approvalAction(id, action),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["approvals"] });
      void queryClient.invalidateQueries({ queryKey: ["runners"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-tools"] });
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
  });
  const refine = useMutation({
    mutationFn: () => api.createRefinement(refineRunnerId!, correction),
    onSuccess: setRefinement,
  });
  const applyRefine = useMutation({
    mutationFn: () => api.applyRefinement(refinement!.runnerId, refinement!.refinementId),
    onSuccess: (result) => {
      setRefinement(result);
      void queryClient.invalidateQueries({ queryKey: ["runners"] });
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
  });
  const items = (runnersQuery.data?.runners ?? []).map((r) => ({
    ...r,
    friendly:
      r.lastTest === "failed"
        ? friendlyStatus.failed
        : (friendlyStatus[r.status] ?? friendlyStatus.draft),
  }));
  const activeAction = runnerAction.variables;
  const mcpToolCount = mcpStatus.data?.tools ?? mcpTools.data?.tools.length ?? items.length;
  const mcpRunning = mcpStatus.data?.status === "running";
  const mcpBusy = mcpLifecycle.isPending;

  useEffect(() => {
    if (autoStartAttempted.current || mcpLifecycle.isPending) {
      return;
    }
    if (mcpStatus.data?.status === "stopped") {
      autoStartAttempted.current = true;
      mcpLifecycle.mutate("start");
    }
  }, [mcpLifecycle, mcpStatus.data?.status]);

  function runAction(runner: RunnerSummaryDto, action: string, inputs?: Record<string, string>) {
    runnerAction.mutate({ id: runner.id, action, inputs });
  }

  function deleteRunner(runner: RunnerSummaryDto) {
    if (
      window.confirm(
        `Delete ${runner.name}? This removes the runner and its automatically exposed MCP tool.`,
      )
    ) {
      runAction(runner, "delete");
    }
  }

  function openRename(runner: RunnerSummaryDto) {
    setRenameRunner(runner);
    setRenameValue(runner.name);
  }

  function editHref(runner: RunnerSummaryDto) {
    const suffix = typeof window === "undefined" ? "" : window.location.search;
    return `/runners/${encodeURIComponent(runner.id)}/edit${suffix}`;
  }

  function openRun(runner: RunnerSummaryDto) {
    const defaults = Object.fromEntries(
      [...runnerInputFields(runner), ...runnerSecretFields(runner)].map((field) => [
        field.name,
        field.defaultValue ?? "",
      ]),
    );
    setRunInputs(defaults);
    setRunRunner(runner);
  }

  return (
    <div className="p-8 md:p-12 max-w-6xl mx-auto">
      <div className="flex items-start justify-between gap-6 mb-8">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">My Runners</h1>
          <p className="text-muted-foreground mt-2 text-sm">
            Your saved automations. Each runner is automatically available as an MCP tool.
          </p>
        </div>
        <Button asChild className="gap-2">
          <Link to="/create">
            <Plus className="h-4 w-4" /> New runner
          </Link>
        </Button>
      </div>

      <div className="mb-6 rounded-2xl border bg-card p-5 shadow-[var(--shadow-card)]">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <span className="relative flex h-3 w-3">
              {mcpRunning && (
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success/40" />
              )}
              <span
                className={`relative inline-flex h-3 w-3 rounded-full ${
                  mcpRunning ? "bg-success" : "bg-muted-foreground"
                }`}
              />
            </span>
            <div>
              <div className="text-sm font-medium">
                MCP server {mcpStatus.data?.status ?? "checking"}
              </div>
              <div className="text-xs text-muted-foreground">
                {mcpToolCount} runner tools on {mcpStatus.data?.bind ?? "local runtime"}
              </div>
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              className="gap-1.5"
              disabled={mcpBusy}
              onClick={() => mcpLifecycle.mutate("restart")}
            >
              <RotateCw className="h-3.5 w-3.5" /> Restart MCP
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="gap-1.5"
              disabled={mcpBusy}
              onClick={() => mcpLifecycle.mutate(mcpRunning ? "stop" : "start")}
            >
              <Power className="h-3.5 w-3.5" />
              {mcpRunning ? "Stop MCP" : "Start MCP"}
            </Button>
          </div>
        </div>
        {mcpLifecycle.isError && (
          <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {mcpLifecycle.error instanceof Error
              ? mcpLifecycle.error.message
              : "MCP server action failed"}
          </div>
        )}
      </div>

      {runnerAction.data && (
        <div className="mb-5 rounded-lg border bg-card px-4 py-3 text-sm">
          {runnerAction.data.runnerId}: {runnerAction.data.action} {runnerAction.data.status}
          <span className="ml-2 text-muted-foreground">{runnerAction.data.evidenceRef}</span>
          {Object.keys(runnerAction.data.outputs).length > 0 && (
            <div className="mt-2 grid gap-1 text-xs text-muted-foreground">
              {Object.entries(runnerAction.data.outputs).map(([key, value]) => (
                <div key={key}>
                  {key}: <span className="font-medium text-foreground">{value}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {runRunner && (
        <div className="mb-5 rounded-lg border bg-card p-4">
          <div className="font-medium text-sm">Run {runRunner.name}</div>
          <div className="mt-3 grid gap-3 sm:grid-cols-3">
            {runnerInputFields(runRunner).map((field) => (
              <div key={field.name}>
                <label className="text-xs text-muted-foreground">
                  {field.name}
                  {field.required ? " *" : ""} · {fieldTypeLabel(field)}
                </label>
                <Input
                  className="mt-1"
                  value={runInputs[field.name] ?? ""}
                  onChange={(event) =>
                    setRunInputs((current) => ({ ...current, [field.name]: event.target.value }))
                  }
                />
                {field.validation && (
                  <div className="mt-1 text-[11px] text-muted-foreground">{field.validation}</div>
                )}
              </div>
            ))}
            {runnerSecretFields(runRunner).map((field) => (
              <div key={field.name}>
                <label className="text-xs text-muted-foreground">
                  {field.name}
                  {field.required ? " *" : ""} · Secret
                </label>
                <Input
                  className="mt-1"
                  type="password"
                  placeholder={field.hasValue ? "Saved secret will be used" : "Enter secret"}
                  value={runInputs[field.name] ?? ""}
                  onChange={(event) =>
                    setRunInputs((current) => ({ ...current, [field.name]: event.target.value }))
                  }
                />
                <div className="mt-1 text-[11px] text-muted-foreground">
                  {field.hasValue ? "Saved in local secrets" : "Saved locally when submitted"}
                </div>
              </div>
            ))}
          </div>
          {runnerInputFields(runRunner).length === 0 &&
            runnerSecretFields(runRunner).length === 0 && (
              <div className="mt-2 text-sm text-muted-foreground">
                This runner does not declare any inputs.
              </div>
            )}
          {(runRunner.outputFields ?? []).length > 0 && (
            <div className="mt-4 rounded-md border bg-muted/30 p-3">
              <div className="text-xs font-medium text-muted-foreground">Expected outputs</div>
              <div className="mt-2 grid gap-1 text-xs">
                {(runRunner.outputFields ?? []).map((field) => (
                  <div key={field.name} className="flex flex-wrap gap-x-2 gap-y-1">
                    <span className="font-medium">{field.name}</span>
                    <span className="text-muted-foreground">{fieldTypeLabel(field)}</span>
                    {field.proof && <span className="text-muted-foreground">{field.proof}</span>}
                  </div>
                ))}
              </div>
            </div>
          )}
          <div className="mt-4 flex gap-2">
            <Button
              size="sm"
              disabled={runnerAction.isPending}
              onClick={() => runAction(runRunner, "run", runInputs)}
            >
              Run
            </Button>
            <Button size="sm" variant="outline" onClick={() => setRunRunner(null)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
      {renameRunner && (
        <div className="mb-5 rounded-lg border bg-card p-4">
          <div className="font-medium text-sm">Rename {renameRunner.name}</div>
          <div className="mt-3 flex flex-col gap-3 sm:flex-row">
            <Input
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              aria-label="Runner name"
            />
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={!renameValue.trim() || runnerAction.isPending}
                onClick={() => runAction(renameRunner, "rename", { name: renameValue.trim() })}
              >
                Save
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setRenameRunner(null);
                  setRenameValue("");
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        </div>
      )}
      {(approvals.data?.approvals ?? []).filter((approval) => approval.status === "pending")
        .length > 0 && (
        <div className="mb-5 rounded-lg border bg-card p-4">
          <div className="mb-3 font-medium text-sm">Approvals</div>
          <div className="space-y-3">
            {(approvals.data?.approvals ?? [])
              .filter((approval) => approval.status === "pending")
              .map((approval) => (
                <div
                  key={approval.id}
                  className="flex flex-wrap items-center justify-between gap-3 text-sm"
                >
                  <div>
                    {approval.action} for {approval.runnerId}
                    <div className="text-xs text-muted-foreground">{approval.policyReason}</div>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      onClick={() => approvalAction.mutate({ id: approval.id, action: "approve" })}
                    >
                      Approve
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => approvalAction.mutate({ id: approval.id, action: "reject" })}
                    >
                      Reject
                    </Button>
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}
      {(evidence.data?.bundles ?? []).length > 0 && (
        <div className="mb-5 rounded-lg border bg-card p-4">
          <div className="mb-2 font-medium text-sm">Evidence</div>
          <div className="grid gap-2 text-xs text-muted-foreground md:grid-cols-2">
            {(evidence.data?.bundles ?? []).slice(0, 4).map((bundle) => (
              <a key={bundle.bundleId} href={`/api/v1/evidence/${bundle.bundleId}`}>
                {bundle.runnerId} · {bundle.status} · {bundle.inputsHash}
              </a>
            ))}
          </div>
        </div>
      )}
      {runnerAction.isError && (
        <div className="mb-5 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {runnerAction.error instanceof Error
            ? runnerAction.error.message
            : "Runner action failed"}
        </div>
      )}
      {runnersQuery.isError && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          Could not load runners.
        </div>
      )}
      {!runnersQuery.isError && runnersQuery.isLoading && (
        <div className="rounded-lg border bg-card px-4 py-3 text-sm text-muted-foreground">
          Loading runners...
        </div>
      )}
      {!runnersQuery.isError && !runnersQuery.isLoading && items.length === 0 && (
        <div className="rounded-lg border bg-card px-4 py-8 text-sm text-muted-foreground">
          No runners saved yet.
        </div>
      )}

      <div className="grid md:grid-cols-2 gap-5">
        {items.map((r) => (
          <div
            key={r.id}
            className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)] hover:shadow-[var(--shadow-elegant)] transition-shadow flex flex-col"
          >
            <div className="flex items-start justify-between gap-3 mb-2">
              <div className="flex items-center gap-3 min-w-0">
                <div className="h-10 w-10 rounded-xl bg-primary/10 flex items-center justify-center shrink-0">
                  <FileCog className="h-5 w-5 text-primary" />
                </div>
                <div className="min-w-0">
                  <div className="flex min-w-0 items-center gap-1.5">
                    <div className="font-semibold truncate">{r.name}</div>
                    <button
                      type="button"
                      className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      aria-label={`Rename ${r.name}`}
                      disabled={runnerAction.isPending}
                      onClick={() => openRename(r)}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    Last tested {r.lastTest || "unknown"}
                  </div>
                </div>
              </div>
              <StatusPill tone={r.friendly.tone} label={r.friendly.label} />
            </div>
            <p className="text-sm text-muted-foreground mt-2 leading-relaxed flex-1">
              {r.description ?? "Local runner package managed by Greentic Desktop."}
            </p>
            {r.evidenceRefs && r.evidenceRefs.length > 0 && (
              <div className="mt-3 truncate text-xs text-muted-foreground">
                Evidence {r.evidenceRefs[0]}
              </div>
            )}
            <div className="mt-5 flex flex-wrap gap-2">
              {r.friendly.tone === "fix" ? (
                <Button
                  size="sm"
                  variant="default"
                  className="gap-1.5"
                  disabled={runnerAction.isPending}
                  onClick={() => setRefineRunnerId(r.id)}
                >
                  <Wrench className="h-3.5 w-3.5" /> Fix
                </Button>
              ) : (
                <Button
                  size="sm"
                  className="gap-1.5"
                  disabled={runnerAction.isPending}
                  onClick={() => openRun(r)}
                >
                  <Play className="h-3.5 w-3.5" />
                  {activeAction?.id === r.id && activeAction.action === "run" ? "Running" : "Run"}
                </Button>
              )}
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                disabled={runnerAction.isPending}
                asChild
              >
                <a href={editHref(r)}>
                  <Pencil className="h-3.5 w-3.5" /> Edit
                </a>
              </Button>
              <Button
                size="sm"
                variant="destructive"
                className="gap-1.5"
                disabled={runnerAction.isPending}
                onClick={() => deleteRunner(r)}
              >
                <Trash2 className="h-3.5 w-3.5" />
                {activeAction?.id === r.id && activeAction.action === "delete"
                  ? "Deleting"
                  : "Delete"}
              </Button>
            </div>
            {refineRunnerId === r.id && (
              <div className="mt-4 rounded-lg border bg-muted/40 p-3">
                <div className="text-sm font-medium">Refine failed runner</div>
                <Input
                  className="mt-3"
                  value={correction}
                  onChange={(event) => setCorrection(event.target.value)}
                  placeholder="Describe the correction"
                />
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    disabled={!correction.trim() || refine.isPending}
                    onClick={() => refine.mutate()}
                  >
                    Preview fix
                  </Button>
                  {refinement && refinement.runnerId === r.id && (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={applyRefine.isPending}
                      onClick={() => applyRefine.mutate()}
                    >
                      Apply fix
                    </Button>
                  )}
                </div>
                {refinement && refinement.runnerId === r.id && (
                  <pre className="mt-3 overflow-x-auto rounded bg-background p-3 text-xs">
                    {refinement.diff.before}
                    {"\n---\n"}
                    {refinement.diff.after}
                  </pre>
                )}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
