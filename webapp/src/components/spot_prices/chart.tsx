'use client';

import { useSpotPrices } from "@/lib/api-client";
import { SpotPrice } from "@/lib/types";
import {
  LineChart, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer
} from "recharts";

const COLORS = ["#6366f1", "#10b981", "#f59e0b", "#ef4444"];

type ChartPoint = {
  label: string;
  [key: string]: string | number;
};

function buildChartData(prices: SpotPrice[]): { chains: string[]; points: ChartPoint[] } {
  const chainSet = new Set(prices.map(p => p.chain));
  const chains = Array.from(chainSet);

  const byTime = new Map<string, Record<string, SpotPrice>>();
  for (const price of prices) {
    if (!price.created_at) continue;
    const existing = byTime.get(price.created_at) ?? {};
    existing[price.chain] = price;
    byTime.set(price.created_at, existing);
  }

  const sorted = Array.from(byTime.entries()).sort(
    ([a], [b]) => new Date(a).getTime() - new Date(b).getTime()
  );

  const points: ChartPoint[] = sorted.map(([ts, byChain]) => {
    const point: ChartPoint = { label: new Date(ts).toLocaleTimeString() };
    for (const chain of chains) {
      const sp = byChain[chain];
      if (sp) {
        point[`${chain}_max`] = sp.max_price;
        point[`${chain}_min`] = sp.min_price;
        point[`${chain}_max_pool`] = sp.max_pool_id;
        point[`${chain}_min_pool`] = sp.min_pool_id;
        point[`${chain}_block`] = sp.block_height;
      }
    }
    return point;
  });

  return { chains, points };
}

function CustomTooltip({ active, payload, label }: {
  active?: boolean;
  payload?: Array<{ name: string; value: number; payload: ChartPoint }>;
  label?: string;
}) {
  if (!active || !payload?.length) return null;

  const seen = new Set<string>();
  return (
    <div className="bg-popover border border-border rounded-md p-3 text-xs shadow-md space-y-2">
      <p className="font-medium text-foreground text-sm">{label}</p>
      {payload.map(entry => {
        const chain = entry.name.replace(/_max$/, '');
        if (!entry.name.endsWith('_max') || seen.has(chain)) return null;
        seen.add(chain);
        const p = entry.payload;
        const min = p[`${chain}_min`] as number | undefined;
        const maxPool = p[`${chain}_max_pool`] as string | undefined;
        const minPool = p[`${chain}_min_pool`] as string | undefined;
        const block = p[`${chain}_block`] as number | undefined;
        return (
          <div key={chain} className="space-y-0.5">
            <p className="font-semibold text-foreground">{chain}</p>
            <p className="text-muted-foreground">
              max <span className="text-foreground">{entry.value.toFixed(6)}</span>
              {maxPool && <span className="ml-1 opacity-60">({maxPool})</span>}
            </p>
            {min !== undefined && (
              <p className="text-muted-foreground">
                min <span className="text-foreground">{min.toFixed(6)}</span>
                {minPool && <span className="ml-1 opacity-60">({minPool})</span>}
              </p>
            )}
            {block !== undefined && (
              <p className="text-muted-foreground">block <span className="text-foreground">{block}</span></p>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function SpotPriceChart() {
  const { data, isLoading, isError } = useSpotPrices({ page: 1, pageSize: 200 });

  if (isLoading) {
    return (
      <div className="h-96 flex items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (isError || !data?.data?.length) {
    return (
      <div className="h-96 flex items-center justify-center text-muted-foreground">
        No data available
      </div>
    );
  }

  const { chains, points } = buildChartData(data.data);

  return (
    <div className="h-96">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={points} margin={{ top: 4, right: 8, bottom: 4, left: 8 }}>
          <XAxis dataKey="label" tick={{ fontSize: 11 }} />
          <YAxis tick={{ fontSize: 11 }} />
          <Tooltip content={<CustomTooltip />} />
          <Legend />
          {chains.map((chain, i) => (
            <Line
              key={chain}
              type="monotone"
              dataKey={`${chain}_max`}
              name={`${chain}_max`}
              stroke={COLORS[i % COLORS.length]}
              dot={false}
              activeDot={{ r: 4 }}
              strokeWidth={2}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
