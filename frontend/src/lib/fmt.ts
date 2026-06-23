export function fmt(n: number | null | undefined, dp = 2): string {
  if (n === null || n === undefined) return "—";
  return n.toLocaleString("en-US", { minimumFractionDigits: dp, maximumFractionDigits: dp });
}
export function fmtPct(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  return `${(n * 100).toFixed(1)}%`;
}
export function fmtPctRaw(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  return `${n.toFixed(2)}%`;
}
export function fmtDate(s: string | null | undefined): string {
  if (!s) return "—";
  return new Date(s).toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
export function pnlColor(n: number | null | undefined): string {
  if (n === null || n === undefined) return "text-muted";
  return n > 0 ? "text-ok" : n < 0 ? "text-bad" : "text-muted";
}
