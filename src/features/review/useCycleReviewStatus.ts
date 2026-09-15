/**
 * 复盘入口状态 hook（spec: 列头复盘入口 未复盘/已复盘两态）。
 *
 * Returns `"loading"` while the first fetch is in flight so callers can avoid
 * flashing the wrong entry state, then `"reviewed"` / `"unreviewed"`. Errors
 * degrade to `"unreviewed"` — a failed read must not lock the entry.
 */

import { useQuery } from "@tanstack/react-query";
import { getCycleReview, reviewQk } from "./api";

export type CycleReviewStatus = "loading" | "reviewed" | "unreviewed";

export function useCycleReviewStatus(cycleId: string): CycleReviewStatus {
  const query = useQuery({
    queryKey: reviewQk.cycleReview(cycleId),
    queryFn: () => getCycleReview(cycleId),
    staleTime: Infinity,
    retry: false,
  });
  if (query.isPending) return "loading";
  return query.data != null ? "reviewed" : "unreviewed";
}
