import { Streamdown, type Components, type StreamdownProps } from "streamdown";
import { useTranslation } from "../../lib/i18n";

// Keep Markdown parsing in Streamdown, while our own tokens control its appearance.
// No raw HTML processing or automatic remote media loading in model replies.
const rehypePlugins: NonNullable<StreamdownProps["rehypePlugins"]> = [];
const safeUrl: NonNullable<StreamdownProps["urlTransform"]> = (url) => /^(https?:|mailto:|#)/i.test(url) ? url : "";
const components: Components = {
  h1: "h1", h2: "h2", h3: "h3", h4: "h4", h5: "h5", h6: "h6",
  p: "p", strong: "strong", ul: "ul", ol: "ol", li: "li", hr: "hr",
  blockquote: "blockquote", code: "code",
  thead: "thead", tbody: "tbody", tr: "tr", th: "th", td: "td",
  img: ({ alt }) => <span className="text-secondary">{alt}</span>,
  a: ({ children, href }) => <span>{children}{href ? ` (${href})` : ""}</span>,
};

export function ChatMarkdown({ children, isStreaming = false }: { children: string; isStreaming?: boolean }) {
  const { t } = useTranslation("ai");
  const localizedComponents: Components = {
    ...components,
    pre: ({ children: content }) => <pre tabIndex={0} aria-label={t("agent.codeBlock")}>{content}</pre>,
    table: ({ children: content }) => (
      <div className="coach-table-scroll" role="region" aria-label={t("agent.replyTable")} tabIndex={0}>
        <table>{content}</table>
      </div>
    ),
  };
  return (
    <Streamdown
      className="coach-prose min-w-0 w-full max-w-full text-body text-primary"
      components={localizedComponents}
      rehypePlugins={rehypePlugins}
      urlTransform={safeUrl}
      skipHtml
      controls={false}
      mode="streaming"
      parseIncompleteMarkdown={isStreaming}
      isAnimating={isStreaming}
    >
      {children}
    </Streamdown>
  );
}
