'use client';

import React, { useState } from 'react';
import { SignalTable } from "@/components/signals/table";
import { SpotPriceTable } from "@/components/spot_prices/table";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import TokenPairPicker from "@/components/tokenPairPicker";

export default function Home() {
  const [selectedPair, setSelectedPair] = useState<string>("WETH-USDC");
  return (
    <main className="container mx-auto px-4 py-8 space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-primary">Kuma</h1>
          <p className="text-muted-foreground">Cross-Chain Signal Dashboard</p>
        </div>
      </header>

      <Separator />

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div className="lg:col-span-7">
          <Card>
            <CardHeader>
              <CardTitle>Price Chart</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-96 flex items-center justify-center text-muted-foreground bg-muted/25 rounded-md">
                Price Chart Placeholder
              </div>
            </CardContent>
          </Card>
        </div>

        <div className="lg:col-span-5 space-y-6">
          <Card>
            <CardContent className="p-4 grid grid-cols-2 gap-4">
              <TokenPairPicker onPairChange={setSelectedPair} />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Spot Prices</CardTitle>
            </CardHeader>
            <CardContent>
              <SpotPriceTable pair={selectedPair} />
            </CardContent>
          </Card>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Signals</CardTitle>
        </CardHeader>
        <CardContent>
          <SignalTable pair={selectedPair} />
        </CardContent>
      </Card>
    </main>
  );
}
