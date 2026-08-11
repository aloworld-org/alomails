// What the HR agent's card promises a reader (B6.09), proven by rendering the
// real cards over the shapes the server actually sends.
//
// The promise is unusual in that most of it is about what is NOT drawn: an
// absence carries no reason, no kind of leave and no note, because the layer
// behind it never loads one (`docs/design/hr.md` § "The absence layer"). A card
// that invented one — or that quietly turned two days apart into the span
// between them — would be saying something about a colleague that no record
// anywhere states, so both are tested here rather than trusted.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";

import { strings } from "../i18n";
import type { AgentActionDto, WhoIsOffResultDto } from "../jmap";
import { AgentActionCard } from "./AgentActionCard";
import { AgentResultCard } from "./AgentResultCard";

afterEach(cleanup);

/** A proposal, as the agent route returns one. */
function proposal(args: Record<string, unknown>): AgentActionDto {
  return { tool: "who_is_off", args, say: "Check who is off next week." };
}

/** An answer, as the execute route returns one. */
function answer(over: Partial<WhoIsOffResultDto> = {}): WhoIsOffResultDto {
  return {
    kind: "whoIsOff",
    from: "2026-08-10",
    to: "2026-08-16",
    daysInRange: 7,
    people: [],
    days: [],
    ...over,
  };
}

test("the proposal shows the days it will look at, and says what it changes", () => {
  render(
    <AgentActionCard
      action={proposal({ from: "2026-08-10", to: "2026-08-16" })}
      running={false}
      onApprove={() => undefined}
      onDiscard={() => undefined}
    />,
  );
  expect(screen.getByText(strings.agentActWhoIsOff)).toBeTruthy();
  // Both ends are on the card: approving "who is off" over the wrong week is
  // approving a different question.
  expect(screen.getByText(/10.*—.*16/u)).toBeTruthy();
  expect(screen.getByText(strings.agentWhoIsOffNote)).toBeTruthy();
});

test("one stated day is drawn as that day rather than as a range of one", () => {
  render(
    <AgentActionCard
      action={proposal({ from: "2026-08-10" })}
      running={false}
      onApprove={() => undefined}
      onDiscard={() => undefined}
    />,
  );
  expect(screen.queryByText(/—/u)).toBeNull();
});

test("nobody away is an answer, and it names the window it means it over", () => {
  render(<AgentResultCard result={answer()} />);
  expect(screen.getByText(strings.agentWhoIsOffNobody)).toBeTruthy();
  // The window sits beside the dates as an aside, so the match is on the text
  // rather than on a whole element of its own.
  expect(
    screen.getByText(new RegExp(strings.agentWhoIsOffDays(7), "u")),
  ).toBeTruthy();
  expect(screen.getByText(strings.agentWhoIsOffFooter)).toBeTruthy();
});

test("two days apart are drawn as two days, never as the span between them", () => {
  render(
    <AgentResultCard
      result={answer({
        people: [
          {
            employeeId: "e1",
            name: "Amara van den Berg",
            awayDays: 2,
            firstDay: "2026-08-10",
            lastDay: "2026-08-14",
          },
        ],
      })}
    />,
  );
  expect(screen.getByText("Amara van den Berg")).toBeTruthy();
  // The count the server sent, not five days inferred from the two ends.
  expect(screen.getByText(strings.agentWhoIsOffDays(2))).toBeTruthy();
  expect(screen.queryByText(strings.agentWhoIsOffDays(5))).toBeNull();
});

test("a single day off is drawn as the day itself, with no count to read twice", () => {
  render(
    <AgentResultCard
      result={answer({
        from: "2026-08-10",
        to: "2026-08-10",
        daysInRange: 1,
        people: [
          {
            employeeId: "e1",
            name: "Mikkel Sørensen",
            awayDays: 1,
            firstDay: "2026-08-10",
            lastDay: "2026-08-10",
          },
        ],
      })}
    />,
  );
  expect(screen.getByText(strings.agentWhoIsOffCount(1))).toBeTruthy();
  expect(screen.queryByText(strings.agentWhoIsOffDays(1))).toBeNull();
});

test("nothing on the card says why anybody is away", () => {
  // The disclosure rule, as a rendering test. A later hand that widened the DTO
  // with a policy, a kind or a note and drew it here fails this.
  const { container } = render(
    <AgentResultCard
      result={
        {
          ...answer({
            people: [
              {
                employeeId: "e1",
                name: "Amara van den Berg",
                awayDays: 3,
                firstDay: "2026-08-10",
                lastDay: "2026-08-12",
              },
            ],
          }),
          // Fields the server does not send and never will — if one ever
          // reached the card, it must still not be drawn.
          reason: "hospital appointment",
          policyName: "Sick leave",
          note: "back on Thursday",
        } as WhoIsOffResultDto
      }
    />,
  );
  const drawn = container.textContent ?? "";
  expect(drawn).toContain("Amara van den Berg");
  for (const secret of ["hospital", "Sick", "Thursday"]) {
    expect(drawn).not.toContain(secret);
  }
});
