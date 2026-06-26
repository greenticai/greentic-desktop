import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { RefinementResultDto, RunnerSummaryDto } from "@/lib/types";
import {
  Play,
  Pencil,
  Upload,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  FileCog,
  Plus,
  Trash2,
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
  published: { label: "Published", tone: "published" },
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
  const runnersQuery = useQuery({ queryKey: ["runners"], queryFn: api.runners });
  const evidence = useQuery({ queryKey: ["evidence"], queryFn: api.evidence });
  const approvals = useQuery({ queryKey: ["approvals"], queryFn: api.approvals });
  const runnerAction = useMutation({
    mutationFn: ({ id, action }: { id: string; action: string }) => api.runnerAction(id, action),
    onSuccess: () => {
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

  function runAction(runner: RunnerSummaryDto, action: string) {
    if (
      action === "delete" &&
      !window.confirm(`Delete "${runner.name}"? This removes the local runner package.`)
    ) {
      return;
    }
    runnerAction.mutate({ id: runner.id, action });
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
          <Link to="/create">
            <Plus className="h-4 w-4" /> New runner
          </Link>
        </Button>
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
                  <div className="font-semibold truncate">{r.name}</div>
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
                  onClick={() => runAction(r, "run")}
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
                onClick={() => runAction(r, "test")}
              >
                <Play className="h-3.5 w-3.5" />
                {activeAction?.id === r.id && activeAction.action === "test" ? "Testing" : "Test"}
              </Button>
              <Button size="sm" variant="outline" className="gap-1.5" asChild>
                <Link to="/create">
                  <Pencil className="h-3.5 w-3.5" /> Edit
                </Link>
              </Button>
              {r.friendly.tone !== "fix" && r.friendly.tone !== "published" && (
                <Button
                  size="sm"
                  variant="outline"
                  className="gap-1.5"
                  disabled={runnerAction.isPending}
                  onClick={() => runAction(r, "publish")}
                >
                  <Upload className="h-3.5 w-3.5" />
                  {activeAction?.id === r.id && activeAction.action === "publish"
                    ? "Publishing"
                    : "Publish as MCP"}
                </Button>
              )}
              <Button
                size="sm"
                variant="destructive"
                className="gap-1.5"
                disabled={runnerAction.isPending}
                onClick={() => runAction(r, "delete")}
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
