import { createFileRoute } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { Plug, Play, Copy, Power, RotateCw, CheckCircle2, Circle, Trash2 } from "lucide-react";

export const Route = createFileRoute("/mcp")({
  head: () => ({ meta: [{ title: "MCP Tools · Greentic Desktop" }] }),
  component: MCPPage,
});

type ToolAction = "test" | "enable" | "disable" | "delete";

function MCPPage() {
  const queryClient = useQueryClient();
  const [actionStatus, setActionStatus] = useState<string | null>(null);
  const status = useQuery({ queryKey: ["mcp-status"], queryFn: api.mcpStatus });
  const tools = useQuery({ queryKey: ["mcp-tools"], queryFn: api.mcpTools });
  const config = useQuery({ queryKey: ["mcp-client-config"], queryFn: api.mcpClientConfig });
  const lifecycle = useMutation({
    mutationFn: (action: "start" | "stop" | "restart") => api.mcpLifecycle(action),
    onSuccess: (result) => {
      setActionStatus(`MCP server ${result.status} on ${result.bind}`);
      void queryClient.invalidateQueries({ queryKey: ["mcp-status"] });
    },
    onError: (error) =>
      setActionStatus(error instanceof Error ? error.message : "MCP action failed"),
  });
  const toolAction = useMutation({
    mutationFn: ({ id, action }: { id: string; action: ToolAction }) =>
      api.mcpToolAction(id, action),
    onSuccess: (result) => {
      setActionStatus(`${result.toolName}: ${result.action} ${result.status}`);
      void queryClient.invalidateQueries({ queryKey: ["mcp-tools"] });
      void queryClient.invalidateQueries({ queryKey: ["mcp-status"] });
      void queryClient.invalidateQueries({ queryKey: ["runners"] });
    },
    onError: (error) =>
      setActionStatus(error instanceof Error ? error.message : "Tool action failed"),
  });
  const mcpTools = tools.data?.tools ?? [];
  const enabledTools = mcpTools.filter((tool) => tool.status === "enabled").length;
  const busy = lifecycle.isPending || toolAction.isPending;

  function runToolAction(id: string, action: ToolAction) {
    const tool = mcpTools.find((candidate) => candidate.id === id);
    if (
      action === "delete" &&
      !window.confirm(
        `Delete MCP tool "${tool?.name ?? id}"? This unpublishes it but keeps the runner.`,
      )
    ) {
      return;
    }
    toolAction.mutate({ id, action });
  }

  return (
    <div className="p-8 md:p-12 max-w-6xl mx-auto">
      <div className="mb-8">
        <h1 className="text-3xl font-semibold tracking-tight">MCP Tools</h1>
        <p className="text-muted-foreground mt-2 text-sm max-w-2xl">
          Your published runners, available to AI workers and Greentic flows.
        </p>
      </div>

      <div className="rounded-2xl border bg-card p-5 mb-6 shadow-[var(--shadow-card)] flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <span className="relative flex h-3 w-3">
            <span className="absolute inline-flex h-full w-full rounded-full bg-success/40 animate-ping" />
            <span className="relative inline-flex h-3 w-3 rounded-full bg-success" />
          </span>
          <div>
            <div className="font-medium text-sm">
              MCP server {status.data?.status ?? "configured"}
            </div>
            <div className="text-xs text-muted-foreground">
              {enabledTools} tools available on {status.data?.bind ?? "local runtime"}
            </div>
          </div>
        </div>
        <div className="flex gap-2">
          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            disabled={busy}
            onClick={() => lifecycle.mutate("restart")}
          >
            <RotateCw className="h-3.5 w-3.5" /> Restart
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            disabled={busy}
            onClick={() => lifecycle.mutate(status.data?.status === "running" ? "stop" : "start")}
          >
            <Power className="h-3.5 w-3.5" />
            {status.data?.status === "running" ? "Stop" : "Start"}
          </Button>
        </div>
      </div>

      {actionStatus && (
        <div className="mb-5 rounded-lg border bg-card px-4 py-3 text-sm">{actionStatus}</div>
      )}
      {status.isError && (
        <div className="mb-5 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          Could not load MCP status.
        </div>
      )}
      {tools.isError && (
        <div className="mb-5 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          Could not load MCP tools.
        </div>
      )}
      {!tools.isError && tools.isLoading && (
        <div className="rounded-lg border bg-card px-4 py-3 text-sm text-muted-foreground">
          Loading MCP tools...
        </div>
      )}
      {!tools.isError && !tools.isLoading && mcpTools.length === 0 && (
        <div className="rounded-lg border bg-card px-4 py-8 text-sm text-muted-foreground">
          No runners have been published as MCP tools.
        </div>
      )}

      <div className="rounded-2xl border bg-card p-5 mb-6 shadow-[var(--shadow-card)]">
        <div className="font-medium text-sm">Client configuration</div>
        <div className="mt-2 text-xs text-muted-foreground">
          Local endpoint {config.data?.localUrl ?? status.data?.bind ?? "not available"}
        </div>
        <pre className="mt-3 overflow-x-auto rounded-lg bg-muted p-3 text-xs">
          {config.data?.clientJson ?? "{}"}
        </pre>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            onClick={() => void navigator.clipboard?.writeText(config.data?.clientJson ?? "{}")}
          >
            <Copy className="h-3.5 w-3.5" /> Copy config
          </Button>
          <Button size="sm" variant="outline" asChild>
            <a href="/docs/aws-workspaces-mcp.md">AWS WorkSpaces</a>
          </Button>
        </div>
      </div>

      <div className="grid md:grid-cols-2 gap-5">
        {mcpTools.map((t) => (
          <div key={t.id} className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3 min-w-0">
                <div className="h-10 w-10 rounded-xl bg-primary/10 flex items-center justify-center shrink-0">
                  <Plug className="h-5 w-5 text-primary" />
                </div>
                <div className="min-w-0">
                  <div className="font-mono text-sm font-semibold truncate">{t.name}</div>
                  <div className="text-xs text-muted-foreground">from {t.runner}</div>
                </div>
              </div>
              <span
                className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${t.status === "enabled" ? "bg-success/15 text-success" : "bg-warning/20 text-foreground"}`}
              >
                {t.status === "enabled" ? (
                  <CheckCircle2 className="h-3 w-3" />
                ) : (
                  <Circle className="h-3 w-3" />
                )}
                {t.status === "enabled" ? "Enabled" : "Disabled"}
              </span>
            </div>
            <p className="text-sm text-muted-foreground mt-3 leading-relaxed">{t.description}</p>
            <div className="text-xs text-muted-foreground mt-3">Last used {t.lastCall}</div>
            <div className="text-xs text-muted-foreground mt-1">
              {t.version ?? "local"} · {t.risk ?? "medium"} risk
            </div>
            <div className="mt-4 flex flex-wrap gap-2">
              <Button
                size="sm"
                className="gap-1.5"
                disabled={busy || t.status !== "enabled"}
                onClick={() => runToolAction(t.id, "test")}
              >
                <Play className="h-3.5 w-3.5" /> Test
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                disabled={busy}
                onClick={() =>
                  runToolAction(t.id, t.status === "enabled" ? "disable" : "enable")
                }
              >
                <Power className="h-3.5 w-3.5" />
                {t.status === "enabled" ? "Disable" : "Enable"}
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                onClick={() => void navigator.clipboard?.writeText(t.name)}
              >
                <Copy className="h-3.5 w-3.5" /> Copy tool name
              </Button>
              <Button
                size="sm"
                variant="destructive"
                className="gap-1.5"
                disabled={busy}
                onClick={() => runToolAction(t.id, "delete")}
              >
                <Trash2 className="h-3.5 w-3.5" /> Delete
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
