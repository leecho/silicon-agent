import { memo } from "react";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import type { Components, UrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";

// rehype 插件：删除「块级元素之间/边缘的只含空白文本节点」。markdown→hast 会在块级子元素
// 间留下 "\n" 格式化文本节点（<p>\n<p>、<li>\n<ol>、<li>\n<p> 等）。这些节点在 WKWebView
// 下被渲染成凭空的行盒，把段落/列表项撑出大片空白（Chrome 会折叠、无此问题）。从源头清掉，
// 段落、松散列表、嵌套列表在所有引擎都紧凑一致。只删「相邻块级元素或块容器边缘」的空白节点，
// 行内元素之间的空格（<strong>a</strong> <strong>b</strong> 里的空格）予以保留。
type HastNode = {
  type: string;
  tagName?: string;
  value?: string;
  children?: HastNode[];
};
const BLOCK_TAGS = new Set([
  "p", "div", "ul", "ol", "li", "blockquote", "pre",
  "h1", "h2", "h3", "h4", "h5", "h6", "hr",
  "table", "thead", "tbody", "tr", "td", "th",
]);
function rehypeTrimBlockWhitespace() {
  const isWs = (n?: HastNode) =>
    !!n && n.type === "text" && /^\s*$/.test(n.value ?? "");
  const isBlock = (n?: HastNode) =>
    !!n && n.type === "element" && !!n.tagName && BLOCK_TAGS.has(n.tagName);
  const walk = (node: HastNode) => {
    if (!node.children) return;
    const kids = node.children;
    const parentIsBlockContainer = !node.tagName || BLOCK_TAGS.has(node.tagName);
    node.children = kids.filter((c, i) => {
      if (!isWs(c)) return true;
      const prev = kids[i - 1];
      const next = kids[i + 1];
      if (isBlock(prev) || isBlock(next)) return false; // 夹在块级元素旁
      if ((prev === undefined || next === undefined) && parentIsBlockContainer)
        return false; // 块容器首/尾的纯空白
      return true; // 行内上下文里的空白保留
    });
    for (const child of node.children) walk(child);
  };
  return (tree: HastNode) => walk(tree);
}

interface MarkdownTextProps {
  value: string;
  className?: string;
  /** 弱化模式：正文用 foreground-muted（与工具概览同色），用于过程区次要内容；间距/行高与常规一致。 */
  muted?: boolean;
}

const baseComponents: Components = {
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
// 稳定的插件数组：避免每次渲染新建（react-markdown 会据此重跑管线）。
const REMARK_PLUGINS = [remarkGfm];
const REHYPE_PLUGINS = [rehypeTrimBlockWhitespace];

// memo：markdown 解析（remark/rehype 管线）较重。MessageFeed 每次流式 delta 都整体重渲染，
// 若不 memo 则会话内每个 markdown 块每帧都重解析 → 阻塞主线程。props（value/className/muted）
// 均为原始值，未变即跳过重解析，只有正在流式的块因 value 变化才重渲染。
export const MarkdownText = memo(function MarkdownText({
  value,
  className = "",
  muted = false,
}: MarkdownTextProps) {
  // 弱化模式仅改颜色（与工具概览同色，区分最终答案），间距/行高与常规一致；
  // md-muted 让标题继承弱化色、并对松散列表项做 WebKit 兜底（见 styles.css）。
  const color = muted ? "text-foreground-secondary md-muted" : "text-foreground";
  return (
    <div className={`min-w-0 break-words text-sm leading-7 ${color} ${className}`}>
      <ReactMarkdown
        components={baseComponents}
        remarkPlugins={REMARK_PLUGINS}
        rehypePlugins={REHYPE_PLUGINS}
        skipHtml
        urlTransform={urlTransform}
      >
        {value}
      </ReactMarkdown>
    </div>
  );
});
