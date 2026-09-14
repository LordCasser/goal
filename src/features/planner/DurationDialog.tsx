/**
 * 长周期时长选择弹窗（任务 7.5，design.md §9.2「周期时长选择」）。
 *
 * 契约：1 / 3 / 6 产品月三选一；选项与日期后果同屏——右侧时间线由
 * deriveTimeline 纯前端推导，切换立即更新，不发任何请求；只有确认才调
 * createPlanningCycle（month 必须无 parent、duration_months 必传，见
 * service/cycles.rs 的校验）；取消/Escape 无副作用。today 取本地时区。
 */
import { useEffect, useMemo, useState } from "react";
import { Button, Dialog } from "../../ui";
import { createPlanningCycle } from "../../lib/ipc";
import { formatShortDate, todayISO } from "./dates";
import { deriveTimeline, type TimelineMonths } from "./timeline";
import { useActionError } from "./actions";

const OPTIONS: ReadonlyArray<TimelineMonths> = [1, 3, 6];

export function DurationDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  // 每次打开回到默认 3 月；today 在打开时取一次，避免弹窗开着跨零点跳日期。
  const [months, setMonths] = useState<TimelineMonths>(3);
  const [today, setToday] = useState(() => todayISO());
  const [creating, setCreating] = useState(false);
  const { error, run } = useActionError();

  useEffect(() => {
    if (open) {
      setMonths(3);
      setToday(todayISO());
    }
  }, [open]);

  const timeline = useMemo(() => deriveTimeline(today, months), [today, months]);

  const onConfirm = async () => {
    setCreating(true);
    const created = await run(() =>
      createPlanningCycle({ cycle_type: "month", duration_months: months, parent_id: null }),
    );
    setCreating(false);
    if (created) onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={() => {
        if (!creating) onClose();
      }}
      wide
      title="Long-term cycle"
      footer={
        <>
          <Button onClick={onClose} disabled={creating}>
            Cancel
          </Button>
          <Button variant="primary" loading={creating} onClick={() => void onConfirm()}>
            Create cycle
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-6 sm:flex-row">
        {/* 左侧：三个预设时长卡片。选中态用焦点色（4.1 导航选中），不用暖色。 */}
        <div role="radiogroup" aria-label="Cycle length" className="flex flex-1 flex-col gap-2">
          {OPTIONS.map((option) => {
            const selected = months === option;
            // 每个选项自带真实结束日预览，不让用户从标签推算（9.2）。
            const endsOn = deriveTimeline(today, option).nodes[2].date;
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setMonths(option)}
                className={[
                  "flex flex-col items-start gap-0.5 rounded-sm border px-4 py-3 text-left",
                  "transition-colors duration-100",
                  selected ? "border-focus bg-focus-surface" : "border-control hover:bg-hover",
                ].join(" ")}
              >
                <span className="text-body font-semibold text-primary">
                  {option} {option === 1 ? "month" : "months"}
                </span>
                <span className="text-caption text-hint">
                  {option * 4} weeks · ends {formatShortDate(endsOn)}
                </span>
              </button>
            );
          })}
        </div>
        {/* 右侧：实时时间线。日期后果直接可见，不能只写「6 个月」标签（9.2）。 */}
        <div className="flex-1 rounded-sm border border-light p-4">
          <ol className="flex flex-col gap-4">
            {timeline.nodes.map((node) => (
              <li key={node.key} data-timeline-node={node.key}>
                <p className="text-caption text-secondary">{node.label}</p>
                <p className="text-body font-medium text-primary">
                  {formatShortDate(node.date)} <span className="text-hint">{node.date}</span>
                </p>
              </li>
            ))}
          </ol>
          <p className="mt-4 border-t border-light pt-3 text-caption font-medium text-secondary">
            <span data-weeks-label>{timeline.weeks} weeks</span> in total
          </p>
        </div>
      </div>
      {error && (
        <p role="alert" className="mt-3 text-caption text-danger">
          {error}
        </p>
      )}
    </Dialog>
  );
}
