/**
 * 复盘面板（change: add-review-retrospective §6 / planner-workspace 复盘入口
 * 与面板）。
 *
 * 两段式布局（spec: 复盘面板的两段式布局）：上方「事实」段只读——数值全部
 * 来自后端查询/快照，界面不自己计算完成率；下方「判断」段可编辑——固定问题
 * 集逐题作答或跳过，未完成项逐条决定去向（carry / later / drop），未决定的
 * 项不会被写入。已保存的复盘显示快照值并标注采集时间；未复盘的周期显示实时
 * 「截至目前」事实（提前复盘合法）。
 *
 * 草稿保留（spec: 关闭并重开）：已填内容存于模块级 draft store（draft.ts），
 * 关闭面板不丢失，重开继续。
 *
 * 导出：调用后端拿到 Markdown 写入剪贴板并就地提示；系统保存对话框为后续
 * 接入点（TODO(coordinator)：接入 dialog 后改为选择保存位置）。
 *
 * 视觉走语义 token（bg-content / border-light / text-secondary …，design.md
 * §4）；面板宽度用全局 --spacing-panel。
 */

import type * as React from "react";
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../ui";
import {
  applyReviewDisposition,
  exportCycleReviewMarkdown,
  getCycleFacts,
  getCycleReview,
  reviewQk,
  saveCycleReview,
  saveCycleReviewMarkdown,
  REVIEW_QUESTIONS,
  type CycleReviewFacts,
  type Disposition,
  type ReviewQuestionId,
} from "./api";
import {
  clearDraft,
  draftFromReview,
  draftToAnswers,
  getDraft,
  setDraft,
  type ReviewDraft,
} from "./draft";
import { formatFocusedTime, formatPercent, formatSnapshotAt } from "./format";

export interface ReviewPanelProps {
  cycleId: string;
  onClose: () => void;
}

const DISPOSITION_BUTTONS: { key: Disposition; label: string; title: string }[] = [
  { key: "carry", label: "Carry", title: "Copy into the next cycle" },
  { key: "later", label: "Do Later", title: "Move back to the Do Later list" },
  { key: "drop", label: "Drop", title: "Let this item go" },
];

export function ReviewPanel({ cycleId, onClose }: ReviewPanelProps): React.JSX.Element {
  const queryClient = useQueryClient();
  const reviewQuery = useQuery({
    queryKey: reviewQk.cycleReview(cycleId),
    queryFn: () => getCycleReview(cycleId),
    retry: false,
  });
  const review = reviewQuery.data ?? null;

  // 未复盘时取实时「截至目前」事实；已复盘时只用快照，不重算历史。
  const liveFactsQuery = useQuery({
    queryKey: reviewQk.cycleFacts(cycleId),
    queryFn: () => getCycleFacts(cycleId),
    enabled: review === null,
    retry: false,
  });
  const facts: CycleReviewFacts | null = review ? review.facts : (liveFactsQuery.data ?? null);

  const [draft, setDraftState] = useState<ReviewDraft>(() => getDraft(cycleId) ?? {});
  // 已保存的回答先落到草稿，再由本地编辑接管（关闭不丢、重开可续）。
  useEffect(() => {
    if (review && getDraft(cycleId) === undefined) {
      const initial = draftFromReview(review);
      setDraft(cycleId, initial);
      setDraftState(initial);
    }
  }, [review, cycleId]);

  const update = (questionId: ReviewQuestionId, text: string) => {
    const next: ReviewDraft = {
      ...draft,
      [questionId]: { text, skipped: draft[questionId]?.skipped ?? false },
    };
    setDraft(cycleId, next);
    setDraftState(next);
  };

  const toggleSkip = (questionId: ReviewQuestionId) => {
    const current = draft[questionId];
    const next: ReviewDraft = {
      ...draft,
      [questionId]: { text: current?.text ?? "", skipped: !current?.skipped },
    };
    setDraft(cycleId, next);
    setDraftState(next);
  };

  const invalidateReview = () => {
    void queryClient.invalidateQueries({ queryKey: reviewQk.cycleReview(cycleId) });
    void queryClient.invalidateQueries({ queryKey: reviewQk.reviewSummary() });
  };

  const save = useMutation({
    mutationFn: () => saveCycleReview({ cycle_id: cycleId, answers: draftToAnswers(draft) }),
    onSuccess: () => {
      clearDraft(cycleId);
      invalidateReview();
    },
  });

  const dispose = useMutation({
    mutationFn: ({ taskId, disposition }: { taskId: string; disposition: Disposition }) =>
      applyReviewDisposition(cycleId, taskId, disposition),
    onSuccess: invalidateReview,
  });

  const [exportNote, setExportNote] = useState<string | null>(null);
  const exportMarkdown = useMutation({
    // 系统保存对话框优先（tasks §7.2）；对话框不可用（如前端测试环境）时
    // 退回剪贴板，两条路径都拿到同一段 Markdown。
    mutationFn: async () => {
      const markdown = await exportCycleReviewMarkdown(cycleId);
      try {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const target = await save({
          defaultPath: `review-${cycleId}.md`,
          filters: [{ name: "Markdown", extensions: ["md"] }],
        });
        if (!target) return markdown; // user cancelled — nothing to report
        await saveCycleReviewMarkdown(cycleId, target);
        setExportNote(`Saved to ${target}`);
        return markdown;
      } catch {
        await navigator.clipboard.writeText(markdown);
        setExportNote("Markdown copied to clipboard");
        return markdown;
      }
    },
    onError: () => setExportNote("Export failed: save a review first"),
  });

  const recordedByTask = new Map(
    (review?.dispositions ?? []).map((record) => [record.task_id, record.disposition]),
  );

  return (
    <aside
      aria-label="Cycle review"
      className="flex h-full w-panel shrink-0 flex-col overflow-hidden border-l border-light bg-content"
    >
      <header className="flex shrink-0 items-center justify-between gap-2 border-b border-light px-4 py-3">
        <h2 className="text-section-title font-bold text-primary">Cycle review</h2>
        <Button variant="ghost" size="compact" aria-label="Close review" onClick={onClose}>
          Close
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {reviewQuery.isPending ? (
          <p className="text-body text-secondary">Loading review…</p>
        ) : (
          <>
            {/* -- 事实段（只读） -------------------------------------- */}
            <section aria-label="Facts" className="mb-6">
              <div className="mb-2 flex items-baseline justify-between gap-2">
                <h3 className="text-block-title font-semibold text-primary">Facts</h3>
                {review ? (
                  <p className="text-caption text-hint">
                    Snapshotted {formatSnapshotAt(review.snapshot_at)}
                    {review.is_final ? "" : " · interim"}
                    {review.kind === "facts_only" ? " · facts only" : ""}
                  </p>
                ) : (
                  <p className="text-caption text-hint">Not reviewed yet · so far</p>
                )}
              </div>
              {facts === null ? (
                <p className="text-body text-secondary">Loading facts…</p>
              ) : facts.has_content ? (
                <dl className="rounded-sm border border-light px-3 py-2 text-body text-primary">
                  <div className="flex justify-between gap-3 py-0.5">
                    <dt className="text-secondary">Completion</dt>
                    <dd>
                      {formatPercent(facts.completion_rate ?? 0)} (
                      {facts.completed_items}/{facts.total_items})
                    </dd>
                  </div>
                  <div className="flex justify-between gap-3 py-0.5">
                    <dt className="text-secondary">Focused time</dt>
                    <dd>{formatFocusedTime(facts.focused_time_ms)}</dd>
                  </div>
                  <div className="flex justify-between gap-3 py-0.5">
                    <dt className="text-secondary">Linked lower-level items</dt>
                    <dd>{facts.linked_lower_items}</dd>
                  </div>
                  <div className="flex justify-between gap-3 py-0.5">
                    <dt className="text-secondary">Unfinished items</dt>
                    <dd>{facts.incomplete.length}</dd>
                  </div>
                </dl>
              ) : (
                /* 空周期：显式说明无可复盘内容，绝不显示 0% 造成误导 */
                <p className="rounded-sm border border-dashed border-light px-3 py-3 text-body text-secondary">
                  This cycle has no reviewable content.
                </p>
              )}
            </section>

            {/* -- 未完成项去向（每条三个去向按钮） -------------------- */}
            {facts !== null && facts.incomplete.length > 0 && (
              <section aria-label="Unfinished items" className="mb-6">
                <h3 className="mb-2 text-block-title font-semibold text-primary">
                  Unfinished items
                </h3>
                <ul className="flex flex-col gap-2">
                  {facts.incomplete.map((item) => {
                    const recorded = recordedByTask.get(item.task_id);
                    return (
                      <li
                        key={item.task_id}
                        className="rounded-sm border border-light px-3 py-2"
                      >
                        <p className="text-body text-primary">{item.title}</p>
                        <div className="mt-1.5 flex items-center gap-2">
                          {DISPOSITION_BUTTONS.map(({ key, label, title }) => (
                            <Button
                              key={key}
                              size="compact"
                              variant={recorded === key ? "primary" : "secondary"}
                              aria-pressed={recorded === key}
                              title={title}
                              loading={
                                dispose.isPending &&
                                dispose.variables?.taskId === item.task_id &&
                                dispose.variables?.disposition === key
                              }
                              disabled={dispose.isPending}
                              onClick={() => dispose.mutate({ taskId: item.task_id, disposition: key })}
                            >
                              {label}
                            </Button>
                          ))}
                          {recorded && (
                            <span className="text-caption text-secondary">
                              {recorded === "carry" && "Carried to next cycle"}
                              {recorded === "later" && "Moved to Do Later"}
                              {recorded === "drop" && "Dropped"}
                            </span>
                          )}
                        </div>
                      </li>
                    );
                  })}
                </ul>
                {dispose.isError && (
                  <p role="alert" className="mt-2 text-menu text-danger">
                    Could not record that outcome. Try again.
                  </p>
                )}
              </section>
            )}

            {/* -- 判断段（可编辑） ------------------------------------ */}
            <section aria-label="Answers" className="mb-6">
              <h3 className="mb-2 text-block-title font-semibold text-primary">Your take</h3>
              <div className="flex flex-col gap-4">
                {REVIEW_QUESTIONS.map((question) => {
                  const entry = draft[question.id];
                  return (
                    <div key={question.id}>
                      <div className="mb-1 flex items-baseline justify-between gap-2">
                        <label
                          htmlFor={`review-${question.id}`}
                          className="text-menu font-medium text-primary"
                        >
                          {question.text}
                        </label>
                        <Button
                          size="compact"
                          variant="ghost"
                          aria-pressed={entry?.skipped ?? false}
                          onClick={() => toggleSkip(question.id)}
                        >
                          {entry?.skipped ? "Restore" : "Skip"}
                        </Button>
                      </div>
                      <textarea
                        id={`review-${question.id}`}
                        value={entry?.text ?? ""}
                        onChange={(event) => update(question.id, event.target.value)}
                        rows={2}
                        placeholder="Answer in your own words, or skip"
                        className="w-full resize-y rounded-sm border border-light bg-content px-2 py-1.5 text-body text-primary placeholder:text-hint focus:border-control focus:outline-none"
                      />
                      {entry?.skipped && (
                        <p className="mt-0.5 text-caption text-hint">
                          Will be saved as skipped{entry.text.trim() ? " with your note" : ""}.
                        </p>
                      )}
                    </div>
                  );
                })}
              </div>
            </section>
          </>
        )}
      </div>

      <footer className="shrink-0 border-t border-light px-4 py-3">
        <div className="flex items-center gap-2">
          <Button
            variant="primary"
            size="md"
            loading={save.isPending}
            onClick={() => save.mutate()}
          >
            Save review
          </Button>
          <Button
            variant="secondary"
            size="md"
            disabled={review === null}
            title="Copy this review as Markdown"
            loading={exportMarkdown.isPending}
            onClick={() => exportMarkdown.mutate()}
          >
            Copy Markdown
          </Button>
        </div>
        <p className="mt-1.5 min-h-4 text-caption text-secondary" role="status">
          {save.isSuccess && !save.isPending
            ? "Saved. You can keep editing and save again."
            : (exportNote ?? "")}
        </p>
        {save.isError && (
          <p role="alert" className="text-caption text-danger">
            Saving failed. Your draft is kept.
          </p>
        )}
      </footer>
    </aside>
  );
}
