import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { StrategyWithRules, BacktestResult } from "../lib/api";
import { fmt, fmtPct, fmtPctRaw, pnlColor } from "../lib/fmt";
import { Loader2, Play } from "lucide-react";
import { LineChart, Line, XAxis, YAxis, ResponsiveContainer, Tooltip, CartesianGrid, ReferenceLine } from "recharts";

export default function Backtest() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";
  const [strategies, setStrategies] = useState<StrategyWithRules[]>([]);
  const [selected, setSelected] = useState("");
  const [form, setForm] = useState({ symbol: "", candles: "1000" });
  const [result, setResult] = useState<BacktestResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!accountId) return;
    api.listStrategies(accountId).then(setStrategies).catch(e => setError(e.message));
  }, [accountId]);

  async function run() {
    if (!selected || !form.symbol) return;
    setLoading(true); setError(""); setResult(null);
    try {
      setResult(await api.backtest(selected, { symbol: form.symbol, candles: Number(form.candles) || 1000 }));
    } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }

  // auto-fill symbol from strategy
  useEffect(() => {
    if (selected) {
      const s = strategies.find(x => x.id === selected);
      if (s && s.symbols[0] && !form.symbol) setForm(f => ({ ...f, symbol: s.symbols[0] }));
    }
  }, [selected]);

  if (!accountId) return <div className="card text-center text-muted py-12">Select an account first.</div>;

  return (
    <div>
      <div className="mb-6"><h2 className="text-xl font-bold text-white">Backtest</h2><p className="text-sm text-muted">Replay historical candles through a strategy's rules</p></div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}

      <div className="card mb-6">
        <div className="grid grid-cols-3 gap-3">
          <div>
            <div className="label mb-1">Strategy</div>
            <select className="input w-full" value={selected} onChange={e => setSelected(e.target.value)}>
              <option value="">Select…</option>
              {strategies.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
          <div><div className="label mb-1">Symbol</div><input className="input w-full" value={form.symbol} onChange={e => setForm({ ...form, symbol: e.target.value })} placeholder="frxEURUSD" /></div>
          <div><div className="label mb-1">Candles</div><input className="input w-full" type="number" value={form.candles} onChange={e => setForm({ ...form, candles: e.target.value })} /></div>
        </div>
        <button onClick={run} disabled={loading || !selected || !form.symbol} className="btn-primary mt-3">
          {loading ? <><Loader2 size={15} className="inline mr-1 animate-spin" />Running…</> : <><Play size={15} className="inline mr-1" />Run Backtest</>}
        </button>
      </div>

      {result && (
        <>
          <div className="grid grid-cols-4 gap-3 mb-4">
            <Stat label="Final Equity" value={fmt(result.final_equity)} color={pnlColor(result.final_equity - result.initial_balance)} />
            <Stat label="Total Return" value={fmtPctRaw(result.total_return_pct)} color={pnlColor(result.total_return_pct)} />
            <Stat label="Win Rate" value={fmtPct(result.win_rate)} />
            <Stat label="Max Drawdown" value={fmtPctRaw(result.max_drawdown_pct)} color="text-bad" />
            <Stat label="Trades" value={String(result.closed_trades)} />
            <Stat label="Wins / Losses" value={`${result.winning_trades} / ${result.losing_trades}`} />
            <Stat label="Avg PnL" value={fmt(result.avg_pnl)} color={pnlColor(result.avg_pnl)} />
            <Stat label="Sharpe" value={fmt(result.sharpe_ratio)} />
          </div>

          <div className="card mb-4">
            <h3 className="text-sm font-semibold text-white mb-3">Equity Curve</h3>
            <ResponsiveContainer width="100%" height={280}>
              <LineChart data={result.equity_curve}>
                <CartesianGrid strokeDasharray="3 3" stroke="#21262d" />
                <XAxis dataKey="ts" tickFormatter={(t) => new Date(t).toLocaleDateString()} stroke="#6e7681" fontSize={11} />
                <YAxis stroke="#6e7681" fontSize={11} domain={["auto", "auto"]} />
                <Tooltip contentStyle={{ background: "#161b22", border: "1px solid #21262d", borderRadius: 6 }} labelFormatter={t => new Date(t as string).toLocaleString()} />
                <ReferenceLine y={result.initial_balance} stroke="#6e7681" strokeDasharray="4 4" />
                <Line type="monotone" dataKey="equity" stroke="#58a6ff" strokeWidth={2} dot={false} />
              </LineChart>
            </ResponsiveContainer>
          </div>

          <div className="card">
            <h3 className="text-sm font-semibold text-white mb-3">Trades ({result.trades.length})</h3>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead><tr className="text-left text-muted border-b border-ink-700">
                  <th className="py-2 px-2">Side</th><th className="px-2">Entry</th><th className="px-2">Exit</th>
                  <th className="px-2">PnL</th><th className="px-2">Reason</th><th className="px-2">Strength</th>
                </tr></thead>
                <tbody>
                  {result.trades.slice(0, 50).map((t, i) => (
                    <tr key={i} className="border-b border-ink-700/50">
                      <td className="py-2 px-2"><span className={`badge ${t.side === "buy" ? "bg-ok/20 text-ok" : "bg-bad/20 text-bad"}`}>{t.side}</span></td>
                      <td className="px-2 text-gray-400">{fmt(t.entry_price, 5)}</td>
                      <td className="px-2 text-gray-400">{fmt(t.exit_price, 5)}</td>
                      <td className={`px-2 font-mono ${pnlColor(t.pnl)}`}>{fmt(t.pnl)}</td>
                      <td className="px-2 text-muted text-xs">{t.exit_reason}</td>
                      <td className="px-2 text-muted">{fmtPct(t.strength)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function Stat({ label, value, color = "text-gray-200" }: { label: string; value: string; color?: string }) {
  return <div className="card"><div className="label">{label}</div><div className={`text-lg font-bold ${color}`}>{value}</div></div>;
}
