import { useState } from "react";
import { Sparkles } from "lucide-react";

import { Button, Input } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { kindLabel } from "./sectionInfo";
import type { SectionsEnvelope } from "./sections";
import type { SiteEditEnvelope, SiteEditOperation } from "./types";

function operationLabel(operation: SiteEditOperation): string {
  switch (operation.op) {
    case "add_section":
      return strings.sitesAiAddChange(
        kindLabel(operation.section.type),
        operation.at + 1,
      );
    case "remove_section":
      return strings.sitesAiRemoveChange(kindLabel(operation.target.type));
    case "reorder_section":
      return strings.sitesAiMoveChange(
        kindLabel(operation.target.type),
        operation.to + 1,
      );
    case "set_prop":
      return strings.sitesAiSettingChange(kindLabel(operation.target.type));
    case "rewrite_copy":
      return strings.sitesAiCopyChange(kindLabel(operation.target.type));
  }
}

export function PageAiEditPanel({
  siteId,
  pageId,
  onApplied,
  onPreviewChange,
}: {
  siteId: string;
  pageId: string;
  onApplied: (sections: SectionsEnvelope) => void;
  onPreviewChange: (html: string | null) => void;
}) {
  const api = useSitesApi();
  const [instruction, setInstruction] = useState("");
  const [proposal, setProposal] = useState<SiteEditEnvelope | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function propose() {
    setBusy(true);
    setError(null);
    try {
      const prepared = await api.proposePageEdit(
        siteId,
        pageId,
        instruction.trim(),
      );
      setProposal(prepared.proposal);
      onPreviewChange(prepared.previewHtml);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiEditFailed));
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    if (proposal === null) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.applyPageEdit(siteId, pageId, proposal);
      onApplied(result);
      onPreviewChange(null);
      setProposal(null);
      setInstruction("");
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiApplyFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      className="border-b border-subtle bg-raised/60 px-4 py-4"
      aria-labelledby="sites-ai-edit-title"
    >
      <div className="flex items-start gap-3">
        <span
          className="inline-flex size-9 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent"
          aria-hidden="true"
        >
          <Sparkles size={17} />
        </span>
        <div className="min-w-0">
          <h3
            id="sites-ai-edit-title"
            className="text-sm font-semibold text-primary"
          >
            {strings.sitesAiEditTitle}
          </h3>
          <p className="mt-0.5 text-xs leading-5 text-secondary">
            {strings.sitesAiEditBody}
          </p>
        </div>
      </div>
      {error !== null && (
        <p
          className="mt-3 rounded-lg bg-danger-tint px-3 py-2 text-sm text-primary"
          role="alert"
        >
          {error}
        </p>
      )}
      {proposal === null ? (
        <div className="mt-3 flex items-center gap-2 max-sm:flex-col max-sm:items-stretch">
          <Input
            id="sites-ai-instruction"
            className="min-w-0 flex-1"
            aria-label={strings.sitesAiInstruction}
            value={instruction}
            onChange={(event) => setInstruction(event.target.value)}
            placeholder={strings.sitesAiInstructionPlaceholder}
          />
          <Button
            disabled={busy || instruction.trim() === ""}
            onClick={() => void propose()}
          >
            {busy ? strings.sitesAiPreparing : strings.sitesAiPropose}
          </Button>
        </div>
      ) : (
        <div className="mt-3 border-t border-subtle pt-3" aria-live="polite">
          <h4 className="text-sm font-semibold text-primary">
            {strings.sitesAiProposalCount(proposal.operations.length)}
          </h4>
          <p className="mt-1 text-xs text-secondary">
            {strings.sitesAiPreviewHint}
          </p>
          <ol className="my-3 list-decimal space-y-1 pl-5 text-sm text-secondary">
            {proposal.operations.map((operation, index) => (
              <li key={`${operation.op}-${index}`}>
                {operationLabel(operation)}
              </li>
            ))}
          </ol>
          <div className="flex items-center justify-between gap-2 border-t border-subtle pt-3">
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => {
                setProposal(null);
                setError(null);
                onPreviewChange(null);
              }}
            >
              {strings.sitesAiDiscard}
            </Button>
            <Button disabled={busy} onClick={() => void apply()}>
              {busy ? strings.sitesAiApplying : strings.sitesAiApprove}
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
