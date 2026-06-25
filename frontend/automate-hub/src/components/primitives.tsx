import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { CheckCircle2, AlertTriangle, XCircle, Info, Shield, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

export function PageHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-6 mb-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && (
          <p className="text-muted-foreground mt-1 text-sm max-w-2xl">{description}</p>
        )}
      </div>
      {action}
    </div>
  );
}

const statusStyles: Record<string, string> = {
  connected: "bg-success/15 text-success border-success/20",
  idle: "bg-muted text-muted-foreground border-border",
  recording: "bg-destructive/15 text-destructive border-destructive/20",
  replaying: "bg-info/15 text-info border-info/20",
  failed: "bg-destructive/15 text-destructive border-destructive/20",
  failing: "bg-destructive/15 text-destructive border-destructive/20",
  enabled: "bg-success/15 text-success border-success/20",
  disabled: "bg-muted text-muted-foreground border-border",
  draft: "bg-muted text-muted-foreground border-border",
  validated: "bg-info/15 text-info border-info/20",
  approved: "bg-primary/15 text-primary border-primary/20",
  published: "bg-success/15 text-success border-success/20",
  deprecated: "bg-warning/20 text-foreground border-warning/30",
};

export function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium",
        statusStyles[status] ?? "bg-muted text-muted-foreground border-border",
      )}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {status}
    </span>
  );
}

export function RiskBadge({ risk }: { risk: "low" | "medium" | "high" }) {
  const map = {
    low: "bg-success/15 text-success border-success/20",
    medium: "bg-warning/20 text-foreground border-warning/30",
    high: "bg-destructive/15 text-destructive border-destructive/20",
  } as const;
  return (
    <Badge variant="outline" className={cn("font-medium capitalize", map[risk])}>
      {risk} risk
    </Badge>
  );
}

export function CapabilityBadge({ children }: { children: ReactNode }) {
  return (
    <Badge variant="secondary" className="font-normal">
      {children}
    </Badge>
  );
}

export function PermissionCheck({ label, ok }: { label: string; ok: boolean }) {
  return (
    <div className="flex items-center gap-2 text-sm">
      {ok ? (
        <CheckCircle2 className="h-4 w-4 text-success" />
      ) : (
        <AlertTriangle className="h-4 w-4 text-warning" />
      )}
      <span className={ok ? "text-foreground" : "text-muted-foreground"}>{label}</span>
    </div>
  );
}

export function EmptyState({
  icon: Icon = Info,
  title,
  description,
  action,
}: {
  icon?: LucideIcon;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="border border-dashed rounded-xl p-10 text-center bg-card/50">
      <div className="h-12 w-12 rounded-full bg-accent mx-auto flex items-center justify-center mb-4">
        <Icon className="h-5 w-5 text-accent-foreground" />
      </div>
      <div className="font-medium">{title}</div>
      {description && (
        <div className="text-sm text-muted-foreground mt-1 max-w-sm mx-auto">{description}</div>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function ActivityIcon({ kind }: { kind: string }) {
  if (kind === "success") return <CheckCircle2 className="h-4 w-4 text-success" />;
  if (kind === "error") return <XCircle className="h-4 w-4 text-destructive" />;
  if (kind === "warning") return <AlertTriangle className="h-4 w-4 text-warning" />;
  return <Info className="h-4 w-4 text-info" />;
}

export function SectionCard({
  title,
  description,
  children,
  action,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="rounded-xl border bg-card shadow-[var(--shadow-card)] overflow-hidden">
      <div className="px-5 py-4 border-b flex items-center justify-between gap-3">
        <div>
          <div className="font-semibold text-sm">{title}</div>
          {description && <div className="text-xs text-muted-foreground mt-0.5">{description}</div>}
        </div>
        {action}
      </div>
      <div className="p-5">{children}</div>
    </div>
  );
}

export function StatCard({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string;
  value: string | number;
  hint?: string;
  tone?: "default" | "success" | "warning" | "danger";
}) {
  const toneClass = {
    default: "text-foreground",
    success: "text-success",
    warning: "text-warning",
    danger: "text-destructive",
  }[tone];
  return (
    <div className="rounded-xl border bg-card p-5 shadow-[var(--shadow-card)]">
      <div className="text-xs text-muted-foreground font-medium uppercase tracking-wide">
        {label}
      </div>
      <div className={cn("text-3xl font-semibold mt-2", toneClass)}>{value}</div>
      {hint && <div className="text-xs text-muted-foreground mt-1">{hint}</div>}
    </div>
  );
}

export function ShieldNote({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-start gap-2 rounded-lg border bg-muted/50 p-3 text-xs text-muted-foreground">
      <Shield className="h-4 w-4 mt-0.5 text-primary shrink-0" />
      <div>{children}</div>
    </div>
  );
}
