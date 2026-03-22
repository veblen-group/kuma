import { useRef } from "react"
import { PaginatedResponse } from "@/lib/types"

export function useStablePageCount<T>(data: PaginatedResponse<T> | undefined): number {
  const locked = useRef<number | null>(null)
  if (locked.current === null && data?.pagination.total_pages) {
    locked.current = data.pagination.total_pages
  }
  return locked.current ?? 0
}
