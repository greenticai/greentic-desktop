import { createFileRoute, Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/mcp")({
  head: () => ({ meta: [{ title: "MCP Publishing · Greentic Desktop" }] }),
  component: MCPPage,
});

function MCPPage() {
  return (
    <div className="p-8 md:p-12 max-w-3xl mx-auto">
      <div className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
        <h1 className="text-2xl font-semibold tracking-tight">MCP publishing moved</h1>
        <p className="mt-2 text-sm text-muted-foreground">
          MCP publishing is managed from My Runners now, alongside run, test, edit, and delete
          actions.
        </p>
        <Button asChild className="mt-5">
          <Link to="/runners">Open My Runners</Link>
        </Button>
      </div>
    </div>
  );
}
