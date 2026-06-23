import { NavLink, Outlet, useSearchParams } from "react-router-dom";
import { LayoutDashboard, FileCode, StickyNote, FlaskConical, Activity, BarChart3, Settings as SettingsIcon } from "lucide-react";

const nav = [
  { to: "/", label: "Accounts", icon: LayoutDashboard, end: true },
  { to: "/strategies", label: "Strategies", icon: FileCode },
  { to: "/notes", label: "Notes", icon: StickyNote },
  { to: "/backtest", label: "Backtest", icon: FlaskConical },
  { to: "/activity", label: "Activity", icon: Activity },
  { to: "/analytics", label: "Analytics", icon: BarChart3 },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
];

export default function Layout() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";

  return (
    <div className="flex min-h-screen">
      <aside className="w-56 shrink-0 bg-ink-900 border-r border-ink-700 flex flex-col">
        <div className="px-5 py-4 border-b border-ink-700">
          <h1 className="text-lg font-bold text-white">Trading</h1>
          <p className="text-xs text-muted">AI Strategy Engine</p>
        </div>
        <nav className="flex-1 py-2">
          {nav.map((n) => (
            <NavLink
              key={n.to}
              to={accountId && n.to !== "/" ? `${n.to}?account=${accountId}` : n.to}
              end={n.end}
              className={({ isActive }) =>
                `flex items-center gap-3 px-5 py-2.5 text-sm transition-colors ${
                  isActive ? "bg-ink-800 text-accent border-r-2 border-accent" : "text-gray-400 hover:text-gray-200 hover:bg-ink-850"
                }`
              }
            >
              <n.icon size={17} />
              {n.label}
            </NavLink>
          ))}
        </nav>
        <div className="px-5 py-3 border-t border-ink-700 text-xs text-muted">
          {accountId ? `Account: ${accountId.slice(0, 8)}…` : "No account selected"}
        </div>
      </aside>
      <main className="flex-1 overflow-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}
