/**
 * Draft persistence for the review panel (spec: 关闭并重开 — 已填写内容仍在).
 *
 * A `useState` inside `ReviewPanel` would die with the unmount on close, and
 * the panel's owner lives outside this feature, so the draft lives in a
 * module-level store keyed by cycle id. It is deliberately in-memory: the
 * draft is uncommitted user input, the saved review is the durable record.
 */

import type { CycleReviewView, ReviewAnswer, ReviewQuestionId } from "./api";

export interface ReviewDraftEntry {
  /** The user's text; for a skipped answer this holds the recorded reason. */
  text: string;
  skipped: boolean;
}

/** question id → entry; absent keys are undecided (not yet saved). */
export type ReviewDraft = Partial<Record<ReviewQuestionId, ReviewDraftEntry>>;

const drafts = new Map<string, ReviewDraft>();

export function getDraft(cycleId: string): ReviewDraft | undefined {
  return drafts.get(cycleId);
}

export function setDraft(cycleId: string, draft: ReviewDraft): void {
  drafts.set(cycleId, draft);
}

/** Called once the review was saved — the backend is the record now. */
export function clearDraft(cycleId: string): void {
  drafts.delete(cycleId);
}

/** Initial draft from a saved review (or an empty one for a fresh panel). */
export function draftFromReview(review: CycleReviewView | null): ReviewDraft {
  if (!review) return {};
  const draft: ReviewDraft = {};
  for (const answer of review.answers) {
    draft[answer.id as ReviewQuestionId] = {
      text: answer.text,
      skipped: answer.status === "skipped",
    };
  }
  return draft;
}

/**
 * Draft → wire answers. Undecided questions are omitted (partial save);
 * skipped questions keep their reason text; answered questions need text.
 */
export function draftToAnswers(draft: ReviewDraft): ReviewAnswer[] {
  const answers: ReviewAnswer[] = [];
  for (const [id, entry] of Object.entries(draft)) {
    if (!entry) continue;
    if (entry.skipped) {
      answers.push({ id, status: "skipped", text: entry.text });
    } else if (entry.text.trim() !== "") {
      answers.push({ id, status: "answered", text: entry.text });
    }
  }
  return answers;
}
