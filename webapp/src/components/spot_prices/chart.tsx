'use client';

import { useSpotPrices } from "@/lib/api-client";
import { SpotPrice } from "@/lib/types";
import {
  ComposedChart, Bar, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer
} from "recharts";

const COLORS = ["#6366f1", "#10b981", "#f59e0b", "#ef4444"];

// Per-chain band values are [min, max] tuples — recharts Bar renders these
// as a discrete range bar (wick) at each timestamp.
// Per-chain timestamps are prefixed with "T" to prevent recharts numeric coercion.
type ChartPoint = {
  label: string;
  [key: string]: string | number | [number, number];
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
    const point: ChartPoint = {
      label: new Date(Number(ts) || ts).toLocaleTimeString([], { hour12: false }),
    };
    for (const chain of chains) {
      const sp = byChain[chain];
      if (sp) {
        point[`${chain}_band`] = [sp.min_price, sp.max_price];
        point[`${chain}_max`] = sp.max_price;
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
        // Only process invisible Line entries with clean chain names
        if (entry.name.endsWith('_wick') || entry.name.endsWith('_band')) return null;
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
              max <span className="text-foreground">{sp.max_price.toFixed(6)}</span>
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
      <div className="h-full min-h-72 flex items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (isError || !prices.length) {
    return (
      <div className="h-full min-h-72 flex items-center justify-center text-muted-foreground">
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
          {chains.map((chain, i) => ([
            // Wick: custom shape draws the filled bar + H/L dots all at the
            // same x-position (avoiding the offset that separate Line dots cause).
            <Bar
              key={`${chain}_wick`}
              dataKey={`${chain}_band`}
              name={`${chain}_wick`}
              barSize={12}
              legendType="none"
              shape={(props: { x: number; y: number; width: number; height: number }) => {
                const color = COLORS[i % COLORS.length];
                const { x, y, width, height } = props;
                if (!height || height <= 0) return <g />;
                const cx = x + width / 2;
                const barW = 12;        // bar width in px
                const dotR = 3.5;         // dot radius in px
                const fillOpacity = 0.2; // bar fill transparency (0–1)
                const strokeWidth = 1;   // bar border thickness
                return (
                  <g>
                    <rect
                      x={cx - barW / 2} y={y} width={barW} height={height}
                      fill={color} fillOpacity={fillOpacity}
                      stroke={color} strokeWidth={strokeWidth} rx={1}
                    />
                    <circle cx={cx} cy={y} r={dotR} fill={color} />
                    <circle cx={cx} cy={y + height} r={dotR} fill={color} />
                  </g>
                );
              }}
            />,
            // Invisible line — provides tooltip payload entry under the chain name.
            <Line
              key={chain}
              type="monotone"
              dataKey={`${chain}_max`}
              name={chain}
              stroke="none"
              dot={false}
              activeDot={false}
              legendType="circle"
              connectNulls={false}
            />,
          ]))}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}
