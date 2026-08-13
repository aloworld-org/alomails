import type { ReactNode } from "react";
import { Paperclip, Sigma, Sparkles, SquareCode, Users } from "lucide-react";

import { strings } from "../i18n";

export function ComposerShareMenu({
  onFile,
  onCode,
  onEquation,
  onMention,
  onAskAlo,
}: {
  onFile: () => void;
  onCode: () => void;
  onEquation: () => void;
  onMention: () => void;
  onAskAlo: () => void;
}) {
  return (
    <div className="absolute bottom-full left-0 z-30 mb-2 grid w-96 grid-cols-2 gap-1 rounded-lg border border-subtle bg-surface p-2 shadow-lg max-sm:w-72 max-sm:grid-cols-1" role="menu">
      <ShareItem icon={<Paperclip size={16} />} name={strings.chatShareFile} hint={strings.chatShareFileHint} onClick={onFile} />
      <ShareItem icon={<SquareCode size={16} />} name={strings.chatCodeBlock} hint={strings.chatCodeBlockHint} onClick={onCode} />
      <ShareItem icon={<Sigma size={16} />} name={strings.chatFormula} hint={strings.chatFormulaHint} onClick={onEquation} />
      <ShareItem icon={<Users size={16} />} name={strings.chatShareMention} hint={strings.chatShareMentionHint} onClick={onMention} />
      <ShareItem icon={<Sparkles size={16} />} name={strings.chatShareAsk} hint={strings.chatShareAskHint} onClick={onAskAlo} />
    </div>
  );
}

function ShareItem({ icon, name, hint, onClick }: { icon: ReactNode; name: string; hint: string; onClick: () => void }) {
  return (
    <button type="button" role="menuitem" className="flex min-h-14 items-start gap-3 rounded-md border-0 bg-transparent p-3 text-left hover:bg-raised focus-visible:outline-2 focus-visible:outline-accent" onClick={onClick}>
      <span className="mt-1 shrink-0 text-accent">{icon}</span>
      <span className="min-w-0"><strong className="block text-sm text-primary">{name}</strong><span className="mt-1 block text-xs leading-snug text-tertiary">{hint}</span></span>
    </button>
  );
}
