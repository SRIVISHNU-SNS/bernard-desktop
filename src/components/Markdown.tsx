import { useCallback, useMemo, useState } from "react";
import type { ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { api } from "../lib/bridge";
import { CheckIcon, CopyIcon } from "../lib/icons";

function extractText(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (typeof node === "object" && "props" in node) {
    return extractText((node as { props?: { children?: ReactNode } }).props?.children);
  }
  return "";
}

function CodeBlock({
  code,
  language,
  children,
}: {
  code: string;
  language: string | null;
  children: ReactNode;
}) {
  const [copied, setCopied] = useState(false);

  const copy = useCallback(() => {
    void api
      .copyText(code)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1600);
      })
      .catch(() => setCopied(false));
  }, [code]);

  return (
    <div className="code-block">
      <div className="code-head">
        <span className="code-lang">{language ?? "code"}</span>
        <button type="button" className="code-copy" onClick={copy}>
          {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="code-pre">{children}</pre>
    </div>
  );
}

interface MarkdownProps {
  source: string;
  onLink?: (href: string) => void;
}

export function Markdown({ source, onLink }: MarkdownProps) {
  const components = useMemo(
    () => ({
      pre({ children }: { children?: ReactNode }) {
        const child = Array.isArray(children) ? children[0] : children;
        const props =
          typeof child === "object" && child !== null && "props" in child
            ? (child as { props?: { className?: string; children?: ReactNode } }).props
            : undefined;
        const className = props?.className ?? "";
        const match = /language-([\w+-]+)/.exec(className);
        const language = match?.[1] ?? null;
        const code = extractText(props?.children).replace(/\n$/, "");
        return (
          <CodeBlock code={code} language={language}>
            {children}
          </CodeBlock>
        );
      },
      code({ className, children }: { className?: string; children?: ReactNode }) {
        return <code className={className ?? "inline-code"}>{children}</code>;
      },
      a({ href, children }: { href?: string; children?: ReactNode }) {
        return (
          <a
            href={href}
            onClick={(event) => {
              event.preventDefault();
              if (href) onLink?.(href);
            }}
            title={href}
          >
            {children}
          </a>
        );
      },
      table({ children }: { children?: ReactNode }) {
        return (
          <div className="table-scroll">
            <table>{children}</table>
          </div>
        );
      },
    }),
    [onLink],
  );

  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[[rehypeHighlight, { detect: false, ignoreMissing: true }]]}
        components={components}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
