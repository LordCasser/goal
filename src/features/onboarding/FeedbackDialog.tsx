/**
 * 反馈弹窗（openspec app-lifecycle「反馈与联系作者」）。
 *
 * 提交走 send_feedback——本版本无上报通道，后端本地暂存（staged_feedback
 * 队列，等待用户显式导出），因此"失败不丢内容"在存储层天然成立：任何
 * 拒绝（空内容、数据库错误）都保留原文并给出可重试的提示。支持 ID 即
 * 匿名标识，经 navigator.clipboard 复制并给出已复制反馈。
 */
import { useState } from "react";
import { Button, Dialog } from "../../ui";
import { errorMessage, useTranslation } from "../../lib/i18n";
import { isAppError } from "../../lib/ipc";
import { getTalkToFounderEligibility, sendFeedback } from "./api";

export interface FeedbackDialogProps {
  open: boolean;
  onClose: () => void;
}

type FeedbackStatus =
  | { kind: "idle" }
  | { kind: "sent" }
  | { kind: "error"; note: string };

export function FeedbackDialog({ open, onClose }: FeedbackDialogProps) {
  const { t } = useTranslation("shell");
  const [message, setMessage] = useState("");
  const [status, setStatus] = useState<FeedbackStatus>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const submit = async () => {
    if (submitting) return;
    if (!message.trim()) {
      setStatus({ kind: "error", note: t("onboarding.feedback.empty") });
      return;
    }
    setSubmitting(true);
    try {
      await sendFeedback(message);
      setStatus({ kind: "sent" });
      setMessage("");
    } catch (error) {
      setStatus({
        kind: "error",
        note:
          isAppError(error) && error.code === "empty_feedback"
            ? t("onboarding.feedback.empty")
            : errorMessage(error),
      });
    } finally {
      setSubmitting(false);
    }
  };

  const copySupportId = async () => {
    try {
      const eligibility = await getTalkToFounderEligibility();
      await navigator.clipboard.writeText(eligibility.support_id);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title={t("onboarding.feedback.title")}>
      {status.kind === "sent" ? (
        <div className="flex flex-col gap-4" data-testid="feedback-sent">
          <p className="text-body text-primary">
            {t("onboarding.feedback.sent")}
          </p>
          <div className="flex justify-end">
            <Button variant="primary" onClick={onClose}>
              {t("onboarding.feedback.done")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-body font-medium text-primary">
              {t("onboarding.feedback.prompt")}
            </span>
            <textarea
              value={message}
              onChange={(event) => {
                setMessage(event.target.value);
                if (status.kind === "error") setStatus({ kind: "idle" });
              }}
              rows={4}
              className="w-full rounded-sm border border-control bg-content px-2 py-1 text-[14px] text-primary placeholder:text-hint"
              placeholder={t("onboarding.feedback.placeholder")}
              aria-label={t("onboarding.feedback.label")}
            />
          </label>
          {status.kind === "error" && (
            <p role="alert" className="text-menu text-danger">
              {status.note}
            </p>
          )}
          <div className="flex items-center justify-between gap-2 pb-1">
            <button
              type="button"
              className="rounded-sm px-2 py-1 text-menu text-secondary hover:bg-hover hover:text-primary"
              onClick={() => void copySupportId()}
              aria-label={t("onboarding.feedback.copySupportId")}
            >
              {copied ? t("onboarding.feedback.supportIdCopied") : t("onboarding.feedback.copySupportId")}
            </button>
            <div className="flex gap-2">
              <Button variant="ghost" onClick={onClose}>
                {t("onboarding.feedback.cancel")}
              </Button>
              <Button
                variant="primary"
                loading={submitting}
                onClick={() => void submit()}
              >
                {t("onboarding.feedback.send")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </Dialog>
  );
}
