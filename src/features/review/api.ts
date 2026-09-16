/**
 * Review-retrospective IPC layer (change: add-review-retrospective §6).
 *
 * Feature-local on purpose: the shared `src/lib/ipc.ts` is the baseline's
 * command surface, and this change keeps its additions scoped here until they
 * are folded back. Shapes mirror the Rust side 1:1 (snake_case keys,
 * `Option<T>` → `T | null`):
 *
 *   src-tauri/src/service/reviews.rs   CycleReviewFacts / CycleReviewView /
 *                                      AnswerOutcome / DispositionRecord /
 *                                      ReviewSummaryPoint / SaveReviewArgs
 *   src-tauri/src/commands/reviews.rs  the five command signatures
 */

import { invoke } from "@tauri-apps/api/core";

/** Command names, declared once (same convention as src/lib/ipc.ts). */
export const commands = {
  getCycleFacts: "get_cycle_facts",
  getCycleReview: "get_cycle_review",
  saveCycleReview: "save_cycle_review",
  applyReviewDisposition: "apply_review_disposition",
  getReviewSummary: "get_review_summary",
  exportCycleReviewMarkdown: "export_cycle_review_markdown",
  saveCycleReviewMarkdown: "save_cycle_review_markdown",
} as const;

/** One answer on the wire (`{ id, status: answered|skipped, text }`). */
export interface ReviewAnswer {
  id: string;
  status: "answered" | "skipped";
  text: string;
}

/** One unfinished leaf item (`service::reviews::IncompleteItem`). */
export interface IncompleteItem {
  task_id: string;
  title: string;
  cycle_id: string;
  cycle_type: "session" | "day" | "week" | "month" | "unknown";
}

/** The frozen facts snapshot (`service::reviews::CycleReviewFacts`). */
export interface CycleReviewFacts {
  cycle_id: string;
  /** False = 无可复盘内容: the UI must not render this as 0%. */
  has_content: boolean;
  total_items: number;
  completed_items: number;
  /** null exactly when has_content is false. */
  completion_rate: number | null;
  focused_time_ms: number;
  linked_lower_items: number;
  incomplete: IncompleteItem[];
}

/** `carry` | `later` | `drop` — the three spec outcomes. */
export type Disposition = "carry" | "later" | "drop";

/** One recorded outcome (`service::reviews::DispositionRecord`). */
export interface DispositionRecord {
  task_id: string;
  disposition: Disposition;
}

/** The saved review (`service::reviews::CycleReviewView`). */
export interface CycleReviewView {
  id: string;
  cycle_id: string;
  /** "full" | "facts_only" — 全部跳过标记为「仅事实」。 */
  kind: "full" | "facts_only";
  /** False for an interim (early) review of a still-running cycle. */
  is_final: boolean;
  facts: CycleReviewFacts;
  answers: ReviewAnswer[];
  dispositions: DispositionRecord[];
  snapshot_at: number;
  created_at: number;
  updated_at: number;
}

/** `service::reviews::SaveReviewArgs` — a partial answer set is legal. */
export interface SaveReviewArgs {
  cycle_id: string;
  answers: ReviewAnswer[];
}

/** One trend point (`service::reviews::ReviewSummaryPoint`). */
export interface ReviewSummaryPoint {
  cycle_id: string;
  cycle_title: string;
  cycle_type: string;
  kind: "full" | "facts_only";
  is_final: boolean;
  completion_rate: number | null;
  focused_time_ms: number;
  snapshot_at: number;
}

/**
 * The fixed question set, mirrored from
 * `src-tauri/src/service/reviews.rs::REVIEW_QUESTIONS` (same ids — they are
 * the wire keys on every answer; labels restated in the UI language).
 */
export const REVIEW_QUESTIONS = [
  { id: "what_went_well", text: "What went well this cycle?" },
  { id: "what_held_you_back", text: "What held you back?" },
  { id: "where_plan_diverged", text: "Where did the plan and reality diverge?" },
  { id: "one_change_next", text: "The one thing to change next cycle?" },
] as const;

export type ReviewQuestionId = (typeof REVIEW_QUESTIONS)[number]["id"];

// --- queries -----------------------------------------------------------------
//
// Reviews have no backend event (saving one changes no cycle/task data), so
// this feature invalidates its own keys after mutations.

export const reviewQk = {
  cycleFacts: (cycleId: string) => ["cycle-review-facts", cycleId] as const,
  cycleReview: (cycleId: string) => ["cycle-review", cycleId] as const,
  reviewSummary: () => ["review-summary"] as const,
};

/** Live「截至目前」facts of a cycle that has no saved review yet. */
export function getCycleFacts(cycle_id: string): Promise<CycleReviewFacts> {
  return invoke<CycleReviewFacts>(commands.getCycleFacts, { cycleId: cycle_id });
}

export function getCycleReview(cycle_id: string): Promise<CycleReviewView | null> {
  return invoke<CycleReviewView | null>(commands.getCycleReview, { cycleId: cycle_id });
}

export function saveCycleReview(args: SaveReviewArgs): Promise<CycleReviewView> {
  return invoke<CycleReviewView>(commands.saveCycleReview, { args });
}

export function applyReviewDisposition(
  cycle_id: string,
  task_id: string,
  disposition: Disposition,
): Promise<void> {
  return invoke<void>(commands.applyReviewDisposition, {
    cycleId: cycle_id,
    taskId: task_id,
    disposition,
  });
}

export function getReviewSummary(): Promise<ReviewSummaryPoint[]> {
  return invoke<ReviewSummaryPoint[]>(commands.getReviewSummary);
}

/** Markdown string; the UI decides where it goes (clipboard / save dialog). */
export function saveCycleReviewMarkdown(cycleId: string, targetPath: string): Promise<void> {
  return invoke(commands.saveCycleReviewMarkdown, { cycleId: cycleId, targetPath: targetPath });
}

export function exportCycleReviewMarkdown(cycle_id: string): Promise<string> {
  return invoke<string>(commands.exportCycleReviewMarkdown, { cycleId: cycle_id });
}
