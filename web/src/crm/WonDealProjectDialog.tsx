import { useState } from "react";
import { BriefcaseBusiness } from "lucide-react";

import { Field, Input } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { DialogFrame } from "./parts";
import type { CrmDeal, DealProject } from "./types";

interface Props {
  deal: CrmDeal;
  onClose: () => void;
  onCreated: (project: DealProject) => void;
}

/** Review-and-confirm handoff from a won opportunity into delivery. */
export function WonDealProjectDialog({ deal, onClose, onCreated }: Props) {
  const api = useCrmApi();
  const [name, setName] = useState(deal.title);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const project = await api.createProject(deal.id, {
        name: name.trim(),
        ...(deal.customerId === null ? {} : { customerId: deal.customerId }),
      });
      onCreated(project);
    } catch (reason) {
      setError(crmMessage(reason, strings.crmProjectCreateFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={BriefcaseBusiness}
      title={strings.crmProjectCreateTitle}
      subtitle={strings.crmProjectCreateSubtitle}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== ""}
      submitLabel={strings.crmProjectCreateConfirm}
      onClose={onClose}
      onSubmit={() => void create()}
    >
      <div className="rounded-xl border border-subtle bg-secondary p-4 text-sm text-secondary">
        {strings.crmProjectCreateSummary(deal.title)}
      </div>
      <Field label={strings.crmProjectName}>
        {(control) => (
          <Input
            {...control}
            value={name}
            maxLength={120}
            onChange={(event) => setName(event.target.value)}
          />
        )}
      </Field>
    </DialogFrame>
  );
}
