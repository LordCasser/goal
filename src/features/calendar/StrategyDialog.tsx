/**
 * 目标日期已有日计划时的策略选择弹窗（tasks.md §5.4：冲突时弹出策略选择；
 * spec：MUST NOT 静默覆盖或丢弃任何一天的数据——只有用户显式选择后才会
 * 发生合并或交换）。
 */
import { Button, Dialog } from "../../ui";
import type { MoveStrategy } from "./api";
import { useTranslation, formatDate } from "../../lib/i18n";

export interface StrategyChoice {
  sourceDayId: string;
  sourceDate: string;
  targetDate: string;
}

export function StrategyDialog({
  choice,
  busy = false,
  onPick,
  onClose,
}: {
  choice: StrategyChoice | null;
  busy?: boolean;
  /** merge 折入目标日；swap 互换两天日期身份。 */
  onPick: (strategy: Exclude<MoveStrategy, "move">) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation("planning");
  const sourceDate = choice ? formatDate(choice.sourceDate, { year: "numeric", month: "short", day: "numeric" }) : "";
  const targetDate = choice ? formatDate(choice.targetDate, { year: "numeric", month: "short", day: "numeric" }) : "";
  return (
    <Dialog
      open={choice !== null}
      onClose={onClose}
      title={t("calendar.planConflict")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("calendar.cancel")}
          </Button>
          <Button onClick={() => onPick("swap")} disabled={busy || !choice}>
            {t("calendar.swapDays")}
          </Button>
          <Button variant="primary" onClick={() => onPick("merge")} disabled={busy || !choice}>
            {t("calendar.mergeInto", { date: targetDate })}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2 text-body text-secondary">
        <p>
          {choice ? (
            <>
            {t("calendar.moving", { source: sourceDate, target: targetDate })}
            </>
          ) : null}
        </p>
        <p className="text-caption text-hint">
          {t("calendar.mergeHelp")}
        </p>
      </div>
    </Dialog>
  );
}
