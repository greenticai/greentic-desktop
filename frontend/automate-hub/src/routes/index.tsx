import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import {
  Wand2,
  Video,
  Workflow,
  ArrowRight,
  CheckCircle2,
  AlertTriangle,
  Leaf,
} from "lucide-react";

export const Route = createFileRoute("/")({
  head: () => ({ meta: [{ title: "Home · Greentic Desktop" }] }),
  component: Home,
});

const actions = [
  {
    to: "/create",
    icon: Wand2,
    title: "Create from Prompt",
    text: "Describe the task you want to automate. Greentic will create a draft runner for you.",
    cta: "Start with a Prompt",
  },
  {
    to: "/create",
    icon: Video,
    title: "Record a Task",
    text: "Perform the task once. Greentic records your actions and turns them into a reusable runner.",
    cta: "Start Recording",
  },
  {
    to: "/runners",
    icon: Workflow,
    title: "Manage Runners",
    text: "View, test and publish your saved automations.",
    cta: "View My Runners",
  },
] as const;

function Home() {
  const runtime = useQuery({
    queryKey: ["runtime-info"],
    queryFn: api.runtimeInfo,
  });
  const setup = useQuery({
    queryKey: ["setup-checklist"],
    queryFn: api.setupChecklist,
  });
  const activity = useQuery({
    queryKey: ["activity"],
    queryFn: api.activity,
  });
  const checklist = setup.data?.items ?? [];
  const events = activity.data?.events ?? [];

  return (
    <div className="p-8 md:p-12 max-w-6xl mx-auto">
      <div className="text-center mb-12">
        <div
          className="inline-flex h-14 w-14 rounded-2xl items-center justify-center mb-5"
          style={{ background: "var(--gradient-primary)" }}
        >
          <Leaf className="h-7 w-7 text-primary-foreground" />
        </div>
        <h1 className="text-4xl font-semibold tracking-tight">
          Automate desktop tasks, no code needed
        </h1>
        <p className="text-muted-foreground mt-4 max-w-2xl mx-auto text-base leading-relaxed">
          Greentic Desktop lets you automate desktop and browser tasks by prompting or recording
          what you do. Once tested, your automation can be reused as an MCP tool by AI workers and
          Greentic flows.
        </p>
        <div className="mt-4 text-xs text-muted-foreground">
          {runtime.isLoading && "Loading local runtime..."}
          {runtime.isError && "Local runtime API is unavailable."}
          {runtime.data &&
            `Runtime ${runtime.data.appVersion} on ${runtime.data.platform} · ${runtime.data.runtimeHome}`}
        </div>
      </div>

      <div className="grid md:grid-cols-3 gap-5 mb-10">
        {actions.map((a) => {
          const Icon = a.icon;
          return (
            <Link
              key={a.title}
              to={a.to}
              className="group rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)] hover:shadow-[var(--shadow-elegant)] hover:border-primary/40 transition-all flex flex-col"
            >
              <div className="h-11 w-11 rounded-xl bg-primary/10 flex items-center justify-center mb-4">
                <Icon className="h-5 w-5 text-primary" />
              </div>
              <div className="font-semibold text-lg">{a.title}</div>
              <p className="text-sm text-muted-foreground mt-2 leading-relaxed flex-1">{a.text}</p>
              <div className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-primary">
                {a.cta}{" "}
                <ArrowRight className="h-4 w-4 group-hover:translate-x-0.5 transition-transform" />
              </div>
            </Link>
          );
        })}
      </div>

      <div className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
        <div className="flex items-center justify-between mb-4">
          <div>
            <div className="font-semibold">Setup checklist</div>
            <div className="text-xs text-muted-foreground mt-0.5">
              A few things Greentic needs to work smoothly.
            </div>
          </div>
          <Button asChild variant="outline" size="sm">
            <Link to="/settings">Open settings</Link>
          </Button>
        </div>
        {setup.isLoading && <div className="text-sm text-muted-foreground">Loading setup...</div>}
        {setup.isError && (
          <div className="text-sm text-destructive">
            Could not load setup state from the local runtime.
          </div>
        )}
        {!setup.isLoading && !setup.isError && checklist.length === 0 && (
          <div className="text-sm text-muted-foreground">No setup checks are available yet.</div>
        )}
        {checklist.length > 0 && (
          <ul className="divide-y">
            {checklist.map((c) => (
              <li key={c.label} className="flex items-center justify-between py-3">
                <div className="flex items-center gap-3 text-sm">
                  {c.ok ? (
                    <CheckCircle2 className="h-5 w-5 text-success" />
                  ) : (
                    <AlertTriangle className="h-5 w-5 text-warning" />
                  )}
                  <span className={c.ok ? "text-foreground" : "text-foreground"}>{c.label}</span>
                </div>
                {c.ok ? (
                  <span className="text-xs text-muted-foreground">Ready</span>
                ) : (
                  <Button size="sm" variant="outline">
                    Set up
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="mt-6 rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
        <div className="mb-4">
          <div className="font-semibold">Recent activity</div>
          <div className="text-xs text-muted-foreground mt-0.5">
            Local runtime events from runner tests, approvals, and publishing.
          </div>
        </div>
        {activity.isLoading && (
          <div className="text-sm text-muted-foreground">Loading activity...</div>
        )}
        {activity.isError && (
          <div className="text-sm text-destructive">Could not load recent activity.</div>
        )}
        {!activity.isLoading && !activity.isError && events.length === 0 && (
          <div className="text-sm text-muted-foreground">No activity recorded yet.</div>
        )}
        {events.length > 0 && (
          <ul className="divide-y">
            {events.slice(0, 6).map((event) => (
              <li key={event.id} className="flex items-center justify-between gap-4 py-3">
                <div className="text-sm">{event.message}</div>
                <div className="text-xs text-muted-foreground">{event.timestamp}</div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
