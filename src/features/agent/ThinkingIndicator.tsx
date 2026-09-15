// Adapted from UI by Halaska's ThinkingIndicator (MIT).
// Copyright (c) 2026 Halaska Studio. See public/licenses/halaska-ui.txt.
// Only the visual indicator is reused; pending state comes from the real request.
export function ThinkingIndicator({ label = "正在思考…" }: { label?: string }) {
  return (
    <p role="status" className="coach-thinking flex items-center gap-2.5 text-caption text-secondary">
      {label}
      <span className="inline-flex items-center gap-1" aria-hidden="true">
        {[0, 1, 2].map((index) => <span key={index} className="coach-thinking-dot" style={{ animationDelay: `${index * 150}ms` }} />)}
      </span>
    </p>
  );
}
