import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import type { Components, UrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";

interface MarkdownTextProps {
  value: string;
  className?: string;
}

const components: Components = {
  h1: ({ children }) => (
    <h1 className="mb-2 mt-4 text-lg font-semibold leading-7 text-foreground first:mt-0">
      {children}
    </h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-4 text-base font-semibold leading-7 text-foreground first:mt-0">
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-2 mt-3 text-sm font-semibold leading-6 text-foreground first:mt-0">
      {children}
    </h3>
  ),
  p: ({ children }) => (
    <p className="my-2 whitespace-pre-wrap first:mt-0 last:mb-0">
      {children}
    </p>
  ),
  ul: ({ children }) => <ul className="my-2 list-disc space-y-1 pl-5">{children}</ul>,
  ol: ({ children }) => <ol className="my-2 list-decimal space-y-1 pl-5">{children}</ol>,
  li: ({ children }) => <li className="pl-1">{children}</li>,
  blockquote: ({ children }) => (
    <blockquote className="my-2 border-l-2 border-border pl-3 text-foreground-secondary">
      {children}
    </blockquote>
  ),
  code: ({ children, className }) => {
    const language = className?.replace(/^language-/, "");
    if (language) {
      return <code className={className}>{children}</code>;
    }
    return (
      <code className="rounded bg-muted px-1 py-0.5 text-[0.92em]">
        {children}
      </code>
    );
  },
  pre: ({ children }) => (
    <pre className="my-3 max-w-full overflow-x-auto rounded-md bg-muted px-3 py-2 text-[13px] leading-6 text-foreground">
      {children}
    </pre>
  ),
  a: ({ children, href }) => (
    <a
      className="text-primary underline underline-offset-2"
      href={href}
      rel="noreferrer"
      target="_blank"
    >
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="my-3 max-w-full overflow-x-auto rounded-sm border border-border bg-surface">
      <table className="w-full border-separate border-spacing-0 text-left text-[13px]">
        {children}
      </table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-r border-border-subtle bg-muted px-2 py-1 font-semibold text-foreground last:border-r-0">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-r border-t border-border-subtle px-2 py-1 last:border-r-0">
      {children}
    </td>
  ),
};

const urlTransform: UrlTransform = (url) => defaultUrlTransform(url);

export function MarkdownText({ value, className = "" }: MarkdownTextProps) {
  return (
    <div className={`min-w-0 break-words text-sm leading-7 text-foreground ${className}`}>
      <ReactMarkdown
        components={components}
        remarkPlugins={[remarkGfm]}
        skipHtml
        urlTransform={urlTransform}
      >
        {value}
      </ReactMarkdown>
    </div>
  );
}
