// The engagement list: every project this workspace can see, as client work —
// who it is for, at what rate, what it has cost in hours, and how much of its
// budget that is.
//
// It is the same list Tasks shows, in the same order, seen through a second
// lens: a project here IS the board over there (`docs/design/projects.md`, "One
// project list, extended"). Internal projects are listed too, greyed rather than
// hidden — "make this client work" is the action this screen exists to offer,
// and a project you cannot see is a project you cannot offer it for.
//
// Every figure is the server's. The hours are the project's aggregate — nobody
// is named, here or in the API — and the budget bar is drawn from basis points
// the server computed, so two people looking at one engagement see one bar.
import { Briefcase, Play } from "lucide-react";

import { Button, IconButton, Spinner } from "../ds";
import { strings } from "../i18n";
import { amountLabel, dayLabel, durationLabel, rateLabel } from "./format";
import { BudgetBar, EmptyState } from "./parts";
import type { Project } from "./types";
import styles from "./ProjectsModule.module.css";

export function ProjectsView({
  projects,
  loading,
  customerName,
  onEditClient,
  onStartTimer,
}: {
  projects: Project[];
  loading: boolean;
  /** The customer's own name for an id, or `null` while the list is loading or
   *  when the customer is one this reader cannot see. Resolved by the caller,
   *  which owns the billing read. */
  customerName: (customerId: string) => string | null;
  onEditClient: (project: Project) => void;
  onStartTimer: (project: Project) => void;
}) {
  if (projects.length === 0) {
    return loading ? (
      <div className={styles.page}>
        <Spinner size={20} />
      </div>
    ) : (
      <div className={styles.page}>
        <EmptyState
          Icon={Briefcase}
          title={strings.projectsEmptyTitle}
          body={strings.projectsEmptyBody}
        />
      </div>
    );
  }

  return (
    <div className={styles.page}>
      <div className={styles.tableWrap}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th scope="col">{strings.projectsProject}</th>
              <th scope="col">{strings.projectsCustomer}</th>
              <th scope="col" className={styles.numeric}>
                {strings.projectsRate}
              </th>
              <th scope="col" className={styles.numeric}>
                {strings.projectsHoursLogged}
              </th>
              <th scope="col">{strings.projectsBudget}</th>
              <th scope="col">{strings.projectsLastWorked}</th>
              <th scope="col" aria-label={strings.projectsActions} />
            </tr>
          </thead>
          <tbody>
            {projects.map((project) => {
              const client = project.client;
              const customer = client === null ? null : customerName(client.customerId);
              return (
                <tr key={project.id}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => onEditClient(project)}
                    >
                      {project.name}
                    </button>
                  </td>
                  <td className={client === null ? styles.internal : undefined}>
                    {client === null
                      ? strings.projectsInternal
                      : (customer ?? strings.projectsCustomerUnknown)}
                  </td>
                  <td className={styles.numeric}>
                    {client === null ? "" : rateLabel(client.rateCents, client.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {durationLabel(project.hours.minutes)}
                    {project.hours.billableMinutes !== project.hours.minutes && (
                      <div className={styles.muted}>
                        {strings.projectsOfWhichBillable(
                          durationLabel(project.hours.billableMinutes),
                        )}
                      </div>
                    )}
                  </td>
                  <td>
                    <BudgetBar
                      consumptionBp={project.hours.budgetConsumptionBp}
                      label={strings.projectsBudgetUsed}
                    />
                    {client !== null && client.budgetCents !== null && (
                      <span className={styles.muted}>
                        {amountLabel(client.budgetCents, client.currency)}
                      </span>
                    )}
                  </td>
                  <td className={styles.muted}>
                    {project.hours.lastWorkedOn === null
                      ? strings.projectsNeverWorked
                      : dayLabel(project.hours.lastWorkedOn)}
                  </td>
                  <td>
                    <div className={styles.rowActions}>
                      <IconButton
                        label={strings.projectsStartTimerOn(project.name)}
                        icon={<Play size={16} />}
                        size="sm"
                        onClick={() => onStartTimer(project)}
                      />
                      <Button variant="ghost" size="sm" onClick={() => onEditClient(project)}>
                        {client === null ? strings.projectsMakeClientWork : strings.projectsEdit}
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
