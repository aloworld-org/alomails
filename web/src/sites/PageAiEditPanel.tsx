import { useState } from "react";
import { Sparkles } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { kindLabel } from "./sectionInfo";
import type { SectionsEnvelope } from "./sections";
import type { SiteEditEnvelope, SiteEditOperation } from "./types";
import styles from "./SitesModule.module.css";

function operationLabel(operation: SiteEditOperation): string {
  switch (operation.op) {
    case "add_section":
      return strings.sitesAiAddChange(kindLabel(operation.section.type), operation.at + 1);
    case "remove_section":
      return strings.sitesAiRemoveChange(kindLabel(operation.target.type));
    case "reorder_section":
      return strings.sitesAiMoveChange(kindLabel(operation.target.type), operation.to + 1);
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
      const prepared = await api.proposePageEdit(siteId, pageId, instruction.trim());
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
    <section className={styles.aiEditPanel} aria-labelledby="sites-ai-edit-title">
      <div className={styles.aiEditHeading}>
        <Sparkles aria-hidden="true" />
        <div>
          <h3 id="sites-ai-edit-title">{strings.sitesAiEditTitle}</h3>
          <p>{strings.sitesAiEditBody}</p>
        </div>
      </div>
      {error !== null && <p className={styles.aiEditError} role="alert">{error}</p>}
      {proposal === null ? (
        <div className={styles.aiEditComposer}>
          <input
            id="sites-ai-instruction"
            className={styles.input}
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
        <div className={styles.aiProposal} aria-live="polite">
          <h4>{strings.sitesAiProposalCount(proposal.operations.length)}</h4>
          <p className={styles.aiProposalHint}>{strings.sitesAiPreviewHint}</p>
          <ol>
            {proposal.operations.map((operation, index) => (
              <li key={`${operation.op}-${index}`}>{operationLabel(operation)}</li>
            ))}
          </ol>
          <div className={styles.aiProposalActions}>
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
