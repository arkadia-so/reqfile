export function report(csv: string): string {
  const rows = csv.split("\n").map((line) => line.split(","));
  const totals = new Map<string, number>();
  for (const [region, amount] of rows) {
    totals.set(region, (totals.get(region) ?? 0) + Number(amount));
  }
  return [...totals].map(([r, t]) => `${r}: ${t.toFixed(2)}`).join("\n");
}
