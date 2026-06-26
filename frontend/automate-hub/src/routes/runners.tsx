import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { RefinementResultDto, RunnerActionResultDto, RunnerSummaryDto } from "@/lib/types";
import {
  Play,
  Pencil,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  FileCog,
  Plus,
  Trash2,
  Copy,
} from "lucide-react";

export const Route = createFileRoute("/runners")({
  head: () => ({ meta: [{ title: "My Runners · Greentic Desktop" }] }),
  component: RunnersPage,
});

const friendlyStatus: Record<
  string,
  { label: string; tone: "draft" | "tested" | "published" | "fix" }
> = {
  draft: { label: "Draft", tone: "draft" },
  validated: { label: "Tested", tone: "tested" },
  approved: { label: "Tested", tone: "tested" },
  published: { label: "Ready", tone: "published" },
  failed: { label: "Needs fixing", tone: "fix" },
};

function StatusPill({ tone, label }: { tone: string; label: string }) {
  const map: Record<string, string> = {
    draft: "bg-muted text-muted-foreground",
    tested: "bg-info/15 text-info",
    published: "bg-success/15 text-success",
    fix: "bg-warning/20 text-foreground",
  };
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${map[tone]}`}
    >
      {tone === "published" && <CheckCircle2 className="h-3 w-3" />}
      {tone === "fix" && <AlertTriangle className="h-3 w-3" />}
      {label}
    </span>
  );
}

function RunnersPage() {
  const queryClient = useQueryClient();
  const [refineRunnerId, setRefineRunnerId] = useState<string | null>(null);
  const [correction, setCorrection] = useState("");
  const [refinement, setRefinement] = useState<RefinementResultDto | null>(null);
  const [execution, setExecution] = useState<{ runnerId: string; action: "run" | "test" } | null>(
    null,
  );
  const [runnerInputs, setRunnerInputs] = useState<Record<string, Record<string, string>>>({});
  const [runnerResults, setRunnerResults] = useState<Record<string, RunnerActionResultDto>>({});
  const [runnerErrors, setRunnerErrors] = useState<Record<string, string>>({});
  const [renamingRunnerId, setRenamingRunnerId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const runnersQuery = useQuery({ queryKey: ["runners"], queryFn: api.runners });
  const mcpStatus = useQuery({ queryKey: ["mcp-status"], queryFn: api.mcpStatus });
  const mcpConfig = useQuery({ queryKey: ["mcp-client-config"], queryFn: api.mcpClientConfig });
  const evidence = useQuery({ queryKey: ["evidence"], queryFn: api.evidence });
  const approvals = useQuery({ queryKey: ["approvals"], queryFn: api.approvals });
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
      setRunnerResults((current) => ({ ...current, [result.runnerId]: result }));
      setRunnerErrors((current) => {
        const next = { ...current };
        delete next[result.runnerId];
        return next;
      });
      setExecution(null);
      if (result.action === "rename") {
        setRenamingRunnerId(null);
        setRenameValue("");
      }
      void queryClient.invalidateQueries({ queryKey: ["runners"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-status"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-tools"] });
      void queryClient.invalidateQueries({ queryKey: ["evidence"] });
      void queryClient.invalidateQueries({ queryKey: ["approvals"] });
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (error, variables) => {
      setRunnerErrors((current) => ({
        ...current,
        [variables.id]: error instanceof Error ? error.message : "Runner action failed",
      }));
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
  const busy = runnerAction.isPending;
  const items = (runnersQuery.data?.runners ?? []).map((r) => ({
    ...r,
    friendly:
      r.lastTest === "failed"
        ? friendlyStatus.failed
        : (friendlyStatus[r.status] ?? friendlyStatus.draft),
  }));
  const activeAction = runnerAction.variables;

  function runAction(runner: RunnerSummaryDto, action: string, inputs?: Record<string, string>) {
    if (
      action === "delete" &&
      !window.confirm(`Delete "${runner.name}"? This removes the runner and its MCP tool.`)
    ) {
      return;
    }
    runnerAction.mutate({ id: runner.id, action, inputs });
  }

  function startExecution(runner: RunnerSummaryDto, action: "run" | "test") {
    if ((runner.inputFields ?? []).length === 0) {
      runAction(runner, action, {});
      return;
    }
    setExecution({ runnerId: runner.id, action });
  }

  function updateRunnerInput(runnerId: string, field: string, value: string) {
    setRunnerInputs((current) => ({
      ...current,
      [runnerId]: { ...(current[runnerId] ?? {}), [field]: value },
    }));
  }

  function startRename(runner: RunnerSummaryDto) {
    setRenamingRunnerId(runner.id);
    setRenameValue(runner.name);
  }

  function saveRename(runner: RunnerSummaryDto) {
    runAction(runner, "rename", { name: renameValue });
  }

  function runnerTaskPrompt(runner: RunnerSummaryDto) {
    const description = runner.description?.trim();
    if (description && description !== "Local runner package managed by Greentic Desktop.") {
      return description;
    }
    return runner.name;
  }

  return (
    <div className="p-8 md:p-12 max-w-6xl mx-auto">
      <div className="flex items-start justify-between gap-6 mb-8">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">My Runners</h1>
          <p className="text-muted-foreground mt-2 text-sm">
            Your saved automations. Run them, test them, or publish them as MCP tools.
          </p>
        </div>
        <Button asChild className="gap-2">
          <Link to="/create" search={{ mode: undefined, prompt: undefined, runnerId: undefined }}>
            <Plus className="h-4 w-4" /> New runner
          </Link>
        </Button>
      </div>

      <div className="mb-6 rounded-2xl border bg-card p-5 shadow-[var(--shadow-card)]">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="font-medium text-sm">MCP runner service</div>
            <div className="mt-1 text-xs text-muted-foreground">
              Server {mcpStatus.data?.status ?? "configured"} on{" "}
              {mcpStatus.data?.bind ?? mcpConfig.data?.localUrl ?? "local runtime"}. Every runner
              is exposed as an MCP tool by default.
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              className="gap-1.5"
              onClick={() =>
                void navigator.clipboard?.writeText(mcpConfig.data?.clientJson ?? "{}")
              }
            >
              <Copy className="h-3.5 w-3.5" /> Copy MCP config
            </Button>
          </div>
        </div>
        {(mcpStatus.isError || mcpConfig.isError) && (
          <div className="mt-3 text-xs text-destructive">
            Some MCP state could not be loaded from the local runtime.
          </div>
        )}
      </div>

      {runnerAction.data && (
        <div className="mb-5 rounded-lg border bg-card px-4 py-3 text-sm">
          {runnerAction.data.runnerId}: {runnerAction.data.action} {runnerAction.data.status}
          <span className="ml-2 text-muted-foreground">{runnerAction.data.evidenceRef}</span>
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
                  {renamingRunnerId === r.id ? (
                    <div className="flex items-center gap-2">
                      <Input
                        className="h-8"
                        value={renameValue}
                        onChange={(event) => setRenameValue(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") saveRename(r);
                          if (event.key === "Escape") setRenamingRunnerId(null);
                        }}
                        autoFocus
                      />
                      <Button
                        size="sm"
                        disabled={busy || !renameValue.trim()}
                        onClick={() => saveRename(r)}
                      >
                        Save
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => setRenamingRunnerId(null)}
                      >
                        Cancel
                      </Button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 min-w-0">
                      <div className="font-semibold truncate">{r.name}</div>
                      <button
                        type="button"
                        className="shrink-0 text-muted-foreground hover:text-foreground"
                        aria-label={`Rename ${r.name}`}
                        onClick={() => startRename(r)}
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  )}
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
            <div className="mt-3 text-xs text-muted-foreground">
              MCP tool runner.{r.id.replaceAll("-", ".")} · {r.name}
            </div>
            <div className="mt-5 flex flex-wrap gap-2">
              {r.friendly.tone === "fix" ? (
                <Button
                  size="sm"
                  variant="default"
                  className="gap-1.5"
                  disabled={busy}
                  onClick={() => setRefineRunnerId(r.id)}
                >
                  <Wrench className="h-3.5 w-3.5" /> Fix
                </Button>
              ) : (
                <Button
                  size="sm"
                  className="gap-1.5"
                  disabled={busy}
                  onClick={() => startExecution(r, "run")}
                >
                  <Play className="h-3.5 w-3.5" />
                  {activeAction?.id === r.id && activeAction.action === "run" ? "Running" : "Run"}
                </Button>
              )}
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                disabled={busy}
                onClick={() => startExecution(r, "test")}
              >
                <Play className="h-3.5 w-3.5" />
                {activeAction?.id === r.id && activeAction.action === "test" ? "Testing" : "Test"}
              </Button>
              <Button size="sm" variant="outline" className="gap-1.5" asChild>
                <Link
                  to="/create"
                  search={{
                    mode: "prompt",
                    runnerId: r.id,
                    prompt: runnerTaskPrompt(r),
                  }}
                >
                  <Pencil className="h-3.5 w-3.5" /> Edit steps
                </Link>
              </Button>
              <Button
                size="sm"
                variant="destructive"
                className="gap-1.5"
                disabled={busy}
                onClick={() => runAction(r, "delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
                {activeAction?.id === r.id && activeAction.action === "delete"
                  ? "Deleting"
                  : "Delete"}
              </Button>
            </div>
            {execution?.runnerId === r.id && (
              <div className="mt-4 rounded-lg border bg-muted/40 p-3">
                <div className="text-sm font-medium">
                  {execution.action === "run" ? "Run" : "Test"} inputs
                </div>
                <div className="mt-3 grid gap-3 sm:grid-cols-2">
                  {(r.inputFields ?? []).map((field) => (
                    <div key={field}>
                      <label className="text-xs text-muted-foreground">{field}</label>
                      <Input
                        className="mt-1"
                        value={runnerInputs[r.id]?.[field] ?? ""}
                        onChange={(event) => updateRunnerInput(r.id, field, event.target.value)}
                      />
                    </div>
                  ))}
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    disabled={busy}
                    onClick={() => runAction(r, execution.action, runnerInputs[r.id] ?? {})}
                  >
                    {execution.action === "run" ? "Run" : "Test"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => setExecution(null)}>
                    Cancel
                  </Button>
                </div>
              </div>
            )}
            {runnerResults[r.id] && (
              <div className="mt-4 rounded-lg border bg-muted/40 p-3">
                <div className="text-sm font-medium">
                  {runnerResults[r.id].action} {runnerResults[r.id].status}
                </div>
                <div className="mt-2 grid gap-2 text-xs">
                  {Object.entries(runnerResults[r.id].outputs).map(([name, value]) => (
                    <div key={name} className="flex justify-between gap-3">
                      <span className="text-muted-foreground">{name}</span>
                      <span className="font-mono text-right">{value}</span>
                    </div>
                  ))}
                </div>
                <div className="mt-2 text-xs text-muted-foreground">
                  Evidence {runnerResults[r.id].evidenceRef}
                </div>
              </div>
            )}
            {runnerErrors[r.id] && (
              <div className="mt-4 rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
                {runnerErrors[r.id]}
              </div>
            )}
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
