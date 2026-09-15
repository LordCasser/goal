/**
 * 一次性提示卡（onboarding-guidance「就地提示」）：展示当前值得显示的
 * 解释卡。卡片占据正常文档流（非浮层），因此永不阻塞其下方控件的操作；
 * 关闭经 dismiss_hint 持久化，之后不再出现。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { dismissHint, getDismissedHints } from "./api";
import { hintById } from "./hints";

export type HintCardProps = {
  /** The hint id from the registry (hints.ts). */
  hintId: string;
  /** Lets the host drop the slot right away once dismissed. */
  onDismissed?: () => void;
};

/**
 * In-flow explainer card for one registry hint. The dismissed query decides
 * visibility; `onDismissed` lets the host drop the slot immediately so the
 * card never flashes back while the write is in flight.
 */
export function HintCard({ hintId, onDismissed }: HintCardProps) {
  const queryClient = useQueryClient();
  const dismissed = useQuery({
    queryKey: dismissedHintsKey(),
    queryFn: getDismissedHints,
    staleTime: Infinity,
  });
  const hide = useMutation({
    mutationFn: () => dismissHint(hintId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: dismissedHintsKey() });
      onDismissed?.();
    },
  });

  if (dismissed.isLoading || dismissed.isError) return null;
  if (dismissed.data?.includes(hintId)) return null;
  const hint = hintById(hintId);
  if (!hint) return null;

  return (
    <section
      data-hint-id={hint.id}
      aria-label={hint.title}
      className="mb-3 rounded-sm border border-light bg-subtle p-3"
    >
      <header className="flex items-start justify-between gap-2">
        <h3 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
          {hint.title}
        </h3>
        <button
          type="button"
          className="h-7 w-7 shrink-0 rounded-sm text-secondary hover:bg-hover hover:text-primary"
          aria-label="Dismiss hint"
          title="Won't show again"
          onClick={() => hide.mutate()}
        >
          <svg
            viewBox="0 0 16 16"
            className="mx-auto h-4 w-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
        </button>
      </header>
      <p className="mt-1 text-menu text-secondary">{hint.body}</p>
    </section>
  );
}

/**
 * Local query key. src/lib/events.ts requires feature modules to use its qk
 * factories, but this change may not edit src/lib — the "onboarding" root is
 * the local exception, never touched by the global invalidation matrix.
 */
export function dismissedHintsKey(): readonly ["onboarding", "dismissed-hints"] {
  return ["onboarding", "dismissed-hints"] as const;
}
