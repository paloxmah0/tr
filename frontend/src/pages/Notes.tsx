import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { Note } from "../lib/api";
import { fmtDate } from "../lib/fmt";
import { Plus, Sparkles, RefreshCw, Loader2 } from "lucide-react";

const statusColors: Record<string, string> = {
  pending: "bg-warn/20 text-warn", extracted: "bg-ok/20 text-ok", failed: "bg-bad/20 text-bad",
};

export default function Notes() {
  const [params] = useSearchParams();
  const accountId = params.get("account") || "";
  const [notes, setNotes] = useState<Note[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState({ title: "", content: "", content_type: "markdown" });
  const [processing, setProcessing] = useState<string | null>(null);

  async function load() {
    if (!accountId) return;
    setLoading(true); setError("");
    try { setNotes(await api.listNotes(accountId)); } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, [accountId]);

  async function create() {
    try {
      await api.createNote(accountId, form);
      setForm({ title: "", content: "", content_type: "markdown" });
      setShowForm(false); load();
    } catch (e: any) { setError(e.message); }
  }

  async function process(id: string) {
    setProcessing(id); setError("");
    try {
      const r = await api.processNote(id);
      if (r.error) setError(`Extraction error: ${r.error}`);
      load();
    } catch (e: any) { setError(e.message); } finally { setProcessing(null); }
  }

  if (!accountId) return <div className="card text-center text-muted py-12">Select an account first.</div>;

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div><h2 className="text-xl font-bold text-white">Notes → Strategy Extraction</h2><p className="text-sm text-muted">Upload trading notes; the LLM extracts executable strategies</p></div>
        <div className="flex gap-2">
          <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Refresh</button>
          <button onClick={() => setShowForm(!showForm)} className="btn-primary"><Plus size={15} className="inline mr-1" />New Note</button>
        </div>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}

      {showForm && (
        <div className="card mb-6 space-y-3">
          <div><div className="label mb-1">Title</div><input className="input w-full" value={form.title} onChange={e => setForm({ ...form, title: e.target.value })} /></div>
          <div><div className="label mb-1">Content (markdown / plain text)</div><textarea className="input w-full h-40 font-mono text-xs" value={form.content} onChange={e => setForm({ ...form, content: e.target.value })} placeholder="Buy EUR/USD when RSI(14) falls below 30 and price is above the 50 EMA. Stop 30 pips, target 60 pips." /></div>
          <button onClick={create} className="btn-primary">Save Note</button>
        </div>
      )}

      {loading ? <p className="text-muted">Loading…</p> : notes.length === 0 ? (
        <div className="card text-center text-muted py-12">No notes yet. Upload one to extract a strategy.</div>
      ) : (
        <div className="space-y-2">
          {notes.map(n => (
            <div key={n.id} className="card">
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="font-semibold text-white">{n.title}</span>
                    <span className={`badge ${statusColors[n.status]}`}>{n.status}</span>
                    <span className="text-xs text-muted">{fmtDate(n.created_at)}</span>
                  </div>
                  <p className="text-sm text-gray-400 line-clamp-2">{n.content.slice(0, 200)}{n.content.length > 200 ? "…" : ""}</p>
                  {n.error && <p className="text-xs text-bad mt-1">{n.error}</p>}
                </div>
                <button onClick={() => process(n.id)} disabled={processing === n.id} className="btn-primary shrink-0">
                  {processing === n.id ? <><Loader2 size={14} className="inline mr-1 animate-spin" />Processing…</> : <><Sparkles size={14} className="inline mr-1" />Extract</>}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
