import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { StrategyWithRules, AssetClass } from "../lib/api";
import { Plus, Trash2, RefreshCw, ChevronDown, ChevronRight } from "lucide-react";

export default function Strategies() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";
  const [strategies, setStrategies] = useState<StrategyWithRules[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState({
    name: "", description: "", asset_class: "forex" as AssetClass,
    symbols: "", stop_loss: "", take_profit: "", risk_per_trade: "0.01",
    rules: [{ name: "", expr: "", weight: "1" }],
  });

  async function load() {
    if (!accountId) return;
    setLoading(true); setError("");
    try { setStrategies(await api.listStrategies(accountId)); } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, [accountId]);

  async function create() {
    try {
      await api.createStrategy(accountId, {
        name: form.name,
        description: form.description || null,
        asset_class: form.asset_class,
        symbols: form.symbols.split(",").map(s => s.trim()).filter(Boolean),
        stop_loss: form.stop_loss ? Number(form.stop_loss) : null,
        take_profit: form.take_profit ? Number(form.take_profit) : null,
        risk_per_trade: Number(form.risk_per_trade) || 0.01,
        rules: form.rules.map(r => ({ name: r.name, expr: r.expr, weight: Number(r.weight) || 1 })),
      });
      setForm({ name: "", description: "", asset_class: "forex", symbols: "", stop_loss: "", take_profit: "", risk_per_trade: "0.01", rules: [{ name: "", expr: "", weight: "1" }] });
      setShowForm(false); load();
    } catch (e: any) { setError(e.message); }
  }

  async function remove(id: string) {
    if (!confirm("Delete this strategy?")) return;
    try { await api.deleteStrategy(id); load(); } catch (e: any) { setError(e.message); }
  }

  async function toggle(id: string, enabled: boolean) {
    try { await api.updateStrategy(id, { enabled: !enabled }); load(); } catch (e: any) { setError(e.message); }
  }

  if (!accountId) return <NoAccount />;

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div><h2 className="text-xl font-bold text-white">Strategies</h2><p className="text-sm text-muted">Author rule-based strategies in the rule DSL</p></div>
        <div className="flex gap-2">
          <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Refresh</button>
          <button onClick={() => setShowForm(!showForm)} className="btn-primary"><Plus size={15} className="inline mr-1" />New Strategy</button>
        </div>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}

      {showForm && (
        <div className="card mb-6 space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div><div className="label mb-1">Name</div><input className="input w-full" value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} /></div>
            <div><div className="label mb-1">Asset Class</div><select className="input w-full" value={form.asset_class} onChange={e => setForm({ ...form, asset_class: e.target.value as AssetClass })}><option value="forex">Forex</option><option value="derivindex">Derivative Index</option></select></div>
            <div><div className="label mb-1">Symbols (comma-separated)</div><input className="input w-full" value={form.symbols} onChange={e => setForm({ ...form, symbols: e.target.value })} placeholder="EUR/USD, GBP/USD" /></div>
            <div><div className="label mb-1">Risk per trade</div><input className="input w-full" type="number" step="0.01" value={form.risk_per_trade} onChange={e => setForm({ ...form, risk_per_trade: e.target.value })} /></div>
            <div><div className="label mb-1">Stop loss (pips/pts)</div><input className="input w-full" type="number" value={form.stop_loss} onChange={e => setForm({ ...form, stop_loss: e.target.value })} /></div>
            <div><div className="label mb-1">Take profit (pips/pts)</div><input className="input w-full" type="number" value={form.take_profit} onChange={e => setForm({ ...form, take_profit: e.target.value })} /></div>
          </div>
          <div><div className="label mb-1">Description</div><input className="input w-full" value={form.description} onChange={e => setForm({ ...form, description: e.target.value })} /></div>
          <div>
            <div className="label mb-2">Rules (DSL)</div>
            {form.rules.map((r, i) => (
              <div key={i} className="grid grid-cols-12 gap-2 mb-2">
                <input className="input col-span-3" placeholder="Rule name" value={r.name} onChange={e => { const rules = [...form.rules]; rules[i] = { ...r, name: e.target.value }; setForm({ ...form, rules }); }} />
                <input className="input col-span-7" placeholder="rsi(14) < 30 and price > ema(50)" value={r.expr} onChange={e => { const rules = [...form.rules]; rules[i] = { ...r, expr: e.target.value }; setForm({ ...form, rules }); }} />
                <input className="input col-span-1" type="number" placeholder="w" value={r.weight} onChange={e => { const rules = [...form.rules]; rules[i] = { ...r, weight: e.target.value }; setForm({ ...form, rules }); }} />
                <button className="btn-danger col-span-1" onClick={() => setForm({ ...form, rules: form.rules.filter((_, j) => j !== i) })}><Trash2 size={14} /></button>
              </div>
            ))}
            <button className="btn-ghost text-xs" onClick={() => setForm({ ...form, rules: [...form.rules, { name: "", expr: "", weight: "1" }] })}><Plus size={13} className="inline mr-1" />Add rule</button>
          </div>
          <button onClick={create} className="btn-primary">Create Strategy</button>
        </div>
      )}

      {loading ? <p className="text-muted">Loading…</p> : strategies.length === 0 ? (
        <div className="card text-center text-muted py-12">No strategies yet. Create one manually or ingest from Notes.</div>
      ) : (
        <div className="space-y-2">
          {strategies.map(s => (
            <div key={s.id} className="card">
              <div className="flex items-center justify-between cursor-pointer" onClick={() => setExpanded(expanded === s.id ? null : s.id)}>
                <div className="flex items-center gap-2">
                  {expanded === s.id ? <ChevronDown size={16} className="text-muted" /> : <ChevronRight size={16} className="text-muted" />}
                  <span className="font-semibold text-white">{s.name}</span>
                  <span className="badge bg-ink-700 text-muted">{s.asset_class}</span>
                  <span className="badge bg-accent/20 text-accent">{s.source}</span>
                  <span className="text-xs text-muted">{s.symbols.join(", ")}</span>
                </div>
                <div className="flex items-center gap-2" onClick={e => e.stopPropagation()}>
                  <button className={`btn text-xs ${s.enabled ? "bg-ok/20 text-ok" : "bg-ink-700 text-muted"}`} onClick={() => toggle(s.id, s.enabled)}>{s.enabled ? "Enabled" : "Disabled"}</button>
                  <button className="btn-danger" onClick={() => remove(s.id)}><Trash2 size={14} /></button>
                </div>
              </div>
              {expanded === s.id && (
                <div className="mt-4 pt-4 border-t border-ink-700 space-y-2">
                  {s.description && <p className="text-sm text-muted">{s.description}</p>}
                  <div className="grid grid-cols-4 gap-3 text-sm mb-3">
                    <div><span className="label">SL</span> <span className="text-gray-300">{s.stop_loss ?? "—"}</span></div>
                    <div><span className="label">TP</span> <span className="text-gray-300">{s.take_profit ?? "—"}</span></div>
                    <div><span className="label">Risk</span> <span className="text-gray-300">{(s.risk_per_trade * 100).toFixed(1)}%</span></div>
                    <div><span className="label">Rules</span> <span className="text-gray-300">{s.rules.length}</span></div>
                  </div>
                  <div className="space-y-1">
                    {s.rules.map(r => (
                      <div key={r.id} className="flex items-center gap-2 bg-ink-900 rounded px-3 py-2">
                        <span className="text-xs font-mono text-accent w-32 truncate">{r.name}</span>
                        <code className="text-xs text-gray-400 flex-1 truncate">{r.expr}</code>
                        <span className="text-xs text-muted">w:{r.weight}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function NoAccount() {
  return <div className="card text-center text-muted py-12">Select an account first from the Accounts page.</div>;
}
