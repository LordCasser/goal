/**
 * 退出调查弹窗（openspec app-lifecycle「退出调查」）。
 *
 * 组件自含三条出路的后端调用：继续使用（continue_after_exit_poll，不退
 * 出）、提交并退出（submit_exit_poll → exit_after_exit_poll）、直接关闭
 * （dismiss_exit_poll，记录一次忽略）。弹窗打开时先 acknowledge 告知后端
 * 已展示。
 *
 * 接线边界（有意为之）：本组件只导出，不接入窗口关闭流程——退出拦截、
 * mark_exit_poll_listener_ready 的调用时机与真正退出由退出流程拥有方完成
 * （见 tasks.md §3.3/§3.4 与协调者交接清单）。
 */
import { useEffect, useRef, useState } from "react";
import { Button, Dialog } from "../../ui";
import { useTranslation } from "../../lib/i18n";
import {
  acknowledgeExitPollShown,
  continueAfterExitPoll,
  dismissExitPoll,
  exitAfterExitPoll,
  submitExitPoll,
} from "./api";

/** What the dialog reported back to the quit flow when it closed. */
export type ExitPollResolution = "submitted" | "dismissed" | "continued";

export interface ExitPollDialogProps {
  open: boolean;
  /** Called once, after the backend recorded whichever出路 the user took. */
  onClose: (resolution: ExitPollResolution) => void;
}

const REASONS: Array<{ value: string; label: string }> = [
  { value: "missing_feature", label: "onboarding.exit.reason.missingFeature" },
  { value: "too_complicated", label: "onboarding.exit.reason.tooComplicated" },
  { value: "did_not_stick", label: "onboarding.exit.reason.didNotStick" },
  { value: "other", label: "onboarding.exit.reason.other" },
];

export function ExitPollDialog({ open, onClose }: ExitPollDialogProps) {
  const { t } = useTranslation("shell");
  const [reason, setReason] = useState<string>("missing_feature");
  const [detail, setDetail] = useState("");
  const [rating, setRating] = useState<number | null>(null);
  const acknowledged = useRef(false);
  // 等待后端落盘期间禁用动作按钮，避免重复提交或双写状态机。
  const [settling, setSettling] = useState(false);

  useEffect(() => {
    if (open && !acknowledged.current) {
      acknowledged.current = true;
      void acknowledgeExitPollShown().catch(() => undefined);
    }
    if (!open) {
      acknowledged.current = false;
      setSettling(false);
    }
  }, [open]);

  const run = async (resolution: ExitPollResolution) => {
    if (settling) return;
    setSettling(true);
    try {
      if (resolution === "continued") {
        await continueAfterExitPoll();
      } else if (resolution === "dismissed") {
        await dismissExitPoll();
      } else {
        await submitExitPoll({
          rating,
          reason,
          detail: detail.trim() || null,
        });
        await exitAfterExitPoll();
      }
      onClose(resolution);
    } catch {
      // 后端状态写失败时不丢答案：保持弹窗与输入，用户可重试；调查绝不
      // 阻断退出——宿主可选择强制关闭（此版本无上报通道，仅本地暂存）。
      setSettling(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={() => void run("dismissed")}
      title={t("onboarding.exit.title")}
    >
      <div className="flex flex-col gap-4">
        <fieldset className="flex flex-col gap-2">
          <legend className="text-body font-medium text-primary">
            {t("onboarding.exit.reasonLegend")}
          </legend>
          {REASONS.map((option) => (
            <label
              key={option.value}
              className="flex items-center gap-2 text-body text-primary"
            >
              <input
                type="radio"
                name="exit-poll-reason"
                value={option.value}
                checked={reason === option.value}
                onChange={() => setReason(option.value)}
              />
              {t(option.label)}
            </label>
          ))}
        </fieldset>
        <label className="flex flex-col gap-1">
          <span className="text-body font-medium text-primary">
            {t("onboarding.exit.detailLabel")}
          </span>
          <textarea
            value={detail}
            onChange={(event) => setDetail(event.target.value)}
            rows={3}
            className="w-full rounded-sm border border-control bg-content px-2 py-1 text-[14px] text-primary placeholder:text-hint"
            placeholder={t("onboarding.exit.detailPlaceholder")}
          />
        </label>
        <fieldset className="flex items-center gap-2">
          <legend className="text-body font-medium text-primary">
            {t("onboarding.exit.ratingLegend")}
          </legend>
          {[1, 2, 3, 4, 5].map((value) => (
            <button
              key={value}
              type="button"
              className={
                rating === value
                  ? "h-8 w-8 rounded-sm bg-primary text-white"
                  : "h-8 w-8 rounded-sm border border-control text-primary hover:bg-hover"
              }
              aria-pressed={rating === value}
              onClick={() => setRating(rating === value ? null : value)}
            >
              {value}
            </button>
          ))}
        </fieldset>
      </div>
      <div className="mt-5 flex items-center justify-end gap-2 pb-1">
        <Button
          variant="secondary"
          disabled={settling}
          onClick={() => void run("continued")}
        >
          {t("onboarding.exit.keepUsing")}
        </Button>
        <Button
          variant="primary"
          loading={settling}
          onClick={() => void run("submitted")}
        >
          {t("onboarding.exit.submitQuit")}
        </Button>
      </div>
    </Dialog>
  );
}
