import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { Signal, Trade } from "../lib/api";
import { fmt, fmtDate, fmtPct, pnlColor } from "../lib/fmt";
import { RefreshCw, X } from "lucide-react";

export default function SignalsTrades() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";
  const [signals, setSignals] = useState<Signal[]>([]);
  const [trades, setTrades] = useState<Trade[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [closing, setClosing] = useState<string | null>(null);
  const [closePrice, setClosePrice] = useState("");

  async function load() {
    if (!accountId) return;
    setLoading(true); setError("");
    try {
      const [s, t] = await Promise.all([api.listSignals(accountId), api.listTrades(accountId)]);
      setSignals(s); setTrades(t);
    } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, [accountId]);

  async function closeTrade(id: string) {
    try { await api.closeTrade(id, Number(closePrice)); setClosing(null); setClosePrice(""); load(); }
    catch (e: any) { setError(e.message); }
  }

  if (!accountId) return <div className="card text-center text-muted py-12">Select an account first.</div>;

  const open = trades.filter(t => t.status === "open");
  const closed = trades.filter(t => t.status === "closed");

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div><h2 className="text-xl font-bold text-white">Signals & Trades</h2><p className="text-sm text-muted">Live signal feed and position management</p></div>
        <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Refresh</button>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}

      {loading ? <p className="text-muted">Loading…</p> : (
        <div className="space-y-6">
          {/* Open trades */}
          <div>
            <h3 className="text-sm font-semibold text-white mb-2">Open Positions ({open.length})</h3>
            {open.length === 0 ? <p className="text-muted text-sm">No open positions.</p> : (
              <div className="card overflow-x-auto">
                <table className="w-full text-sm">
                  <thead><tr className="text-left text-muted border-b border-ink-700">
                    <th className="py-2 px-2">Symbol</th><th className="px-2">Side</th><th className="px-2">Size</th>
                    <th className="px-2">Entry</th><th className="px-2">SL</th><th className="px-2">TP</th>
                    <th className="px-2">Mode</th><th className="px-2">Opened</th><th className="px-2">Action</th>
                  </tr></thead>
                  <tbody>
                    {open.map(t => (
                      <tr key={t.id} className="border-b border-ink-700/50">
                        <td className="py-2 px-2 font-medium text-white">{t.symbol}</td>
                        <td className="px-2"><span className={`badge ${t.side === "buy" ? "bg-ok/20 text-ok" : "bg-bad/20 text-bad"}`}>{t.side}</span></td>
                        <td className="px-2 text-gray-400">{fmt(t.size)}</td>
                        <td className="px-2 text-gray-400">{fmt(t.entry_price, 5)}</td>
                        <td className="px-2 text-gray-400">{t.stop_loss ? fmt(t.stop_loss, 5) : "—"}</td>
                        <td className="px-2 text-gray-400">{t.take_profit ? fmt(t.take_profit, 5) : "—"}</td>
                        <td className="px-2"><span className="badge bg-ink-700 text-muted">{t.mode}</span></td>
                        <td className="px-2 text-muted text-xs">{fmtDate(t.opened_at)}</td>
                        <td className="px-2">
                          {closing === t.id ? (
                            <span className="flex items-center gap-1">
                              <input className="input w-24 text-xs" type="number" placeholder="Exit price" value={closePrice} onChange={e => setClosePrice(e.target.value)} />
                              <button className="btn-danger text-xs" onClick={() => closeTrade(t.id)}>Close</button>
                              <button className="btn-ghost text-xs" onClick={() => setClosing(null)}><X size={12} /></button>
                            </span>
                          ) : (
                            <button className="btn-ghost text-xs" onClick={() => { setClosing(t.id); setClosePrice(String(t.entry_price)); }}>Close</button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>

          {/* Signals feed */}
          <div>
            <h3 className="text-sm font-semibold text-white mb-2">Recent Signals</h3>
            {signals.length === 0 ? <p className="text-muted text-sm">No signals yet.</p> : (
              <div className="space-y-1">
                {signals.slice(0, 20).map(s => (
                  <div key={s.id} className="card flex items-center gap-3 py-3">
                    <span className={`badge ${s.side === "buy" ? "bg-ok/20 text-ok" : "bg-bad/20 text-bad"}`}>{s.side}</span>
                    <span className="font-medium text-white text-sm">{s.symbol}</span>
                    <span className="text-gray-400 text-sm">@ {fmt(s.price, 5)}</span>
                    <span className="text-muted text-xs">strength {fmtPct(s.strength)}</span>
                    <span className="text-muted text-xs flex-1 truncate">{s.rationale}</span>
                    <span className="text-muted text-xs">{fmtDate(s.created_at)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Closed trades */}
          <div>
            <h3 className="text-sm font-semibold text-white mb-2">Closed Trades ({closed.length})</h3>
            {closed.length === 0 ? <p className="text-muted text-sm">No closed trades.</p> : (
              <div className="card overflow-x-auto">
                <table className="w-full text-sm">
                  <thead><tr className="text-left text-muted border-b border-ink-700">
                    <th className="py-2 px-2">Symbol</th><th className="px-2">Side</th><th className="px-2">Entry</th>
                    <th className="px-2">Exit</th><th className="px-2">PnL</th><th className="px-2">Closed</th>
                  </tr></thead>
                  <tbody>
                    {closed.slice(0, 50).map(t => (
                      <tr key={t.id} className="border-b border-ink-700/50">
                        <td className="py-2 px-2 font-medium text-white">{t.symbol}</td>
                        <td className="px-2"><span className={`badge ${t.side === "buy" ? "bg-ok/20 text-ok" : "bg-bad/20 text-bad"}`}>{t.side}</span></td>
                        <td className="px-2 text-gray-400">{fmt(t.entry_price, 5)}</td>
                        <td className="px-2 text-gray-400">{t.exit_price ? fmt(t.exit_price, 5) : "—"}</td>
                        <td className={`px-2 font-mono ${pnlColor(t.pnl)}`}>{t.pnl ? fmt(t.pnl) : "—"}</td>
                        <td className="px-2 text-muted text-xs">{fmtDate(t.closed_at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
