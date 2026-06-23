import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Save, Loader2, CheckCircle2, XCircle, KeyRound, RefreshCw } from "lucide-react";

interface SettingsData {
  values: Record<string, string>;
  masked: Record<string, string>;
  is_set: Record<string, boolean>;
}

const FIELDS = [
  { section: "LLM (Strategy Extraction)", items: [
    { key: "llm_base_url", label: "Base URL", placeholder: "https://api.openai.com/v1", type: "text" },
    { key: "llm_api_key", label: "API Key", placeholder: "sk-...", type: "password" },
    { key: "llm_model", label: "Model", placeholder: "gpt-4o-mini", type: "text" },
  ]},
  { section: "Deriv (Derivative Indices)", items: [
    { key: "deriv_app_id", label: "App ID", placeholder: "1089", type: "text" },
    { key: "deriv_api_token", label: "API Token", placeholder: "Deriv API token", type: "password" },
    { key: "deriv_account_id", label: "Account ID", placeholder: "CR123456", type: "text" },
  ]},
  { section: "OANDA (Forex Spot)", items: [
    { key: "oanda_base_url", label: "Base URL", placeholder: "https://api-fxpractice.oanda.com", type: "text" },
    { key: "oanda_api_token", label: "API Token", placeholder: "OANDA API token", type: "password" },
    { key: "oanda_account_id", label: "Account ID", placeholder: "001-001-0000-001", type: "text" },
  ]},
];

export default function Settings() {
  const [data, setData] = useState<SettingsData | null>(null);
  const [form, setForm] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<Record<string, { ok: boolean; message: string }>>({});

  async function load() {
    setLoading(true); setError("");
    try {
      const d = await api.getSettings();
      setData(d);
      // Pre-fill form with masked values for display; user overwrites to change.
      setForm({ ...d.masked });
      setSaved(false);
    } catch (e: any) { setError(e.message); } finally { setLoading(false); }
  }
  useEffect(() => { load(); }, []);

  async function save() {
    setSaving(true); setError(""); setSaved(false);
    try {
      await api.updateSettings(form);
      setSaved(true);
      load();
    } catch (e: any) { setError(e.message); } finally { setSaving(false); }
  }

  async function test(service: string) {
    setTesting(service);
    try {
      const r = await api.testService(service);
      setTestResult(prev => ({ ...prev, [service]: r }));
    } catch (e: any) {
      setTestResult(prev => ({ ...prev, [service]: { ok: false, message: e.message } }));
    } finally { setTesting(null); }
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className="text-xl font-bold text-white flex items-center gap-2"><KeyRound size={20} /> Settings</h2>
          <p className="text-sm text-muted">Configure API tokens — changes take effect immediately, no restart needed</p>
        </div>
        <button onClick={load} className="btn-ghost"><RefreshCw size={15} className="inline mr-1" />Reload</button>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}
      {saved && <div className="card border-ok/50 text-ok text-sm mb-4 flex items-center gap-2"><CheckCircle2 size={16} /> Settings saved successfully.</div>}

      {loading ? <p className="text-muted">Loading…</p> : data && (
        <div className="space-y-6 max-w-2xl">
          {FIELDS.map(group => (
            <div key={group.section} className="card">
              <h3 className="text-sm font-semibold text-white mb-4">{group.section}</h3>
              <div className="space-y-3">
                {group.items.map(item => (
                  <div key={item.key}>
                    <div className="flex items-center justify-between mb-1">
                      <label className="label">{item.label}</label>
                      {data.is_set[item.key] && <span className="text-xs text-ok">✓ set</span>}
                    </div>
                    <input
                      className="input w-full"
                      type={item.type}
                      value={form[item.key] || ""}
                      placeholder={item.placeholder}
                      onChange={e => { setForm({ ...form, [item.key]: e.target.value }); setSaved(false); }}
                    />
                  </div>
                ))}
              </div>
              {/* Test button per section */}
              {group.section.startsWith("LLM") && (
                <button onClick={() => test("llm")} disabled={testing === "llm"} className="btn-ghost mt-3 text-xs">
                  {testing === "llm" ? <Loader2 size={13} className="inline mr-1 animate-spin" /> : null}
                  Test LLM
                </button>
              )}
              {group.section.startsWith("Deriv") && (
                <button onClick={() => test("deriv")} disabled={testing === "deriv"} className="btn-ghost mt-3 text-xs">
                  {testing === "deriv" ? <Loader2 size={13} className="inline mr-1 animate-spin" /> : null}
                  Test Deriv
                </button>
              )}
              {group.section.startsWith("OANDA") && (
                <button onClick={() => test("oanda")} disabled={testing === "oanda"} className="btn-ghost mt-3 text-xs">
                  {testing === "oanda" ? <Loader2 size={13} className="inline mr-1 animate-spin" /> : null}
                  Test OANDA
                </button>
              )}
              {testResult[group.section.startsWith("LLM") ? "llm" : group.section.startsWith("Deriv") ? "deriv" : "oanda"] && (
                <div className={`mt-2 text-xs flex items-center gap-1 ${testResult[group.section.startsWith("LLM") ? "llm" : group.section.startsWith("Deriv") ? "deriv" : "oanda"].ok ? "text-ok" : "text-bad"}`}>
                  {testResult[group.section.startsWith("LLM") ? "llm" : group.section.startsWith("Deriv") ? "deriv" : "oanda"].ok ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                  {testResult[group.section.startsWith("LLM") ? "llm" : group.section.startsWith("Deriv") ? "deriv" : "oanda"].message}
                </div>
              )}
            </div>
          ))}

          <button onClick={save} disabled={saving} className="btn-primary w-full">
            {saving ? <><Loader2 size={15} className="inline mr-1 animate-spin" />Saving…</> : <><Save size={15} className="inline mr-1" />Save All Settings</>}
          </button>
        </div>
      )}
    </div>
  );
}
