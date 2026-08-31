import { strings } from "../i18n";
import { CircularCreateButton } from "./CircularCreateButton";

export function SectionInsertControl({
  disabled,
  expanded,
  onAdd,
}: {
  disabled: boolean;
  expanded: boolean;
  onAdd: () => void;
}) {
  return (
    <li className="flex justify-center py-3">
      <CircularCreateButton
        label={strings.sitesAddSection}
        expanded={expanded}
        sectionComposer
        disabled={disabled}
        onClick={onAdd}
      />
    </li>
  );
}
