/**
 * 周期列头 / 专注块卡片的选项菜单（任务 7.4 的菜单部分）。
 *
 * 菜单项与禁用态严格跟随 service 层规则，UI 不发明额外约束：
 * - Finish 只有 started 状态可用（domain::cycle::transition：not_started
 *   直接 finish 会得到 cycle_not_started）。
 * - Delete 先取 getCycleDeletionPreview（design.md §9.2「删除影响确认」），
 *   guard_code 非空（past_cycle / has_started_session / not_latest_n /
 *   protected_container）时展示后端 message 并禁用确认。
 * - Start 需要先有时长（cycle_duration_required）；同日其他块运行中时界面
 *   先行禁用（design.md §7 一次只推进一个块）。
 * - Repeat 需要先有时长（repeat_duration_required）；模板编辑只作用于未来
 *   实例，编辑/停止入口只在 repeat_id 非空时出现。
 * - 标题/时长编辑只对 session 开放（update_cycle 的 cycle_immutable 规则），
 *   month/week/day 的列头标题保持静态。
 */
import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Button, Dialog, Input, Popover, PopoverItem } from "../../ui";
import {
  addRepeat,
  copyUncompletedFromPrevious,
  deletePlanningCycle,
  finishCycle,
  getCycleDeletionPreview,
  lifecycleOf,
  startCycle,
  stopRepeat,
  updateCycle,
  updateRepeat,
  type Cycle,
  type CycleDeletionPreview,
} from "../../lib/ipc";
import { errorMessage, invalidateCycles, invalidateTasks, useActionError } from "./actions";

const TYPE_NAME: Record<Cycle["type"], string> = {
  month: "long-term cycle",
  week: "week plan",
  day: "day plan",
  session: "focus block",
};

export function CycleOptionsMenu({
  cycle,
  /** session 专属：同一天是否已有别的块在运行（互斥提示）。 */
  runningElsewhere = false,
}: {
  cycle: Cycle;
  runningElsewhere?: boolean;
}) {
  const qc = useQueryClient();
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [repeatOpen, setRepeatOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const { error, run, dismiss } = useActionError();

  const state = lifecycleOf(cycle);
  const started = state === "started";
  const finished = state === "finished";
  const noDuration = cycle.duration === null || cycle.duration <= 0;

  const close = () => setOpen(false);
  const refresh = () => invalidateCycles(qc);

  const onCopy = async () => {
    close();
    const copied = await run(() => copyUncompletedFromPrevious(cycle.id));
    if (copied) {
      setNotice(
        copied.length > 0
          ? `Copied ${copied.length} uncompleted item${copied.length === 1 ? "" : "s"}`
          : "Nothing uncompleted in the previous cycle",
      );
      invalidateTasks(qc, cycle.id);
    }
  };

  const onFinish = async () => {
    close();
    if (await run(() => finishCycle(cycle.id))) refresh();
  };

  const onStart = async () => {
    close();
    if (await run(() => startCycle(cycle.id))) refresh();
  };

  const onRepeat = async () => {
    close();
    if (await run(() => addRepeat(cycle.id))) refresh();
  };

  const onStopRepeat = async () => {
    close();
    const repeatId = cycle.repeat_id;
    if (repeatId && (await run(() => stopRepeat(repeatId)))) refresh();
  };

  const message = error ?? notice;

  return (
    <span className="relative inline-flex shrink-0">
      <button
        ref={anchorRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`${cycle.title} options`}
        onClick={() => setOpen((o) => !o)}
        className="flex h-7 w-7 items-center justify-center rounded-sm text-secondary transition-colors duration-100 hover:bg-hover"
      >
        <svg viewBox="0 0 16 16" className="h-4 w-4" aria-hidden="true">
          <circle cx="3" cy="8" r="1.2" fill="currentColor" />
          <circle cx="8" cy="8" r="1.2" fill="currentColor" />
          <circle cx="13" cy="8" r="1.2" fill="currentColor" />
        </svg>
      </button>
      {/* 菜单关闭后错误/结果仍要可见：挂绝对定位气泡，不挤压列头布局。 */}
      {message && (
        <div
          role={error ? "alert" : "status"}
          className="absolute right-0 top-8 z-40 w-max max-w-[280px] rounded-sm border border-light bg-content px-3 py-2 text-caption shadow-[0_8px_24px_rgba(0,0,0,0.12)]"
        >
          <p className={error ? "text-danger" : "text-secondary"}>{message}</p>
          <button
            type="button"
            className="mt-1 text-hint underline"
            onClick={() => {
              dismiss();
              setNotice(null);
            }}
          >
            Dismiss
          </button>
        </div>
      )}
      <Popover open={open} onClose={close} anchorRef={anchorRef} label={`${cycle.title} options`}>
        {cycle.type === "session" && (
          <PopoverItem
            onSelect={() => {
              close();
              setRenameOpen(true);
            }}
          >
            Rename
          </PopoverItem>
        )}
        {(cycle.type === "week" || cycle.type === "day") && (
          <PopoverItem
            onSelect={() => void onCopy()}
            disabled={finished || !cycle.starts_on}
            title={
              finished
                ? "This page has ended"
                : !cycle.starts_on
                  ? "Copying needs a dated cycle"
                  : "Copy uncompleted items from the previous cycle"
            }
          >
            Copy uncompleted from previous
          </PopoverItem>
        )}
        {cycle.type === "session" && !finished && !started && (
          <PopoverItem
            onSelect={() => void onStart()}
            disabled={runningElsewhere || noDuration}
            title={
              runningElsewhere
                ? "Another focus block is running"
                : noDuration
                  ? "Set a duration before starting"
                  : "Start this focus block"
            }
          >
            Start
          </PopoverItem>
        )}
        {cycle.type === "session" && started && (
          <PopoverItem onSelect={() => void onFinish()} title="Stop and record the focused time">
            Stop
          </PopoverItem>
        )}
        {/* Finish 之外的所有周期类型共享同一条生命周期规则。 */}
        {cycle.type !== "session" && (
          <PopoverItem
            onSelect={() => void onFinish()}
            disabled={!started}
            title={started ? "Mark this cycle as finished" : "The cycle is not running"}
          >
            Finish
          </PopoverItem>
        )}
        {cycle.type === "session" && (
          <>
            <PopoverItem
              onSelect={() => void onRepeat()}
              disabled={noDuration}
              title={noDuration ? "A focus block needs a duration to repeat" : "Create a daily template from this block"}
            >
              Repeat every day
            </PopoverItem>
            {cycle.repeat_id && (
              <PopoverItem
                onSelect={() => {
                  close();
                  setRepeatOpen(true);
                }}
              >
                Edit repeat
              </PopoverItem>
            )}
          </>
        )}
        <PopoverItem
          onSelect={() => {
            close();
            setDeleteOpen(true);
          }}
          /* 危险操作用危险色文字（9.2）；inline 样式避开与默认 text-primary 的类冲突。 */
          style={{ color: "var(--color-danger)" }}
        >
          Delete
        </PopoverItem>
        {cycle.type === "session" && cycle.repeat_id && (
          <PopoverItem onSelect={() => void onStopRepeat()} style={{ color: "var(--color-danger)" }}>
            Stop repeat
          </PopoverItem>
        )}
      </Popover>
      {renameOpen && (
        <RenameSessionDialog cycle={cycle} open={renameOpen} onClose={() => setRenameOpen(false)} />
      )}
      {repeatOpen && cycle.repeat_id && (
        <EditRepeatDialog cycle={cycle} open={repeatOpen} onClose={() => setRepeatOpen(false)} />
      )}
      <DeleteCycleDialog cycle={cycle} open={deleteOpen} onClose={() => setDeleteOpen(false)} />
    </span>
  );
}

/** session 改名：标题必填、时长保持不变（update_cycle 的两个参数都必传）。 */
function RenameSessionDialog({
  cycle,
  open,
  onClose,
}: {
  cycle: Cycle;
  open: boolean;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [title, setTitle] = useState(cycle.title);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();

  useEffect(() => {
    if (open) setTitle(cycle.title);
  }, [open, cycle.title]);

  const save = async () => {
    const trimmed = title.trim();
    if (!trimmed) return; // 空 title 后端必拒（invalid_title），输入态先拦
    setSaving(true);
    const ok = await run(() => updateCycle(cycle.id, trimmed, cycle.duration));
    setSaving(false);
    if (ok) {
      invalidateCycles(qc);
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Rename focus block"
      footer={
        <>
          <Button onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={saving}
            disabled={!title.trim()}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <Input
        autoFocus
        value={title}
        aria-label="Focus block title"
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.nativeEvent.isComposing) void save();
        }}
      />
      {error && (
        <p role="alert" className="mt-2 text-caption text-danger">
          {error}
        </p>
      )}
    </Dialog>
  );
}

/** 编辑重复模板：只改模板行，未来实例生效（service/repeats.rs 语义）。 */
function EditRepeatDialog({
  cycle,
  open,
  onClose,
}: {
  cycle: Cycle;
  open: boolean;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [title, setTitle] = useState(cycle.title);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();
  const repeatId = cycle.repeat_id;

  useEffect(() => {
    if (open) setTitle(cycle.title);
  }, [open, cycle.title]);

  const save = async () => {
    const trimmed = title.trim();
    if (!trimmed || !repeatId) return;
    setSaving(true);
    const ok = await run(() => updateRepeat(repeatId, { title: trimmed }));
    setSaving(false);
    if (ok) {
      invalidateCycles(qc);
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Edit repeat"
      footer={
        <>
          <Button onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={saving}
            disabled={!title.trim()}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <p className="mb-3 text-caption text-hint">
        Edits the daily template — future instances pick it up, past ones stay unchanged.
      </p>
      <Input
        autoFocus
        value={title}
        aria-label="Repeat title"
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.nativeEvent.isComposing) void save();
        }}
      />
      {error && (
        <p role="alert" className="mt-2 text-caption text-danger">
          {error}
        </p>
      )}
    </Dialog>
  );
}

/**
 * 删除确认：先展示影响数量与保护规则，guard 命中时确认键禁用并显示后端
 * message（past_cycle / has_started_session / not_latest_n 等，文案由
 * service 层给出，前端不重复翻译）。
 */
function DeleteCycleDialog({
  cycle,
  open,
  onClose,
}: {
  cycle: Cycle;
  open: boolean;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [preview, setPreview] = useState<CycleDeletionPreview | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const { error, run, dismiss } = useActionError();

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setPreview(null);
    setLoadError(null);
    dismiss();
    setLoading(true);
    getCycleDeletionPreview(cycle.id).then(
      (p) => {
        if (cancelled) return;
        setPreview(p);
        setLoading(false);
      },
      (e: unknown) => {
        if (cancelled) return;
        setLoadError(errorMessage(e));
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [open, cycle.id, dismiss]);

  const onDelete = async () => {
    setDeleting(true);
    const ok = await run(() => deletePlanningCycle(cycle.id));
    setDeleting(false);
    if (ok) {
      invalidateCycles(qc);
      onClose();
    }
  };

  const blocked = preview?.guard_code != null;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Delete “${cycle.title || TYPE_NAME[cycle.type]}”`}
      footer={
        <>
          <Button onClick={onClose} disabled={deleting} autoFocus>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={deleting}
            disabled={loading || blocked || loadError !== null}
            /* 危险主操作：深红底白字；inline 覆盖 bg-primary，避开类序问题。 */
            style={{ backgroundColor: "var(--color-danger)" }}
            onClick={() => void onDelete()}
          >
            Delete {TYPE_NAME[cycle.type]}
          </Button>
        </>
      }
    >
      {loading && <p className="text-body text-hint">Loading impact…</p>}
      {loadError && (
        <p role="alert" className="text-body text-danger">
          {loadError}
        </p>
      )}
      {preview && blocked && (
        <p className="text-body text-danger" role="alert">
          {preview.guard_message}
        </p>
      )}
      {preview && !blocked && (
        <div className="flex flex-col gap-2">
          <p className="text-body text-primary">
            This deletes the {TYPE_NAME[cycle.type]} and everything inside it:
          </p>
          <ul className="flex flex-col gap-1 text-body text-secondary">
            <li>
              {preview.tasks} task{plural(preview.tasks)}
            </li>
            <li>
              {preview.descendant_cycles} nested cycle{plural(preview.descendant_cycles)}
            </li>
            {preview.started_sessions > 0 && (
              <li>
                {preview.started_sessions} started focus block{plural(preview.started_sessions)}
              </li>
            )}
          </ul>
        </div>
      )}
      {error && (
        <p role="alert" className="mt-3 text-caption text-danger">
          {error}
        </p>
      )}
    </Dialog>
  );
}

function plural(n: number): string {
  return n === 1 ? "" : "s";
}
