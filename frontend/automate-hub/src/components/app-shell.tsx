import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { BrandLogo } from "@/components/brand-logo";
import { Home, Plus, Workflow, Settings, Circle } from "lucide-react";

const nav = [
  { to: "/", label: "Home", icon: Home },
  { to: "/create", label: "Create Runner", icon: Plus },
  { to: "/runners", label: "My Runners", icon: Workflow },
  { to: "/settings", label: "Settings", icon: Settings },
] as const;

export function AppShell() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <div className="min-h-screen flex w-full bg-background text-foreground">
      <aside className="w-64 shrink-0 border-r border-sidebar-border bg-sidebar flex flex-col">
        <div className="h-16 flex items-center gap-2.5 px-5 border-b border-sidebar-border">
          <div
            className="h-9 w-9 rounded-lg flex items-center justify-center bg-background border border-sidebar-border"
          >
            <BrandLogo className="h-7 w-7" />
          </div>
          <div>
            <div className="text-sm font-semibold leading-tight">Greentic</div>
            <div className="text-[11px] text-muted-foreground leading-tight">
              Desktop Automation
            </div>
          </div>
        </div>
        <nav className="flex-1 px-3 py-4 space-y-1 overflow-y-auto">
          {nav.map((item) => {
            const active = item.to === "/" ? pathname === "/" : pathname.startsWith(item.to);
            const Icon = item.icon;
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex items-center gap-3 rounded-md px-3 py-2.5 text-sm transition-colors ${
                  active
                    ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
                    : "text-sidebar-foreground/80 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
                }`}
              >
                <Icon className="h-4 w-4" />
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="p-4 border-t border-sidebar-border">
          <div className="rounded-lg p-3 text-xs" style={{ background: "var(--gradient-subtle)" }}>
            <div className="flex items-center gap-1.5 font-medium text-foreground mb-1">
              <Circle className="h-2 w-2 fill-success text-success" />
              Everything's running
            </div>
            <div className="text-muted-foreground">Desktop runner & MCP server ready</div>
          </div>
        </div>
      </aside>

      <div className="flex-1 flex flex-col min-w-0">
        <main className="flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
