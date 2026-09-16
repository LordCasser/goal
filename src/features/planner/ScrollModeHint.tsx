import { useTranslation } from "../../lib/i18n";

/** Local mouse-wheel status; keyboard focus keeps its own control outline. */
export function ScrollModeHint() {
  const { t } = useTranslation("planning");
  return (
    <span className="inline-flex h-3.5 w-3.5 shrink-0 items-center">
      <span data-workspace-scroll-hint hidden className="inline-flex cursor-default text-secondary" title={t("workspace.verticalScrollHelp")}>
        <svg className="h-3.5 w-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M8 2v12M5 5l3-3 3 3M5 11l3 3 3-3" />
        </svg>
        <span className="sr-only">{t("workspace.verticalScroll")}</span>
      </span>
    </span>
  );
}
