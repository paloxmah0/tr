import { useState } from "react";
import { api } from "../lib/api";
import type { Prediction, Evidence } from "../lib/api";
import { fmt, fmtPct } from "../lib/fmt";
import { Brain, Loader2, TrendingUp, TrendingDown, Minus, Zap, Clock, Globe, Target, Activity, BarChart3, Eye, Layers } from "lucide-react";

const MARKETS = [
  { symbol: "R_100", label: "Volatility 100 Index", class: "derivindex" },
  { symbol: "R_75", label: "Volatility 75 Index", class: "derivindex" },
  { symbol: "R_50", label: "Volatility 50 Index", class: "derivindex" },
  { symbol: "R_25", label: "Volatility 25 Index", class: "derivindex" },
  { symbol: "frxEURUSD", label: "EUR/USD", class: "forex" },
  { symbol: "frxGBPUSD", label: "GBP/USD", class: "forex" },
  { symbol: "frxUSDJPY", label: "USD/JPY", class: "forex" },
  { symbol: "frxAUDUSD", label: "AUD/USD", class: "forex" },
];

const TIMEFRAMES = [
  { mins: 1, label: "1 min" }, { mins: 5, label: "5 min" }, { mins: 10, label: "10 min" },
  { mins: 15, label: "15 min" }, { mins: 30, label: "30 min" }, { mins: 60, label: "1 hour" },
];

function fmtUTC(s: string): string {
  if (!s) return "—";
  return new Date(s).toISOString().replace("T", " ").slice(0, 19) + " UTC";
}

const STATE_LABELS: Record<string, string> = {
  trending_up: "Trending Up",
  trending_down: "Trending Down",
  ranging: "Ranging",
  reversing_up: "Reversing Up",
  reversing_down: "Reversing Down",
  squeeze: "Volatility Squeeze",
  mixed: "Mixed Signals",
};

export default function AITrade() {
  const [symbol, setSymbol] = useState("R_100");
  const [timeframe, setTimeframe] = useState(15);
  const [analyzing, setAnalyzing] = useState(false);
  const [prediction, setPrediction] = useState<Prediction | null>(null);
  const [trading, setTrading] = useState(false);
  const [tradeResult, setTradeResult] = useState<string | null>(null);
  const [error, setError] = useState("");

  async function runAnalysis() {
    setAnalyzing(true); setError(""); setPrediction(null); setTradeResult(null);
    const m = MARKETS.find(m => m.symbol === symbol);
    try { setPrediction(await api.analyze(symbol, timeframe, m?.class)); }
    catch (e: any) { setError(e.message); } finally { setAnalyzing(false); }
  }

  async function placeTrade() {
    if (!prediction || prediction.direction === "wait") return;
    setTrading(true); setError("");
    try {
      const m = MARKETS.find(m => m.symbol === symbol);
      const r = await api.placeTrade(symbol, prediction.direction, timeframe, undefined, m?.class);
      setTradeResult(r.message || "Trade placed!");
    } catch (e: any) { setError(e.message); } finally { setTrading(false); }
  }

  const DirIcon = prediction?.direction === "buy" ? TrendingUp : prediction?.direction === "sell" ? TrendingDown : Minus;
  const dirColor = prediction?.direction === "buy" ? "text-ok" : prediction?.direction === "sell" ? "text-bad" : "text-muted";
  const dirBg = prediction?.direction === "buy" ? "bg-ok/10 border-ok/30" : prediction?.direction === "sell" ? "bg-bad/10 border-bad/30" : "bg-ink-800 border-ink-700";

  const bullCount = prediction?.evidence.filter(e => e.confirms === "buy" && e.weight > 0).length || 0;
  const bearCount = prediction?.evidence.filter(e => e.confirms === "sell" && e.weight > 0).length || 0;

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-xl font-bold text-white flex items-center gap-2"><Brain size={22} className="text-accent" /> AI Market Reader</h2>
        <p className="text-sm text-muted">Reads the current market state with tools + candlestick knowledge. Evidence-based, not prediction.</p>
      </div>

      {error && <div className="card border-bad/50 text-bad text-sm mb-4">{error}</div>}
      {tradeResult && <div className="card border-ok/50 text-ok text-sm mb-4">{tradeResult}</div>}

      <div className="card mb-6">
        <div className="grid grid-cols-2 gap-4">
          <div>
            <div className="label mb-2"><Globe size={12} className="inline mr-1" />Market</div>
            <select className="input w-full" value={symbol} onChange={e => { setSymbol(e.target.value); setPrediction(null); }}>
              {MARKETS.map(m => <option key={m.symbol} value={m.symbol}>{m.label}</option>)}
            </select>
          </div>
          <div>
            <div className="label mb-2"><Clock size={12} className="inline mr-1" />Timeframe</div>
            <div className="flex flex-wrap gap-1">
              {TIMEFRAMES.map(tf => (
                <button key={tf.mins} onClick={() => { setTimeframe(tf.mins); setPrediction(null); }}
                  className={`btn text-xs ${timeframe === tf.mins ? "bg-accent-dim text-white" : "bg-ink-700 text-gray-400 hover:bg-ink-600"}`}>
                  {tf.label}
                </button>
              ))}
            </div>
          </div>
        </div>
        <button onClick={runAnalysis} disabled={analyzing} className="btn-primary mt-4 w-full text-base py-2.5">
          {analyzing ? <><Loader2 size={18} className="inline mr-2 animate-spin" />Reading the market…</> : <><Brain size={18} className="inline mr-2" />Read Market</>}
        </button>
      </div>

      {prediction && (
        <div className="space-y-4">
          {/* Header: time + session + countdown */}
          <div className="card flex items-center justify-between text-sm">
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-1.5 text-muted"><Clock size={14} /><span className="font-mono text-xs">{fmtUTC(prediction.analysis_time_utc)}</span></div>
              <div className="flex items-center gap-1.5 text-accent"><Globe size={14} />{prediction.market_session}</div>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted">Next candle:</span>
              <span className="font-mono text-sm font-bold text-warn bg-warn/10 px-2 py-0.5 rounded">{prediction.countdown}</span>
            </div>
          </div>

          {/* Market state + trade bias */}
          <div className={`card border-2 ${dirBg}`}>
            <div className="flex items-center justify-between">
              <div>
                <div className="label mb-1">Current Market State</div>
                <div className="text-lg font-bold text-white">{STATE_LABELS[prediction.market_state] || prediction.market_state}</div>
              </div>
              <div className="text-right">
                <div className="label">Evidence Score</div>
                <div className={`text-3xl font-bold ${dirColor}`}>{fmtPct(prediction.evidence_score)}</div>
              </div>
            </div>
            <div className="flex items-center justify-center gap-3 mt-4 pt-4 border-t border-ink-700">
              <DirIcon size={32} className={dirColor} />
              <span className={`text-2xl font-bold ${dirColor} uppercase`}>{prediction.direction === "wait" ? "WAIT" : prediction.direction}</span>
            </div>
            <div className="flex items-center justify-center gap-6 mt-2 text-sm">
              <span className="text-ok">{bullCount} tools confirm BUY</span>
              <span className="text-muted">vs</span>
              <span className="text-bad">{bearCount} tools confirm SELL</span>
            </div>
          </div>

          {/* Trade levels */}
          <div className="grid grid-cols-3 gap-3">
            <div className="card text-center"><div className="label">Entry</div><div className="text-lg font-mono text-gray-200">{fmt(prediction.entry_price, 5)}</div></div>
            <div className="card text-center"><div className="label">Stop Loss</div><div className="text-lg font-mono text-bad">{fmt(prediction.stop_loss, 5)}</div></div>
            <div className="card text-center"><div className="label">Take Profit</div><div className="text-lg font-mono text-ok">{fmt(prediction.take_profit, 5)}</div></div>
          </div>

          {/* Evidence from each tool */}
          <div className="card">
            <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2"><BarChart3 size={15} className="text-accent" /> Tool Readings (Evidence)</h3>
            <div className="space-y-1">
              {prediction.evidence.map((e, i) => <EvidenceRow key={i} evidence={e} />)}
            </div>
          </div>

          {/* Recent candles */}
          {prediction.recent_candles && prediction.recent_candles.length > 0 && (
            <div className="card">
              <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2"><Activity size={15} className="text-accent" /> Last {prediction.recent_candles.length} Candles</h3>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead><tr className="text-left text-muted border-b border-ink-700">
                    <th className="py-1 px-2">#</th><th className="px-2">Dir</th><th className="px-2">Open</th><th className="px-2">High</th><th className="px-2">Low</th><th className="px-2">Close</th><th className="px-2">Body</th><th className="px-2">Pattern</th>
                  </tr></thead>
                  <tbody>
                    {prediction.recent_candles.map((c, i) => (
                      <tr key={i} className="border-b border-ink-700/40">
                        <td className="py-1 px-2 text-muted">-{prediction.recent_candles.length - i}</td>
                        <td className="px-2"><span className={c.direction === "bullish" ? "text-ok" : c.direction === "bearish" ? "text-bad" : "text-muted"}>{c.direction}</span></td>
                        <td className="px-2 font-mono text-gray-400">{fmt(c.open, 5)}</td>
                        <td className="px-2 font-mono text-gray-400">{fmt(c.high, 5)}</td>
                        <td className="px-2 font-mono text-gray-400">{fmt(c.low, 5)}</td>
                        <td className="px-2 font-mono text-gray-400">{fmt(c.close, 5)}</td>
                        <td className="px-2 font-mono text-gray-400">{fmt(c.body, 5)}</td>
                        <td className="px-2 text-accent">{c.pattern}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* Upper timeframe context */}
          {prediction.upper_timeframe_context && prediction.upper_timeframe_context.length > 0 && (
            <div className="card">
              <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2"><Layers size={15} className="text-accent" /> Upper Timeframe State</h3>
              <div className="space-y-2">
                {prediction.upper_timeframe_context.map((u, i) => (
                  <div key={i} className="flex items-center gap-3 bg-ink-900 rounded px-3 py-2">
                    <span className="font-bold text-white text-sm w-12">{u.label}</span>
                    <span className={u.trend === "bullish" ? "text-ok text-sm" : "text-bad text-sm"}>{u.trend}</span>
                    <span className="text-xs text-muted">RSI {String(u.rsi)} | ADX {String(u.adx)}</span>
                    <span className="text-xs text-accent ml-auto">{u.summary.split("—")[1]?.trim() || ""}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* What to watch */}
          {prediction.what_to_watch && prediction.what_to_watch.length > 0 && (
            <div className="card border-warn/20">
              <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2"><Eye size={15} className="text-warn" /> What to Watch</h3>
              <ul className="space-y-1.5">
                {prediction.what_to_watch.map((w, i) => (
                  <li key={i} className="text-sm text-gray-300 flex items-start gap-2">
                    <span className="text-warn mt-0.5">▸</span> {w}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Full report */}
          <div className="card">
            <h3 className="text-sm font-semibold text-white mb-2 flex items-center gap-2"><Target size={15} className="text-accent" /> Full Reading Report</h3>
            <pre className="text-xs text-gray-300 whitespace-pre-wrap font-mono leading-relaxed">{prediction.reasoning}</pre>
          </div>

          {/* Trade button */}
          {prediction.direction !== "wait" ? (
            <button onClick={placeTrade} disabled={trading} className={`btn w-full text-base py-3 ${prediction.direction === "buy" ? "bg-ok text-white hover:bg-ok/80" : "bg-bad text-white hover:bg-bad/80"}`}>
              {trading ? <><Loader2 size={18} className="inline mr-2 animate-spin" />Placing…</> : <><Zap size={18} className="inline mr-2" />Place {prediction.direction.toUpperCase()} Trade</>}
            </button>
          ) : (
            <div className="card text-center text-muted py-6"><Minus size={24} className="inline mb-2" /><p>Evidence is inconclusive. WAIT for clearer signals.</p></div>
          )}
        </div>
      )}
    </div>
  );
}

function EvidenceRow({ evidence }: { evidence: Evidence }) {
  const color = evidence.confirms === "buy" ? "text-ok" : evidence.confirms === "sell" ? "text-bad" : "text-muted";
  const bg = evidence.confirms === "buy" ? "bg-ok/5" : evidence.confirms === "sell" ? "bg-bad/5" : "bg-ink-900";
  const Icon = evidence.source.includes("candlestick") ? Activity : evidence.source.includes("note") ? Brain : evidence.source.includes("upper") ? Layers : BarChart3;
  return (
    <div className={`flex items-start gap-3 rounded px-3 py-2 ${bg}`}>
      <Icon size={14} className="text-muted shrink-0 mt-0.5" />
      <div className="flex-1 min-w-0">
        <span className="text-sm text-gray-300">{evidence.finding}</span>
      </div>
      <span className={`text-xs font-bold shrink-0 mt-0.5 ${color}`}>
        {evidence.confirms === "buy" ? "→ BUY" : evidence.confirms === "sell" ? "→ SELL" : ""}
      </span>
    </div>
  );
}
