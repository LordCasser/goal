/**
 * 目标日期已有日计划时的策略选择弹窗（tasks.md §5.4：冲突时弹出策略选择；
 * spec：MUST NOT 静默覆盖或丢弃任何一天的数据——只有用户显式选择后才会
 * 发生合并或交换）。
 */
import { Button, Dialog } from "../../ui";
import type { MoveStrategy } from "./api";

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
  return (
    <Dialog
      open={choice !== null}
      onClose={onClose}
      title="This date already has a plan"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={() => onPick("swap")} disabled={busy || !choice}>
            Swap days
          </Button>
          <Button variant="primary" onClick={() => onPick("merge")} disabled={busy || !choice}>
            Merge into {choice?.targetDate ?? ""}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2 text-body text-secondary">
        <p>
          {choice ? (
            <>
              Moving <span className="font-medium text-primary">{choice.sourceDate}</span> onto{" "}
              <span className="font-medium text-primary">{choice.targetDate}</span>, which already
              has a day plan. Choose how to combine them — nothing is discarded silently.
            </>
          ) : null}
        </p>
        <p className="text-caption text-hint">
          Merge folds the source entries into the target day and removes the source column. Swap
          exchanges the two dates, each keeping its own content.
        </p>
      </div>
    </Dialog>
  );
}
