// The deal list: the same records the board holds, read as a table — the view
// for "my open deals on this board", for sorting by what a deal is worth, and
// for the closed ones a board deliberately stops showing.
//
// **The filters are the server's.** Column, owner and state go into the query
// and the answer is rendered whole; the browser narrows nothing, so what the
// server counted and what the user sees cannot disagree. An unknown column or
// state is a `422` there — which is why the selects only ever offer values the
// server just handed us. The one exception is the search box, which is plainly
// a text match over the rows already on screen and says so.
import { useMemo, useState } from "react";
import { Handshake } from "lucide-react";

import { useAuth } from "../auth";
import {
  Button,
  Checkbox,
  Input,
  Select,
  Spinner,
  Table,
  TableEmpty,
  Td,
  Th,
  Toolbar,
} from "../ds";
import { strings } from "../i18n";
import { dayLabel, dealValue } from "./format";
import { EmptyState, ErrorBanner, StateChip } from "./parts";
import { useDealList } from "./useCrmData";
import type { CrmDeal, CrmStage, DealState } from "./types";
import styles from "./CrmModule.module.css";

/** Whether a deal answers the search box (title, company, contact, source). */
function matches(deal: CrmDeal, needle: string): boolean {
  if (needle === "") return true;
  const hay = [
    deal.title,
    deal.companyName,
    deal.contactName,
    deal.contactEmail,
    deal.source,
  ]
    .join(" ")
    .toLowerCase();
  return hay.includes(needle);
}

/** The three states the server accepts, plus "any". */
const STATES: { value: DealState | ""; label: () => string }[] = [
  { value: "", label: () => strings.crmFilterAnyState },
  { value: "open", label: () => strings.crmStateOpen },
  { value: "won", label: () => strings.crmStateWon },
  { value: "lost", label: () => strings.crmStateLost },
];

interface Props {
  pipelineId: string | null;
  stages: CrmStage[];
  /** Bumped by an edit made elsewhere (the drawer), so the list re-reads. */
  revision: number;
  onOpen: (id: string) => void;
  onCreate: () => void;
}

export function ListView({
  pipelineId,
  stages,
  revision,
  onOpen,
  onCreate,
}: Props) {
  const { identity } = useAuth();
  const [stageId, setStageId] = useState("");
  const [state, setState] = useState<DealState | "">("");
  const [mine, setMine] = useState(false);
  const [search, setSearch] = useState("");

  // The owner filter is an exact user id — the OIDC subject IS the tenant's
  // user id — and the server answers an id that owns nothing with an empty
  // list rather than an error, so "mine" is safe to ask even on a new account.
  const ownerUserId = mine && identity !== null ? identity.sub : undefined;
  const { deals, loading, error } = useDealList(
    pipelineId,
    {
      ...(stageId === "" ? {} : { stageId }),
      ...(state === "" ? {} : { state }),
      ...(ownerUserId === undefined ? {} : { ownerUserId }),
    },
    revision,
  );

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return deals.filter((d) => matches(d, needle));
  }, [deals, search]);

  const stageName = (id: string) => stages.find((s) => s.id === id)?.name ?? "";

  return (
    <div className={styles.page}>
      <Toolbar label={strings.crmDealFilters}>
        {/* The search field takes the room the row has left, down to a width a
            deal title is still readable in. */}
        <Input
          className="flex-1 basis-[220px] min-w-[180px]"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.crmSearchDeals}
          aria-label={strings.crmSearchDeals}
        />
        <Select
          value={stageId}
          onChange={(e) => setStageId(e.target.value)}
          aria-label={strings.crmFilterStage}
        >
          <option value="">{strings.crmFilterAnyStage}</option>
          {stages.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </Select>
        <Select
          value={state}
          onChange={(e) => setState(e.target.value as DealState | "")}
          aria-label={strings.crmFilterState}
        >
          {STATES.map((s) => (
            <option key={s.value} value={s.value}>
              {s.label()}
            </option>
          ))}
        </Select>
        <Checkbox
          checked={mine}
          onChange={setMine}
          label={strings.crmFilterMine}
        />
        {loading && <Spinner size={16} />}
        <Button onClick={onCreate}>{strings.crmNewDeal}</Button>
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}

      {deals.length === 0 && !loading ? (
        <EmptyState
          Icon={Handshake}
          title={strings.crmNoDealsTitle}
          body={strings.crmNoDealsBody}
          cta={strings.crmNewDeal}
          onCta={onCreate}
        />
      ) : (
        // Every row's first cell opens the deal, so the row really does respond
        // to a click — which is what `interactiveRows` is for.
        <Table label={strings.crmDealsTable} interactiveRows>
          <thead>
            <tr>
              <Th>{strings.crmColDeal}</Th>
              <Th>{strings.crmColCompany}</Th>
              <Th>{strings.crmColStage}</Th>
              <Th numeric>{strings.crmColValue}</Th>
              <Th>{strings.crmColExpectedClose}</Th>
              <Th>{strings.crmColState}</Th>
            </tr>
          </thead>
          <tbody>
            {/* Inside the table rather than beside it: a "no matches" line in a
                sibling paragraph leaves anyone navigating by table in an empty
                grid with no explanation. */}
            {shown.length === 0 && !loading ? (
              <TableEmpty cols={6}>{strings.crmNoMatches}</TableEmpty>
            ) : (
              shown.map((deal) => (
                <tr key={deal.id}>
                  <Td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => onOpen(deal.id)}
                    >
                      {deal.title}
                    </button>
                  </Td>
                  <Td>{deal.companyName}</Td>
                  <Td>{stageName(deal.stageId)}</Td>
                  <Td numeric>{dealValue(deal)}</Td>
                  <Td>
                    {deal.expectedClose === null
                      ? ""
                      : dayLabel(deal.expectedClose)}
                  </Td>
                  <Td>
                    <StateChip state={deal.state} />
                  </Td>
                </tr>
              ))
            )}
          </tbody>
        </Table>
      )}
    </div>
  );
}
