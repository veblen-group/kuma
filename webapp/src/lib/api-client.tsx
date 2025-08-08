'use client';

import { SpotPrice, Signal, PaginatedResponse } from "@/lib/types";
import { QueryClient, useQuery, UseQueryOptions, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";

import React, { useState } from "react";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

export interface FetchParams {
  pair: string;
  page?: number;
  pageSize?: number;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(endpoint: string, params?: Record<string, string>): Promise<PaginatedResponse<T>> {
    const url = new URL(`${this.baseUrl}${endpoint}`);

    if (params) {
      Object.entries(params).forEach(([key, value]) => {
        url.searchParams.append(key, value);
      });
    }

    const response = await fetch(url.toString());

    if (!response.ok) {
      throw new Error(`API request failed: ${response.status} ${response.statusText}`);
    }

    return response.json();
  }

  async getSpotPrices(params: FetchParams): Promise<PaginatedResponse<SpotPrice>> {
    return this.request<SpotPrice>('/spot_prices', {
      pair: params.pair,
      page: (params.page ?? 1).toString(),
      page_size: (params.pageSize ?? 10).toString()
    });
  }

  async getSignals(params: FetchParams): Promise<PaginatedResponse<Signal>> {
    return this.request<Signal>('/signals', {
      pair: params.pair,
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
      params.pair,
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
      params.pair,
      params.page ?? 1,
      params.pageSize ?? 10
    ],
    queryFn: () => apiClient.getSignals(params),
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
        // Global settings
        staleTime: 1000 * 60 * 5, // 5 minutes
        gcTime: 1000 * 60 * 60, // 1 hour
        retry: 2, // Retry failed requests twice
        retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000), // Exponential backoff
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
