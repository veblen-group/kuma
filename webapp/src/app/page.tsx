import { SignalTable } from "@/components/signals/table";
import { SpotPriceTable } from "@/components/spot_prices/table";
import { SpotPriceChart } from "@/components/spot_prices/chart";
import { SuccessfulTradeResultTable } from "@/components/successful_trades/table";
import { FailedOnSlowTradeResultTable } from "@/components/failed_on_slow_trades/table";
import { FailedOnFastTradeResultTable } from "@/components/failed_on_fast_trades/table";
import { StrategyCard } from "@/components/strategy-card";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { ThemeToggle } from "@/components/theme-toggle";

export default function Home() {
  return (
    <main className="container mx-auto px-4 py-6 space-y-4">
      <header className="flex justify-between items-start">
        <div>
          <h1 className="text-3xl font-bold text-primary">Kuma</h1>
          <p className="text-muted-foreground text-sm">
            Cross-chain arbitrage bot for Tycho Community Extensions{" "}
            <a
              href="https://github.com/propeller-heads/tycho-x/blob/main/TAP-6.md"
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:text-primary"
            >
              TAP-6
            </a>
          </p>
        </div>
        <ThemeToggle />
      </header>

      <Separator />

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div className="lg:col-span-7 flex flex-col">
          <Card className="flex flex-col flex-1">
            <CardHeader className="py-3 px-4">
              <CardTitle>Price Chart</CardTitle>
            </CardHeader>
            <CardContent className="flex-1 min-h-0 px-4 pb-4">
              <SpotPriceChart />
            </CardContent>
          </Card>
        </div>

        <div className="lg:col-span-5 space-y-4">
          <Card>
            <CardContent className="px-4 py-3">
              <p className="text-sm font-semibold mb-3">Strategy</p>
              <StrategyCard />
            </CardContent>
          </Card>
          <Card>
            <CardContent className="px-4 py-3">
              <p className="text-sm font-semibold mb-2">Spot Prices</p>
              <SpotPriceTable />
            </CardContent>
          </Card>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Signals</CardTitle>
          <CardDescription>Detected cross-chain arbitrage opportunities. Each signal identifies a price imbalance between the slow and fast chain pools.</CardDescription>
        </CardHeader>
        <CardContent>
          <SignalTable />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Successful Trades</CardTitle>
          <CardDescription>Trades where both the slow and fast chain transactions executed on-chain.</CardDescription>
        </CardHeader>
        <CardContent>
          <SuccessfulTradeResultTable />
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card>
          <CardHeader>
            <CardTitle>Failed Trades</CardTitle>
            <CardDescription>Trades where execution failed on the slow chain before the fast leg was attempted.</CardDescription>
          </CardHeader>
          <CardContent>
            <FailedOnSlowTradeResultTable />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Stuck Trades</CardTitle>
            <CardDescription>Trades where the slow transaction confirmed but the fast chain transaction failed to execute.</CardDescription>
          </CardHeader>
          <CardContent>
            <FailedOnFastTradeResultTable />
          </CardContent>
        </Card>
      </div>
    </main>
  );
}
