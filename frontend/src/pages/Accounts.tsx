import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { Account, TradingMode } from "../lib/api";
import { fmt } from "../lib/fmt";
import { Plus, RefreshCw } from "lucide-react";

const modeColors: Record<TradingMode, string> = {
  paper: "bg-warn/20 text-warn",
  signals: "bg-accent/20 text-accent",
  live: "bg-bad/20 text-bad",
};

export default function Accounts() {
  const [params, setParams] = useSearchParams();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState({ label: "", broker: "", account_ref: "", balance: "10000", currency: "USD" });

  const selectedId = params.get("account");

  async function load() {
    setLoading(true); setError("");
    try { setAccounts(await api.listAccounts()); } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, []);

  async function create() {
    try {
      const acc = await api.createAccount({ ...form, balance: Number(form.balance) || 0 });
      setForm({ label: "", broker: "", account_ref: "", balance: "10000", currency: "USD" });
      setShowForm(false);
      setParams({ account: acc.id });
      load();
    } catch (e: any) { setError(e.message); }
  }

  async function changeMode(id: string, mode: TradingMode) {
    try { await api.setMode(id, mode); load(); } catch (e: any) { setError(e.message); }
  }

  function select(id: string) { setParams({ account: id }); }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className="text-xl font-bold text-white">Accounts</h2>
          <p className="text-sm text-muted">Select an account to use across the dashboard</p>
        </div>
        <div className="flex gap-2">
          <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Refresh</button>
          <button onClick={() => setShowForm(!showForm)} className="btn-primary"><Plus size={15} className="inline mr-1" />New Account</button>
        </div>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}

      {showForm && (
        <div className="card mb-6 grid grid-cols-2 gap-3">
          <div><div className="label mb-1">Label</div><input className="input w-full" value={form.label} onChange={e => setForm({ ...form, label: e.target.value })} /></div>
          <div><div className="label mb-1">Broker</div><input className="input w-full" value={form.broker} onChange={e => setForm({ ...form, broker: e.target.value })} placeholder="oanda / deriv" /></div>
          <div><div className="label mb-1">Account Ref</div><input className="input w-full" value={form.account_ref} onChange={e => setForm({ ...form, account_ref: e.target.value })} /></div>
          <div><div className="label mb-1">Balance</div><input className="input w-full" type="number" value={form.balance} onChange={e => setForm({ ...form, balance: e.target.value })} /></div>
          <div className="col-span-2"><button onClick={create} className="btn-primary">Create</button></div>
        </div>
      )}

      {loading ? <p className="text-muted">Loading…</p> : accounts.length === 0 ? (
        <div className="card text-center text-muted py-12">No accounts yet. Create one to get started.</div>
      ) : (
        <div className="grid gap-3">
          {accounts.map(a => (
            <div key={a.id} className={`card cursor-pointer transition-all ${selectedId === a.id ? "border-accent ring-1 ring-accent/30" : "hover:border-ink-600"}`} onClick={() => select(a.id)}>
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-semibold text-white">{a.label}</span>
                    <span className={`badge ${modeColors[a.mode]}`}>{a.mode}</span>
                  </div>
                  <div className="text-xs text-muted mt-0.5">{a.broker} · {a.account_ref} · {fmt(a.balance, 2)} {a.currency}</div>
                </div>
                <div className="flex gap-1" onClick={e => e.stopPropagation()}>
                  {(["paper", "signals", "live"] as TradingMode[]).map(m => (
                    <button key={m} onClick={() => changeMode(a.id, m)} className={`btn text-xs ${a.mode === m ? "bg-accent-dim text-white" : "bg-ink-700 text-gray-400 hover:bg-ink-600"}`}>{m}</button>
                  ))}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
