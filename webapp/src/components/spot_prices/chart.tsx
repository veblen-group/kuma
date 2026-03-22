'use client';

import { useSpotPrices } from "@/lib/api-client";
import { SpotPrice } from "@/lib/types";
import {
  ComposedChart, Area, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer
} from "recharts";

const COLORS = ["#6366f1", "#10b981", "#f59e0b", "#ef4444"];

// Only numeric price fields. Per-chain timestamps are stored as "T"+ts so
// recharts won't coerce them to numbers and corrupt the Y-axis domain.
type ChartPoint = {
  label: string;
  [key: string]: string | number;
};

function buildChartData(prices: SpotPrice[]): {
  chains: string[];
  points: ChartPoint[];
  rawByTime: Map<string, Record<string, SpotPrice>>;
} {
  const chainSet = new Set(prices.map(p => p.chain));
  const chains = Array.from(chainSet).sort();

  const byTime = new Map<string, Record<string, SpotPrice>>();
  for (const price of prices) {
    if (!price.created_at) continue;
    const existing = byTime.get(price.created_at) ?? {};
    existing[price.chain] = price;
    byTime.set(price.created_at, existing);
  }

  const sorted = Array.from(byTime.entries()).sort(
    ([a], [b]) => new Date(Number(a) || a).getTime() - new Date(Number(b) || b).getTime()
  );

  const points: ChartPoint[] = sorted.map(([ts, byChain]) => {
    const point: ChartPoint = { label: new Date(Number(ts) || ts).toLocaleTimeString() };
    for (const chain of chains) {
      const sp = byChain[chain];
      if (sp) {
        point[`${chain}_min`] = sp.min_price;
        point[`${chain}_max`] = sp.max_price;
        // Prefix with "T" so recharts won't coerce to a number for domain calc
        point[`${chain}_ts`] = 'T' + ts;
      }
    }
    return point;
  });

  return { chains, points, rawByTime: byTime };
}

function CustomTooltip({
  active,
  payload,
  label,
  rawByTime,
}: {
  active?: boolean;
  payload?: Array<{ name: string; value: number; payload: ChartPoint }>;
  label?: string;
  rawByTime: Map<string, Record<string, SpotPrice>>;
}) {
  if (!active || !payload?.length) return null;

  const seen = new Set<string>();
  return (
    <div className="bg-popover border border-border rounded-md p-3 text-xs shadow-md space-y-2">
      <p className="font-medium text-foreground text-sm">{label}</p>
      {payload.map(entry => {
        // Skip area band entries — only process the Line entries (clean chain names)
        if (entry.name.endsWith('_band')) return null;
        if (seen.has(entry.name)) return null;
        seen.add(entry.name);
        const chain = entry.name;
        const chainTs = (entry.payload[`${chain}_ts`] as string | undefined)?.slice(1);
        const sp = chainTs ? rawByTime.get(chainTs)?.[chain] : undefined;
        if (!sp) return null;
        return (
          <div key={chain} className="space-y-0.5">
            <p className="font-semibold text-foreground">{chain}</p>
            <p className="text-muted-foreground">
              max <span className="text-foreground">{entry.value.toFixed(6)}</span>
              {sp.max_pool_id && <span className="ml-1 opacity-60">({sp.max_pool_id})</span>}
            </p>
            <p className="text-muted-foreground">
              min <span className="text-foreground">{sp.min_price.toFixed(6)}</span>
              {sp.min_pool_id && <span className="ml-1 opacity-60">({sp.min_pool_id})</span>}
            </p>
            <p className="text-muted-foreground">block <span className="text-foreground">{sp.block_height}</span></p>
          </div>
        );
      })}
    </div>
  );
}

export function SpotPriceChart() {
  const { data, isLoading, isError } = useSpotPrices(
    { page: 1, pageSize: 50 },
    { staleTime: 30_000, refetchInterval: 30_000 }
  );
  const prices = data?.data ?? [];

  if (isLoading) {
    return (
      <div className="h-96 flex items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (isError || !prices.length) {
    return (
      <div className="h-96 flex items-center justify-center text-muted-foreground">
        No data available
      </div>
    );
  }

  const { chains, points, rawByTime } = buildChartData(prices);

  const yMin = Math.min(...prices.map(p => p.min_price)) * 0.9995;
  const yMax = Math.max(...prices.map(p => p.max_price)) * 1.0005;

  return (
    <div className="h-full min-h-72">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={points} margin={{ top: 4, right: 8, bottom: 4, left: 8 }}>
          <XAxis dataKey="label" tick={{ fontSize: 11 }} tickCount={6} />
          <YAxis
            type="number"
            tick={{ fontSize: 11 }}
            domain={[yMin, yMax]}
            allowDataOverflow
            tickFormatter={(v: number) => v.toFixed(5)}
            width={70}
          />
          <Tooltip content={<CustomTooltip rawByTime={rawByTime} />} />
          <Legend />
          {chains.map((chain, i) => (
            [
              // Band between min_price and max_price using a function dataKey
              // returning [lower, upper]. No stacking — domain stays explicit.
              <Area
                key={`${chain}_band`}
                type="monotone"
                name={`${chain}_band`}
                dataKey={(d: ChartPoint) => {
                  const lo = d[`${chain}_min`];
                  const hi = d[`${chain}_max`];
                  return (typeof lo === 'number' && typeof hi === 'number') ? [lo, hi] : null;
                }}
                fill={COLORS[i % COLORS.length]}
                fillOpacity={0.2}
                stroke="none"
                legendType="none"
                connectNulls
                dot={false}
                activeDot={false}
              />,
              // Top edge line with dots at actual data points
              <Line
                key={chain}
                type="monotone"
                dataKey={`${chain}_max`}
                name={chain}
                stroke={COLORS[i % COLORS.length]}
                dot={{ r: 3, fill: COLORS[i % COLORS.length] }}
                activeDot={{ r: 5 }}
                strokeWidth={2}
              />,
            ]
          ))}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}
