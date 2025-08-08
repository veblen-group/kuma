import type { Metadata } from "next";
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "kuma",
  description: "Cross-chain arbitrage bot for Tycho Community Extensions TAP-6",
};


// Create a client
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Global settings
      staleTime: 1000 * 60 * 5, // 5 minutes
      gcTime: 1000 * 60 * 60, // 1 hour
      retry: 2, // Retry failed requests twice
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000), // Exponential backoff
    },
  },
})


export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <QueryClientProvider client={queryClient}>
        <body
          className={`${geistSans.variable} ${geistMono.variable}  antialiased`}
        >
          {children}
          {process.env.NODE_ENV === 'development' &&
            <ReactQueryDevtools initialIsOpen={false} />}
        </body>
      </QueryClientProvider>
    </html>
  );
}
