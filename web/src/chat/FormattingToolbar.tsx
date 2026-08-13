import type { ReactNode } from "react";
import { Bold, Code, Italic, List, Quote, Sigma, SquareCode } from "lucide-react";

import { strings } from "../i18n";

export function FormattingToolbar({ wrap }: { wrap: (before: string, after: string, sample: string) => void }) {
  const tool = "flex size-8 shrink-0 items-center justify-center rounded-sm border-0 bg-transparent text-tertiary hover:bg-raised hover:text-primary focus-visible:outline-2 focus-visible:outline-accent";
  return <div className="absolute bottom-full left-3 z-20 mb-2 flex max-w-full items-center gap-1 overflow-x-auto rounded-lg border border-subtle bg-surface p-1 shadow-md" role="toolbar" aria-label={strings.chatFormatting}>
    <Item className={tool} label={strings.chatBold} title={`${strings.chatBold}  (Ctrl+B)`} onClick={() => wrap("**", "**", strings.chatFormatHint)}><Bold size={15} /></Item>
    <Item className={tool} label={strings.chatItalic} title={`${strings.chatItalic}  (Ctrl+I)`} onClick={() => wrap("_", "_", strings.chatFormatHint)}><Italic size={15} /></Item>
    <Item className={tool} label={strings.chatInlineCode} onClick={() => wrap("`", "`", "code")}><Code size={15} /></Item>
    <Item className={tool} label={strings.chatCodeBlock} onClick={() => wrap("```\n", "\n```", "code")}><SquareCode size={15} /></Item>
    <Item className={tool} label={strings.chatFormula} onClick={() => wrap("$", "$", "e^{i\\pi}+1=0")}><Sigma size={15} /></Item>
    <Item className={tool} label={strings.chatBulletList} onClick={() => wrap("\n- ", "", strings.chatFormatHint)}><List size={15} /></Item>
    <Item className={tool} label={strings.chatQuoteAction} onClick={() => wrap("\n> ", "", strings.chatFormatHint)}><Quote size={15} /></Item>
  </div>;
}

function Item({ className, label, title = label, onClick, children }: { className: string; label: string; title?: string; onClick: () => void; children: ReactNode }) {
  return <button type="button" className={className} onClick={onClick} aria-label={label} title={title}>{children}</button>;
}
