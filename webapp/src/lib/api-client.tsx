'use client';

import { SpotPrice, Signal, SuccessfulTrade, FailedOnSlowTrade, FailedOnFastTrade, PaginatedResponse } from "@/lib/types";
import { QueryClient, useQuery, UseQueryOptions, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import config from "@/generated/kuma.config.json";
import React, { useState } from "react";

const firstStrategy = config.strategies[0];
const pair = `${firstStrategy.token_a}-${firstStrategy.token_b}`;
// In production the browser hits /api/* which Next.js SSR proxies to the
// internal backend (BACKEND_URL). For local dev without the proxy, fall back
// to the backend directly.
const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || '/api';

export interface FetchParams {
  page?: number;
  pageSize?: number;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(endpoint: string, params?: Record<string, string>): Promise<PaginatedResponse<T>> {
    const queryString = params
      ? '?' + new URLSearchParams(params).toString()
      : '';
    const url = `${this.baseUrl}${endpoint}${queryString}`;

    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`API request failed: ${response.status} ${response.statusText}`);
    }

    return response.json();
  }

  async getSpotPrices(params: FetchParams): Promise<PaginatedResponse<SpotPrice>> {
    return this.request<SpotPrice>('/spot_prices', {
      pair: pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }

  async getSignals(params: FetchParams): Promise<PaginatedResponse<Signal>> {
    return this.request<Signal>('/signals', {
      pair: pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }

  async getSuccessfulTradeResults(params: FetchParams): Promise<PaginatedResponse<SuccessfulTrade>> {
    return this.request<SuccessfulTrade>('/trades/successful', {
      pair: pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }

  async getFailedOnSlowTradeResults(params: FetchParams): Promise<PaginatedResponse<FailedOnSlowTrade>> {
    return this.request<FailedOnSlowTrade>('/trades/failed-on-slow', {
      pair: pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }

  async getFailedOnFastTradeResults(params: FetchParams): Promise<PaginatedResponse<FailedOnFastTrade>> {
    return this.request<FailedOnFastTrade>('/trades/failed-on-fast', {
      pair: pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }
}

export const apiClient = new ApiClient();

export function useSpotPrices(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<SpotPrice>>>) {
  return useQuery<PaginatedResponse<SpotPrice>>({
    ...options,
    queryKey: [
      'spot_prices',
      pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getSpotPrices(params),
  });
}

export function useSignals(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<Signal>>>) {
  return useQuery<PaginatedResponse<Signal>>({
    ...options,
    queryKey: [
      'signals',
      pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getSignals(params),
  });
}

export function useSuccessfulTradeResults(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<SuccessfulTrade>>>) {
  return useQuery<PaginatedResponse<SuccessfulTrade>>({
    ...options,
    queryKey: [
      'successful_trades',
      pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getSuccessfulTradeResults(params),
  });
}

export function useFailedOnSlowTradeResults(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<FailedOnSlowTrade>>>) {
  return useQuery<PaginatedResponse<FailedOnSlowTrade>>({
    ...options,
    queryKey: [
      'failed_on_slow_trades',
      pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getFailedOnSlowTradeResults(params),
  });
}

export function useFailedOnFastTradeResults(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<FailedOnFastTrade>>>) {
  return useQuery<PaginatedResponse<FailedOnFastTrade>>({
    ...options,
    queryKey: [
      'failed_on_fast_trades',
      pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getFailedOnFastTradeResults(params),
  });
}

export default function ApiClientProvider({
  children
}: {
  children: React.ReactNode
}) {
  const [queryClient] = useState(() => new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: Infinity,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        retry: 2,
        retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000),
      },
    },
  }));

  return (
    <QueryClientProvider client={queryClient}>
      {children}
      {process.env.NODE_ENV === 'development' && <ReactQueryDevtools initialIsOpen={false} />}
    </QueryClientProvider>
  );
}
