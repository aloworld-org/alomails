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
import { Briefcase, CopyPlus, FolderKanban, Play, Star } from "lucide-react";

import { Button, IconButton, Spinner } from "../ds";
import { strings } from "../i18n";
import { amountLabel, dayLabel, durationLabel, rateLabel } from "./format";
import { BudgetBar, EmptyState } from "./parts";
import type { Project } from "./types";

export function ProjectsView({
  projects,
  loading,
  customerName,
  isTemplate,
  onEditClient,
  onStartTimer,
  onToggleTemplate,
  onNewFromTemplate,
}: {
  projects: Project[];
  loading: boolean;
  /** The customer's own name for an id, or `null` while the list is loading or
   *  when the customer is one this reader cannot see. Resolved by the caller,
   *  which owns the billing read. */
  customerName: (customerId: string) => string | null;
  /** Whether this board is already marked reusable. */
  isTemplate: (projectId: string) => boolean;
  onEditClient: (project: Project) => void;
  onStartTimer: (project: Project) => void;
  /** Marks the board reusable, or takes the mark off — the same control, because
   *  a board either is a template or is not. */
  onToggleTemplate: (project: Project) => void;
  onNewFromTemplate: () => void;
}) {
  if (projects.length === 0) {
    return loading ? (
      <div className="flex min-h-0 flex-1 items-center justify-center p-8">
        <Spinner size={20} />
      </div>
    ) : (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-6">
        <EmptyState
          Icon={Briefcase}
          title={strings.projectsEmptyTitle}
          body={strings.projectsEmptyBody}
        />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 pb-8 pt-4 max-sm:px-3">
      <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
        <div className="flex items-center justify-between gap-4 border-b border-subtle px-5 py-4 max-sm:items-start">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg--soft text-accent">
              <FolderKanban className="size-5" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h2 className="m-0 text-base font-semibold text-primary">
                {strings.projectsTabList}
              </h2>
              <p className="m-0 mt-0.5 text-sm text-secondary">
                {projects.length}{" "}
                {projects.length === 1
                  ? strings.projectsProject.toLowerCase()
                  : strings.projectsTabList.toLowerCase()}
              </p>
            </div>
          </div>
          <Button
            icon={<CopyPlus size={16} />}
            className="h-10 shrink-0 rounded-xl px-4 text-sm font-semibold"
            onClick={onNewFromTemplate}
          >
            {strings.projectsTemplateNew}
          </Button>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[68rem] border-collapse text-sm">
          <thead>
            <tr className="bg-raised/60 text-left text-xs font-semibold uppercase tracking-wide text-tertiary">
              <th scope="col" className="px-5 py-3">{strings.projectsProject}</th>
              <th scope="col" className="px-4 py-3">{strings.projectsCustomer}</th>
              <th scope="col" className="px-4 py-3 text-right">
                {strings.projectsRate}
              </th>
              <th scope="col" className="px-4 py-3 text-right">
                {strings.projectsHoursLogged}
              </th>
              <th scope="col" className="px-4 py-3">{strings.projectsBudget}</th>
              <th scope="col" className="px-4 py-3">{strings.projectsLastWorked}</th>
              <th scope="col" className="px-5 py-3" aria-label={strings.projectsActions} />
            </tr>
          </thead>
          <tbody>
            {projects.map((project) => {
              const client = project.client;
              const customer = client === null ? null : customerName(client.customerId);
              return (
                <tr
                  key={project.id}
                  className="group border-t border-subtle transition-colors hover:bg-raised/50"
                >
                  <td className="px-5 py-4">
                    <button
                      type="button"
                      className="flex items-center gap-3 text-left text-sm font-semibold text-primary no-underline outline-none hover:text-accent focus-visible:rounded-md focus-visible:ring-2 focus-visible:ring-accent"
                      onClick={() => onEditClient(project)}
                    >
                      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg--soft text-accent">
                        <Briefcase className="size-4" aria-hidden="true" />
                      </span>
                      <span className="min-w-0 truncate">{project.name}</span>
                    </button>
                  </td>
                  <td className="px-4 py-4">
                    <span
                      className={
                        client === null
                          ? "inline-flex rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary"
                          : "font-medium text-primary"
                      }
                    >
                      {client === null
                        ? strings.projectsInternal
                        : (customer ?? strings.projectsCustomerUnknown)}
                    </span>
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-right font-medium tabular-nums text-primary">
                    {client === null ? "\u2014" : rateLabel(client.rateCents, client.currency)}
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-right font-medium tabular-nums text-primary">
                    {durationLabel(project.hours.minutes)}
                    {project.hours.billableMinutes !== project.hours.minutes && (
                      <div className="mt-1 text-xs font-normal text-tertiary">
                        {strings.projectsOfWhichBillable(
                          durationLabel(project.hours.billableMinutes),
                        )}
                      </div>
                    )}
                  </td>
                  <td className="min-w-40 px-4 py-4">
                    <BudgetBar
                      consumptionBp={project.hours.budgetConsumptionBp}
                      label={strings.projectsBudgetUsed}
                    />
                    {client !== null && client.budgetCents !== null && (
                      <span className="mt-1 block text-xs text-tertiary">
                        {amountLabel(client.budgetCents, client.currency)}
                      </span>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-secondary">
                    {project.hours.lastWorkedOn === null
                      ? strings.projectsNeverWorked
                      : dayLabel(project.hours.lastWorkedOn)}
                  </td>
                  <td className="px-5 py-4">
                    <div className="flex items-center justify-end gap-2">
                      <IconButton
                        label={strings.projectsStartTimerOn(project.name)}
                        icon={<Play size={16} />}
                        size="sm"
                        onClick={() => onStartTimer(project)}
                      />
                      {/* A personal board cannot be a template — the list of
                          templates is the whole workspace's — so the control is
                          absent there rather than offered and refused. */}
                      {project.kind === "team" && (
                        <IconButton
                          label={
                            isTemplate(project.id)
                              ? strings.projectsTemplateUnmarkOn(project.name)
                              : strings.projectsTemplateMarkOn(project.name)
                          }
                          icon={
                            <Star
                              size={16}
                              fill={isTemplate(project.id) ? "currentColor" : "none"}
                            />
                          }
                          size="sm"
                          active={isTemplate(project.id)}
                          onClick={() => onToggleTemplate(project)}
                        />
                      )}
                      <Button
                        variant={client === null ? "secondary" : "ghost"}
                        className="h-9 shrink-0 rounded-lg px-4 text-sm font-semibold"
                        onClick={() => onEditClient(project)}
                      >
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
      </section>
    </div>
  );
}
