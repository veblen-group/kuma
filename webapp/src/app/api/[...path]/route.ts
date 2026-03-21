/// Server-side proxy for the kuma-backend API.
///
/// All GET requests to `/api/*` are forwarded to the internal backend service
/// (reachable only within the Docker network). This keeps the backend
/// unexposed to the public internet — only the Next.js server talks to it.
///
/// The `BACKEND_URL` env var is read at runtime (server-side only, no
/// `NEXT_PUBLIC_` prefix). Defaults to `http://localhost:8080` for local dev.

import { NextRequest, NextResponse } from "next/server";

const BACKEND_URL = process.env.BACKEND_URL || "http://localhost:8080";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> },
) {
  const { path } = await params;
  const backendPath = path.join("/");
  const search = request.nextUrl.search; // includes leading '?'
  const backendUrl = `${BACKEND_URL}/${backendPath}${search}`;

  try {
    const response = await fetch(backendUrl, {
      headers: {
        Accept: "application/json",
      },
      // Don't cache on the server — the client manages its own cache via
      // React Query with a 5-minute stale time.
      cache: "no-store",
    });

    if (!response.ok) {
      const body = await response.text();
      return NextResponse.json(
        { error: body || response.statusText },
        { status: response.status },
      );
    }

    const data = await response.json();
    return NextResponse.json(data);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Backend unreachable";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}
