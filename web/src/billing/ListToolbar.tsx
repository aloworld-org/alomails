import type { ReactNode } from "react";
import { Plus, Search } from "lucide-react";
import { Button, Input, Spinner, Toggle, Toolbar, ToolbarSpacer } from "../ds";
import { strings } from "../i18n";
import styles from "./billingStyles";

export function ListToolbar({ label, search, onSearch, searchLabel, includeArchived, onIncludeArchived, createLabel, onCreate, busy, showCreate = true, beforeCreate }: { label: string; search: string; onSearch: (value: string) => void; searchLabel: string; includeArchived: boolean; onIncludeArchived: (value: boolean) => void; createLabel: string; onCreate: () => void; busy: boolean; showCreate?: boolean; beforeCreate?: ReactNode }) {
  return <Toolbar label={label} className={styles.listBar}><label className={styles.searchWrap}><Input className="pr-10" type="search" value={search} onChange={(event) => onSearch(event.target.value)} placeholder={searchLabel} aria-label={searchLabel} /><Search aria-hidden="true" /></label><ToolbarSpacer /><Toggle checked={includeArchived} onChange={onIncludeArchived} label={strings.billingShowArchived} />{busy && <Spinner size={16} />}{beforeCreate}{showCreate && <Button icon={<Plus aria-hidden="true" />} onClick={onCreate}>{createLabel}</Button>}</Toolbar>;
}
