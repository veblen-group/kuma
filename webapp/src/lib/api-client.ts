import { SpotPrice, Signal, PaginatedResponse } from "@/lib/types";
import { useQuery, UseQueryOptions } from "@tanstack/react-query";
import React from "react";

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
    return this.request<SpotPrice>('/spot-prices', {
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
  const queryKey = React.useMemo(() => [
    'spot-prices',
    params.pair,
    params.page ?? 1,
    params.pageSize ?? 10
  ], [params.pair, params.page, params.pageSize]);

  return useQuery<PaginatedResponse<SpotPrice>>({
    ...options,
    queryKey: queryKey,
    queryFn: () => apiClient.getSpotPrices(params),
  });
}

export function useSignals(params: FetchParams, options?: Partial<UseQueryOptions<PaginatedResponse<Signal>>>) {
  const queryKey = React.useMemo(() => [
    'signals',
    params.pair,
    params.page ?? 1,
    params.pageSize ?? 10
  ], [params.pair, params.page, params.pageSize]);

  return useQuery<PaginatedResponse<Signal>>({
    ...options,
    queryKey: queryKey,
    queryFn: () => apiClient.getSignals(params),
  });
}
