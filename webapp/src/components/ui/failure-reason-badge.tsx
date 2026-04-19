"use client"

import type { FailureReason } from "@/lib/types"

const LABELS: Record<FailureReason, string> = {
  slippage: "Slippage",
  insufficient_inventory: "Insufficient inventory",
  reverted: "Reverted",
  submission_error: "Submission error",
}

const CLASSES: Record<FailureReason, string> = {
  slippage: "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30",
  insufficient_inventory: "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30",
  reverted: "bg-rose-500/15 text-rose-700 dark:text-rose-400 border-rose-500/30",
  submission_error: "bg-slate-500/15 text-slate-700 dark:text-slate-300 border-slate-500/30",
}

export function FailureReasonBadge({ reason }: { reason: FailureReason }) {
  const label = LABELS[reason] ?? reason
  const cls = CLASSES[reason] ?? "bg-muted text-muted-foreground border-border"
  return (
    <span
      className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium whitespace-nowrap ${cls}`}
    >
      {label}
    </span>
  )
}
