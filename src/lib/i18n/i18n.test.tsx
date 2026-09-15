import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { applyLocale, errorMessage, formatDate, formatDuration, i18n, initialLocale, matchLocale, resources, useTranslation } from ".";

const localStorageStub = {
  values: new Map<string, string>(),
  getItem(key: string) { return this.values.get(key) ?? null; },
  setItem(key: string, value: string) { this.values.set(key, value); },
  removeItem(key: string) { this.values.delete(key); },
  clear() { this.values.clear(); },
};

beforeEach(() => {
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: localStorageStub });
});

afterEach(() => { cleanup(); applyLocale("en"); localStorageStub.clear(); vi.unstubAllEnvs(); });

describe("bundled language contract", () => {
  it("ships both languages with the same namespaces, keys and interpolation parameters", () => {
    const english = resources.en!;
    const chinese = resources["zh-CN"]!;
    expect(Object.keys(chinese).sort()).toEqual(Object.keys(english).sort());
    const parameters = (message: string) => [...message.matchAll(/{{\s*([^},]+)[^}]*}}/g)].map(m => m[1]!).sort();
    for (const [namespace, catalog] of Object.entries(english)) {
      expect(Object.keys(chinese[namespace]!).sort(), namespace).toEqual(Object.keys(catalog).sort());
      for (const [key, message] of Object.entries(catalog)) {
        expect(message.trim(), `${namespace}:${key}`).not.toBe("");
        expect(parameters(chinese[namespace]![key]!), `${namespace}:${key}`).toEqual(parameters(message));
        // Product terminology applies to bundled copy, never to user content.
        expect(chinese[namespace]![key]!, `${namespace}:${key}`).not.toMatch(/时间块|\b(?:Later|Coach|Plan with AI)\b/i);
      }
    }
  });

  it("updates a mounted component without replacing user input or interpreting interpolated markup", async () => {
    function Example() {
      const { t } = useTranslation();
      return <><input aria-label="user input" defaultValue="设计 review" /><p>{t("error.detail", { detail: "<img src=x>" })}</p></>;
    }
    applyLocale("en");
    render(<Example />);
    const input = screen.getByRole("textbox");
    expect(screen.getByText("Could not complete the operation: <img src=x>")).toBeTruthy();
    act(() => applyLocale("zh-CN"));
    expect(screen.getByText("操作未完成：<img src=x>")).toBeTruthy();
    expect(screen.getByRole("textbox")).toBe(input);
    expect((input as HTMLInputElement).value).toBe("设计 review");
    expect(document.querySelector("img")).toBeNull();
    expect(document.documentElement.lang).toBe("zh-CN");
  });

  it("chooses a supported initial locale and ignores an invalid cache", () => {
    expect(matchLocale("zh-Hans-CN")).toBe("zh-CN");
    expect(matchLocale("en-GB")).toBe("en");
    expect(matchLocale("fr-FR")).toBe("en");
    applyLocale("zh-CN");
    expect(initialLocale()).toBe("zh-CN");
    localStorage.setItem("goal.locale", "fr");
    expect(initialLocale()).toBe(matchLocale(navigator.language));
  });

  it("formats display dates without UTC day shifts and quantities in either language", () => {
    vi.stubEnv("TZ", "America/Los_Angeles");
    applyLocale("en");
    expect(formatDate("2026-09-15", { day: "numeric" })).toBe("15");
    expect(formatDuration(90 * 60_000)).toBe("1 h 30 min");
    act(() => applyLocale("zh-CN"));
    expect(formatDuration(90 * 60_000)).toBe("1 小时 30 分钟");
    expect(i18n.t("notification.day", { ns: "backend", count: 1 })).toBe("今日计划已就绪，安排了 1 项事务。");
  });

  it("localizes error codes while keeping unknown diagnostics", () => {
    applyLocale("zh-CN");
    expect(errorMessage({ code: "invalid_locale", message: "internal copy" })).toBe("请选择简体中文或 English。");
    expect(errorMessage(new Error("EIO"))).toBe("操作未完成：EIO");
  });
});
