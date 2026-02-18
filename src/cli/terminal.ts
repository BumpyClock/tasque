/** Shared terminal dimension helpers used by render and watch modules. */

export type Density = "wide" | "medium" | "narrow";

export function resolveWidth(raw?: number): number {
  if (typeof raw === "number" && Number.isFinite(raw) && raw > 0) {
    return Math.floor(raw);
  }
  if (typeof process.stdout.columns === "number" && process.stdout.columns > 0) {
    return process.stdout.columns;
  }
  return 120;
}

export function resolveDensity(width: number): Density {
  if (width >= 120) return "wide";
  if (width >= 90) return "medium";
  return "narrow";
}
