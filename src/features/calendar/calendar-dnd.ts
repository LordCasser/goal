/**
 * Calendar-only HTML5 drag payloads.
 *
 * The payload intentionally contains an opaque token instead of an entity id.
 * Calendar drops resolve the id from the live drag state after validating that
 * token, so unrelated text drops and stale state cannot schedule or move data.
 */
export const DAY_DRAG_TYPE = "application/x-planner-calendar-day";
export const SESSION_DRAG_TYPE = "application/x-planner-focus-block";

export function createDragToken(): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export function writeDragToken(dataTransfer: DataTransfer, type: string, token: string): void {
  dataTransfer.setData(type, token);
  dataTransfer.effectAllowed = "move";
}

export function readDragToken(dataTransfer: DataTransfer, type: string): string | null {
  try {
    const token = dataTransfer.getData(type).trim();
    return token.length > 0 ? token : null;
  } catch {
    return null;
  }
}
