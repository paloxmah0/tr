import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { AnalyticsResp, Insight } from "../lib/api";
import { fmt, fmtPct, pnlColor } from "../lib/fmt";
import { RefreshCw, Lightbulb, TrendingUp, TrendingDown, Activity } from "lucide-react";

export default function Analytics() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";
  const [analytics, setAnalytics] = useState<AnalyticsResp | null>(null);
  const [insights, setInsights] = useState<Insight | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  async function load() {
    if (!accountId) return;
    setLoading(true); setError("");
    try {
      const [a, i] = await Promise.all([api.analytics(accountId), api.insights(accountId)]);
      setAnalytics(a); setInsights(i);
    } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, [accountId]);

  if (!accountId) return <div className="card text-center text-muted py-12">Select an account first.</div>;
  if (loading) return <p className="text-muted">Loading…</p>;
  if (error) return <div className="card border-bad/50 text-bad text-sm">{error}</div>;
  if (!analytics || !insights) return null;

  const s = analytics.summary;

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div><h2 className="text-xl font-bold text-white">Analytics & Insights</h2><p className="text-sm text-muted">Performance breakdown and AI-generated insights</p></div>
        <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Refresh</button>
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-4 gap-3 mb-6">
        <Card label="Total PnL" value={fmt(s.total_pnl)} color={pnlColor(s.total_pnl)} icon={s.total_pnl >= 0 ? TrendingUp : TrendingDown} />
        <Card label="Win Rate" value={fmtPct(s.win_rate)} icon={Activity} />
        <Card label="Total Trades" value={String(s.total_trades)} icon={Activity} />
        <Card label="Open Trades" value={String(s.open_trades)} icon={Activity} />
        <MiniCard label="Closed" value={String(s.closed_trades)} />
        <MiniCard label="Wins" value={String(s.winning_trades)} color="text-ok" />
        <MiniCard label="Losses" value={String(s.losing_trades)} color="text-bad" />
        <MiniCard label="Avg PnL" value={fmt(s.avg_pnl)} color={pnlColor(s.avg_pnl)} />
        <MiniCard label="Best Trade" value={s.best_trade ? fmt(s.best_trade) : "—"} color="text-ok" />
        <MiniCard label="Worst Trade" value={s.worst_trade ? fmt(s.worst_trade) : "—"} color="text-bad" />
        <MiniCard label="Open Exposure" value={fmt(insights.open_exposure)} />
        <MiniCard label="Recent Signals" value={String(insights.recent_signals)} />
      </div>

      {/* Insights */}
      <div className="card mb-6">
        <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2"><Lightbulb size={16} className="text-warn" /> Insights</h3>
        <ul className="space-y-2">
          {insights.notes.map((n, i) => (
            <li key={i} className="text-sm text-gray-300 flex items-start gap-2">
              <span className="text-accent mt-0.5">•</span> {n}
            </li>
          ))}
        </ul>
      </div>

      {/* Per-strategy performance */}
      <div className="card">
        <h3 className="text-sm font-semibold text-white mb-3">Per-Strategy Performance</h3>
        {analytics.per_strategy.length === 0 ? <p className="text-muted text-sm">No strategy data yet.</p> : (
          <table className="w-full text-sm">
            <thead><tr className="text-left text-muted border-b border-ink-700">
              <th className="py-2 px-2">Strategy</th><th className="px-2">Trades</th><th className="px-2">Win Rate</th><th className="px-2">Total PnL</th>
            </tr></thead>
            <tbody>
              {analytics.per_strategy.map(p => (
                <tr key={p.strategy_id} className="border-b border-ink-700/50">
                  <td className="py-2 px-2 text-gray-300 font-mono text-xs">{p.strategy_id.slice(0, 8)}…</td>
                  <td className="px-2 text-gray-400">{p.trades}</td>
                  <td className="px-2 text-gray-400">{fmtPct(p.win_rate)}</td>
                  <td className={`px-2 font-mono ${pnlColor(p.total_pnl)}`}>{fmt(p.total_pnl)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function Card({ label, value, color = "text-gray-200", icon: Icon }: any) {
  return <div className="card flex items-center justify-between"><div><div className="label">{label}</div><div className={`text-xl font-bold ${color}`}>{value}</div></div><Icon size={20} className="text-muted" /></div>;
}
function MiniCard({ label, value, color = "text-gray-200" }: { label: string; value: string; color?: string }) {
  return <div className="card"><div className="label">{label}</div><div className={`text-base font-semibold ${color}`}>{value}</div></div>;
}
