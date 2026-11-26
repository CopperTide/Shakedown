import type { Components } from 'react-markdown';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { cn } from '@/lib/cn';

/**
 * An agent's or a model's text as Markdown (GitHub-flavoured: lists, tables, code, bold).
 * Raw HTML in the text is never rendered (react-markdown's default), so model output can't
 * put markup or scripts on the page; links open in a new tab without a referrer.
 */
const parts: Components = {
  p: ({ children }) => <p className="my-1.5 first:mt-0 last:mb-0">{children}</p>,
  ul: ({ children }) => <ul className="my-1.5 list-disc pl-5">{children}</ul>,
  ol: ({ children }) => <ol className="my-1.5 list-decimal pl-5">{children}</ol>,
  li: ({ children }) => <li className="my-0.5">{children}</li>,
  h1: ({ children }) => <p className="mt-3 mb-1 font-semibold">{children}</p>,
  h2: ({ children }) => <p className="mt-3 mb-1 font-semibold">{children}</p>,
  h3: ({ children }) => <p className="mt-2 mb-1 font-semibold">{children}</p>,
  h4: ({ children }) => <p className="mt-2 mb-1 font-semibold">{children}</p>,
  strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
  a: ({ href, children }) => <a href={href} target="_blank" rel="noreferrer noopener" className="text-accent underline underline-offset-2">{children}</a>,
  blockquote: ({ children }) => <blockquote className="my-1.5 border-l-2 border-line pl-3 text-muted">{children}</blockquote>,
  code: ({ className, children }) => <code className={cn('rounded bg-sunken px-1 font-mono text-[0.85em]', className)}>{children}</code>,
  pre: ({ children }) => <pre className="my-1.5 overflow-x-auto rounded bg-sunken p-2 font-mono text-xs [&_code]:bg-transparent [&_code]:p-0">{children}</pre>,
  table: ({ children }) => <div className="my-1.5 overflow-x-auto"><table className="text-xs [&_td]:border [&_td]:border-line [&_td]:px-1.5 [&_th]:border [&_th]:border-line [&_th]:px-1.5 [&_th]:text-left">{children}</table></div>,
  hr: () => <hr className="my-2 border-line" />,
};

export default function Markdown({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn('break-words text-sm leading-relaxed', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={parts}>{text}</ReactMarkdown>
    </div>
  );
}
