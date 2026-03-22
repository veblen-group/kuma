import { SignalTable } from "@/components/signals/table";
import { SpotPriceTable } from "@/components/spot_prices/table";
import { SpotPriceChart } from "@/components/spot_prices/chart";
import { SuccessfulTradeResultTable } from "@/components/successful_trades/table";
import { FailedOnSlowTradeResultTable } from "@/components/failed_on_slow_trades/table";
import { FailedOnFastTradeResultTable } from "@/components/failed_on_fast_trades/table";
import { StrategyCard } from "@/components/strategy-card";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";

export default function Home() {
  return (
    <main className="container mx-auto px-4 py-8 space-y-6">
      <header>
        <h1 className="text-3xl font-bold text-primary">Kuma</h1>
        <p className="text-muted-foreground">Cross-Chain Signal Dashboard</p>
      </header>

      <Separator />

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div className="lg:col-span-7 flex flex-col self-stretch">
          <Card className="flex flex-col flex-1">
            <CardHeader className="shrink-0">
              <CardTitle>Price Chart</CardTitle>
            </CardHeader>
            <CardContent className="flex-1 min-h-0">
              <SpotPriceChart />
            </CardContent>
          </Card>
        </div>

        <div className="lg:col-span-5 space-y-6">
          <Card>
            <CardContent className="p-4">
              <StrategyCard />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Spot Prices</CardTitle>
            </CardHeader>
            <CardContent>
              <SpotPriceTable />
            </CardContent>
          </Card>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Signals</CardTitle>
        </CardHeader>
        <CardContent>
          <SignalTable />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Successful Trades</CardTitle>
        </CardHeader>
        <CardContent>
          <SuccessfulTradeResultTable />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Failed Trades</CardTitle>
        </CardHeader>
        <CardContent>
          <FailedOnSlowTradeResultTable />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Stuck Trades</CardTitle>
        </CardHeader>
        <CardContent>
          <FailedOnFastTradeResultTable />
        </CardContent>
      </Card>
    </main>
  );
}
