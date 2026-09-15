/**
 * 「添加模型」对话框（tasks §5.2、design D6，约 420px）。
 *
 * 输入/输出类型锁定为文本——ModelConfig 校验要求 input_types 必含 text、
 * output_types 只能为 ["text"]——因此展示为禁用勾选 + 锁定图标，不发可选
 * 交互；工具调用为唯一可改的能力开关，默认开启（design D1：用户声明而非
 * 运行时探测）。草稿在每次打开时重置，未保存的内容不残留。
 */
import { useEffect, useId, useState, type JSX } from "react";

import { Button, Checkbox, Dialog, Input } from "../../ui";
import type { ModelConfig } from "../../lib/ipc";

export type AddModelDialogProps = {
  open: boolean;
  onClose: () => void;
  /** 传入已通过本对话框校验的完整 ModelConfig；关闭对话框由调用方负责。 */
  onSave: (model: ModelConfig) => void;
};

/** 12px 细线锁形图标：字段被锁定、不可修改的视觉线索（不只靠禁用态灰度）。 */
function LockIcon(): JSX.Element {
  return (
    <svg
      viewBox="0 0 24 24"
      className="h-3 w-3 text-hint"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="5" y="10" width="14" height="10" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </svg>
  );
}

export function AddModelDialog({ open, onClose, onSave }: AddModelDialogProps): JSX.Element {
  const modelIdInputId = useId();
  const contextInputId = useId();
  const outputInputId = useId();

  const [modelId, setModelId] = useState("");
  const [contextWindow, setContextWindow] = useState("");
  const [maxOutput, setMaxOutput] = useState("");
  const [supportsTools, setSupportsTools] = useState(true);

  // 打开即重置：取消/关闭丢弃草稿，下一次打开从空值开始。
  useEffect(() => {
    if (!open) return;
    setModelId("");
    setContextWindow("");
    setMaxOutput("");
    setSupportsTools(true);
  }, [open]);

  // 镜像 config 层校验：model_id 非空、窗口/输出为正整数（task 1.3）。
  const contextValue = Number(contextWindow);
  const outputValue = Number(maxOutput);
  const contextInvalid = !Number.isInteger(contextValue) || contextValue <= 0;
  const outputInvalid = !Number.isInteger(outputValue) || outputValue <= 0;
  const missing: string[] = [];
  if (modelId.trim() === "") missing.push("模型 ID");
  if (contextInvalid) missing.push("上下文窗口（正整数）");
  if (outputInvalid) missing.push("最大输出 Token（正整数）");
  const canSave = missing.length === 0;

  const save = () => {
    if (!canSave) return;
    onSave({
      model_id: modelId.trim(),
      context_window: contextValue,
      max_output_tokens: outputValue,
      input_types: ["text"],
      output_types: ["text"],
      supports_tools: supportsTools,
    });
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="添加模型"
      footer={
        <>
          <Button variant="secondary" size="compact" onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" size="compact" disabled={!canSave} onClick={save}>
            保存
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <label htmlFor={modelIdInputId} className="text-caption text-secondary">
            模型 ID
          </label>
          <Input
            id={modelIdInputId}
            value={modelId}
            onChange={(e) => setModelId(e.currentTarget.value)}
            placeholder="如 llama3、claude-sonnet-4"
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={contextInputId} className="text-caption text-secondary">
            上下文窗口
          </label>
          <Input
            id={contextInputId}
            type="number"
            min={1}
            value={contextWindow}
            onChange={(e) => setContextWindow(e.currentTarget.value)}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={outputInputId} className="text-caption text-secondary">
            最大输出 Token
          </label>
          <Input
            id={outputInputId}
            type="number"
            min={1}
            value={maxOutput}
            onChange={(e) => setMaxOutput(e.currentTarget.value)}
          />
        </div>
        {/* 文本为协议必需能力，锁定展示（禁用勾选 + 锁形图标）。 */}
        <div className="flex items-center justify-between gap-3">
          <span className="text-caption text-secondary">输入类型</span>
          <span className="flex items-center gap-1">
            <Checkbox checked onChange={() => {}} disabled>
              <span className="text-body text-primary">文本</span>
            </Checkbox>
            <LockIcon />
          </span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-caption text-secondary">输出类型</span>
          <span className="flex items-center gap-1">
            <Checkbox checked onChange={() => {}} disabled>
              <span className="text-body text-primary">文本</span>
            </Checkbox>
            <LockIcon />
          </span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-caption text-secondary">工具调用</span>
          <Checkbox checked={supportsTools} onChange={setSupportsTools}>
            <span className="text-body text-primary">支持工具调用</span>
          </Checkbox>
        </div>
        {!canSave && (
          <p className="text-caption text-hint">还需完善：{missing.join("、")}</p>
        )}
      </div>
    </Dialog>
  );
}
