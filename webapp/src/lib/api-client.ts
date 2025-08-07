import { SpotPrice, Signal, PaginatedResponse } from "@/lib/types";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

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

  async getSpotPrices(pair: string): Promise<SpotPrice[]> {
    // Use mock data block height from migrations
    const response = await this.request<SpotPrice>('/spot_prices', {
      pair
    });
    return response.data;
  }

  async getSignals(pair: string): Promise<Signal[]> {
    // Use mock data block height from migrations
    const response = await this.request<Signal>('/signals', {
      pair
    });
    return response.data;
  }
}

export const apiClient = new ApiClient();