'use client';

import { useSpotPricesChart } from "@/lib/api-client";
import { useStrategy } from "@/components/strategy-provider";
import { getChainName, getChainLogoUrl } from "@/lib/chains";
import { SpotPrice } from "@/lib/types";
import {
  LineChart, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer
} from "recharts";

const CHAIN_COLORS: Record<string, string> = {
  ethereum: "#10b981", // green
  base:     "#2563eb", // blue
  unichain: "#e91e8c", // pink
};
const FALLBACK_COLORS = ["#6366f1", "#f59e0b", "#ef4444"];

function getChainColor(chain: string, index: number): string {
  return CHAIN_COLORS[chain] ?? FALLBACK_COLORS[index % FALLBACK_COLORS.length];
}

type ChartPoint = {
  label: string;
  ts: string;
  [key: string]: string | number | undefined;
};

// Round a timestamp to the nearest minute so rows from different chains
// that were inserted within the same minute bucket get grouped together.
function bucketKey(raw: string): string {
  const ms = new Date(Number(raw) || raw).getTime();
  return String(Math.round(ms / 60_000) * 60_000);
}

function buildChartData(prices: SpotPrice[]): {
  chains: string[];
  points: ChartPoint[];
  rawByBucket: Map<string, Record<string, SpotPrice>>;
} {
  const chainSet = new Set(prices.map(p => p.chain));
  const chains = Array.from(chainSet).sort();

  const byBucket = new Map<string, Record<string, SpotPrice>>();
  for (const price of prices) {
    if (!price.created_at) continue;
    const key = bucketKey(price.created_at);
    const existing = byBucket.get(key) ?? {};
    existing[price.chain] = price;
    byBucket.set(key, existing);
  }

  const sorted = Array.from(byBucket.entries()).sort(
    ([a], [b]) => Number(a) - Number(b)
  );

  const points: ChartPoint[] = sorted.map(([bucket, byChain]) => {
    const point: ChartPoint = {
      label: new Date(Number(bucket)).toLocaleTimeString([], { hour12: false }),
      ts: bucket,
    };
    for (const chain of chains) {
      const sp = byChain[chain];
      if (sp) {
        point[`${chain}_min`] = sp.min_price;
        point[`${chain}_max`] = sp.max_price;
      }
    }
    return point;
  });

  return { chains, points, rawByBucket: byBucket };
}

function PriceTooltip({
  active,
  payload,
  rawByBucket,
}: {
  active?: boolean;
  payload?: Array<{ name: string; value: number; color: string; dataKey: string; payload: ChartPoint }>;
  label?: string;
  rawByBucket: Map<string, Record<string, SpotPrice>>;
}) {
  if (!active || !payload?.length) return null;

  const bucket = payload[0]?.payload?.ts;
  const timestamp = bucket
    ? new Date(Number(bucket)).toLocaleString([], {
        month: 'short', day: 'numeric',
        hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
      })
    : null;
  const seen = new Set<string>();

  return (
    <div className="bg-popover border border-border rounded-md p-3 text-xs shadow-md space-y-2">
      {timestamp && <p className="font-medium text-foreground text-sm">{timestamp}</p>}
      {payload.map(entry => {
        // dataKey is "ethereum_max" / "ethereum_min" — strip the suffix to get the chain
        const chain = entry.dataKey.replace(/_min$|_max$/, '');
        if (seen.has(chain)) return null;
        seen.add(chain);

        const sp = bucket ? rawByBucket.get(bucket)?.[chain] : undefined;
        if (!sp) return null;

        return (
          <div key={chain} className="space-y-0.5">
            <div className="flex items-center gap-1.5">
              <span
                className="w-2.5 h-2.5 rounded-full shrink-0"
                style={{ backgroundColor: entry.color }}
              />
              <span className="font-semibold text-foreground">{getChainName(chain)}</span>
              <span className="text-muted-foreground ml-auto">block {sp.block_height.toLocaleString()}</span>
            </div>
            <p className="text-muted-foreground pl-4">
              max <span className="text-foreground">{sp.max_price.toFixed(6)}</span>
            </p>
            <p className="text-muted-foreground pl-4">
              min <span className="text-foreground">{sp.min_price.toFixed(6)}</span>
            </p>
          </div>
        );
      })}
    </div>
  );
}

export function SpotPriceChart() {
  const { pair } = useStrategy()
  const { data, isLoading, isError, refetch, isFetching } = useSpotPricesChart();
  const prices = data?.data ?? [];

  if (isLoading) {
    return (
      <div className="h-full flex items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (isError || !prices.length) {
    return (
      <div className="h-full flex items-center justify-center text-muted-foreground">
        No price data for {pair}
      </div>
    );
  }

  const { chains, points, rawByBucket } = buildChartData(prices);

  const allPrices = prices.flatMap(p => [p.min_price, p.max_price]);
  const globalMin = Math.min(...allPrices);
  const globalMax = Math.max(...allPrices);
  const padding = (globalMax - globalMin) * 0.05 || globalMax * 0.0005;

  return (
    <div className="h-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={points} margin={{ top: 4, right: 8, bottom: 4, left: 8 }}>
          <XAxis dataKey="label" tick={{ fontSize: 10 }} tickCount={6} />
          <YAxis
            type="number"
            tick={{ fontSize: 10 }}
            domain={[globalMin - padding, globalMax + padding]}
            allowDataOverflow
            tickFormatter={(v: number) => v.toFixed(5)}
            width={70}
          />
          <Tooltip content={<PriceTooltip rawByBucket={rawByBucket} />} />
          <Legend
            content={({ payload }) => (
              <div className="flex flex-wrap justify-center gap-x-4 gap-y-1 mt-1">
                {chains.map((chain, i) => {
                  const color = getChainColor(chain, i);
                  const logoUrl = getChainLogoUrl(chain);
                  return (
                    <span key={chain} className="inline-flex items-center gap-1.5 text-xs">
                      {logoUrl
                        ? <img src={logoUrl} alt={chain} width={12} height={12} className="rounded-full shrink-0" />
                        : <span className="w-3 h-3 rounded-full shrink-0" style={{ backgroundColor: color }} />
                      }
                      <span style={{ color }}>{getChainName(chain)}</span>
                      <span className="text-muted-foreground">max</span>
                      <span className="inline-block w-6 border-b" style={{ borderColor: color }} />
                      <span className="text-muted-foreground">min</span>
                      <span className="inline-block w-6 border-b border-dashed" style={{ borderColor: color }} />
                    </span>
                  );
                })}
              </div>
            )}
          />
          {chains.map((chain, i) => {
            const color = getChainColor(chain, i);
            return [
              <Line
                key={`${chain}_max`}
                type="linear"
                dataKey={`${chain}_max`}
                name={`${chain} max`}
                stroke={color}
                strokeWidth={1.5}
                dot={{ r: 2.5, fill: color, strokeWidth: 0 }}
                activeDot={{ r: 4, strokeWidth: 0 }}
                connectNulls={false}
              />,
              <Line
                key={`${chain}_min`}
                type="linear"
                dataKey={`${chain}_min`}
                name={`${chain} min`}
                stroke={color}
                strokeWidth={1.5}
                strokeDasharray="4 3"
                dot={{ r: 2.5, fill: color, strokeWidth: 0 }}
                activeDot={{ r: 4, strokeWidth: 0 }}
                connectNulls={false}
              />,
            ];
          })}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
