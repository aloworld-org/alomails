// The record's agent, on the record (A8.4, ADR 0057/0058).
//
// One panel for every module's detail view, in three parts: where this record
// came from (its provenance, said in words); what its agent can do here (the
// registry's verbs that take this record, each a button that opens the
// agent's one-to-one with the words pre-filled — the panel itself never runs
// anything, ADR 0023); and a one-line ask answered in place.
//
// Quiet until asked (ADR 0057): the only call made on open is the directory
// read that renders the verbs — plus, when a thread origin arrives without a
// name, the one room read that lets the origin be cited by name. No
// summaries, no suggestions, nothing generated.
import { useEffect, useState } from "react";
import { Bot } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { ChatApi, chatMessage, useChatApi } from "../chat/api";
import { Button, Input, Spinner } from "../ds";
import { strings } from "../i18n";
import { useAgentsApi, type DirectoryAgent } from "./api";
import { verbsFor, type RecordOrigin, type RecordVerb } from "./recordAgent";

/** How long an ask will wait for the agent's answer before pointing at the
 *  conversation instead: 20 polls, 1.5 s apart. */
export const ASK_POLLS = 20;
export const ASK_POLL_MS = 1500;

/** The agent's first message after `afterSeq`, or `null` when none arrives
 *  within the patience above — the answer keeps landing in the room either
 *  way, so the panel links there rather than losing it. */
export async function waitForAgentReply(
  api: ChatApi,
  channel: string,
  afterSeq: number,
  polls: number = ASK_POLLS,
): Promise<string | null> {
  for (let attempt = 0; attempt < polls; attempt += 1) {
    const page = await api.messages(channel);
    const reply = page.find(
      (message) =>
        message.authorKind === "agent" &&
        message.seq > afterSeq &&
        message.kind === "text" &&
        message.body !== "",
    );
    if (reply !== undefined) return reply.body;
    await new Promise((resolve) => setTimeout(resolve, ASK_POLL_MS));
  }
  return null;
}

export function RecordAgentPanel({
  product,
  recordKind,
  recordId,
  recordLabel,
  origin,
  onBeforeNavigate,
}: {
  /** The module's word for itself, as the directory's `product` spells it. */
  product: string;
  /** The record's word (`task`, `invoice`, …) — part of the record's
   *  identity, and the phrasing hook for a module that needs its own. */
  recordKind: string;
  recordId: string;
  /** What to call the record when asking about it — its title, number or
   *  name, in the words the person already sees on screen. */
  recordLabel: string;
  /** Where the record came from, as its record view carries it; `null` when
   *  it does not say. */
  origin: RecordOrigin | null;
  /** Called before the panel navigates away (a verb or the conversation
   *  link) — the detail view's chance to close itself. */
  onBeforeNavigate?: () => void;
}) {
  const api = useChatApi();
  const agents = useAgentsApi();
  const navigate = useNavigate();
  // `undefined` while the directory is being read; `null` when this product
  // has no live agent — the panel then shows origin only, offering nothing
  // it cannot do.
  const [agent, setAgent] = useState<DirectoryAgent | null | undefined>(
    undefined,
  );
  // A thread origin's room name, read once when the origin arrived unnamed.
  const [threadName, setThreadName] = useState<string | null>(null);
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [answer, setAnswer] = useState<{
    channel: string;
    body: string | null;
  } | null>(null);
  const [busyVerb, setBusyVerb] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void agents
      .directory()
      .then((all) => {
        if (!alive) return;
        setAgent(
          all.find((each) => each.product === product && !each.disabled) ??
            null,
        );
      })
      .catch(() => {
        if (alive) setAgent(null);
      });
    return () => {
      alive = false;
    };
  }, [agents, product]);

  useEffect(() => {
    if (origin === null || origin.kind !== "thread" || origin.label !== null) {
      return undefined;
    }
    let alive = true;
    // Best-effort: a room the reader cannot open still has an origin — it is
    // just cited without its name.
    void api
      .channel(origin.id)
      .then((room) => {
        if (alive) setThreadName(room.name);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [api, origin]);

  function goTo(path: string) {
    onBeforeNavigate?.();
    navigate(path);
  }

  async function startVerb(verb: RecordVerb) {
    if (agent === null || agent === undefined || busyVerb !== null) return;
    setBusyVerb(verb.tool);
    setError(null);
    try {
      const room = await agents.openDm(agent.id);
      goTo(
        `/chat?channel=${encodeURIComponent(room.id)}&draft=${encodeURIComponent(
          verb.draft(recordLabel),
        )}`,
      );
    } catch (failure) {
      setError(chatMessage(failure, strings.recordAgentVerbFailed));
    } finally {
      setBusyVerb(null);
    }
  }

  async function ask() {
    const words = question.trim();
    if (words === "" || agent === null || agent === undefined || asking) return;
    setAsking(true);
    setError(null);
    setAnswer(null);
    try {
      const room = await agents.openDm(agent.id);
      const posted = await api.post(
        room.id,
        strings.recordAgentAskAbout(recordLabel, words),
      );
      setAnswer({ channel: room.id, body: null });
      const reply = await waitForAgentReply(api, room.id, posted.seq);
      setAnswer({ channel: room.id, body: reply });
      setQuestion("");
    } catch (failure) {
      setAnswer(null);
      setError(chatMessage(failure, strings.recordAgentAskFailed));
    } finally {
      setAsking(false);
    }
  }

  /** The origin said in words — one sentence per source kind, the label as
   *  the citation. An unknown kind still cites what it can rather than
   *  hiding a provenance the store took the trouble to keep. */
  function originSentence(from: RecordOrigin): string {
    const label = from.label ?? (from.kind === "thread" ? threadName : null);
    switch (from.kind) {
      case "person":
        return strings.recordAgentOriginPerson(label ?? from.id);
      case "thread":
        return label === null
          ? strings.recordAgentOriginThreadUnnamed
          : strings.recordAgentOriginThread(label);
      case "message":
        return strings.recordAgentOriginEmail;
      // A message's own provenance is who sent it — the one origin the mail
      // API does carry, and the one nobody has to be told twice.
      case "sender":
        return strings.recordAgentOriginSender(label ?? from.id);
      case "event":
        return strings.recordAgentOriginEvent;
      case "quote":
        return strings.recordAgentOriginQuote(label ?? from.id);
      case "import":
        return strings.recordAgentOriginImport(label ?? from.id);
      default:
        return strings.recordAgentOriginFrom(label ?? from.kind);
    }
  }

  /** Where the origin's own words link back to, when its source has a
   *  screen: the conversation a capture came from, the email a task was
   *  raised from. */
  function originPath(from: RecordOrigin): string | null {
    if (from.kind === "thread") {
      return `/chat?channel=${encodeURIComponent(from.id)}`;
    }
    if (from.kind === "message") {
      return `/mail?open=${encodeURIComponent(from.id)}`;
    }
    return null;
  }

  const verbs =
    agent === null || agent === undefined
      ? []
      : verbsFor(product, recordKind).filter((verb) =>
          agent.tools.some((tool) => tool.name === verb.tool),
        );
  const sourcePath = origin === null ? null : originPath(origin);

  return (
    <section
      aria-label={strings.recordAgentTitle}
      data-record={`${recordKind}:${recordId}`}
      className="flex flex-col gap-3 rounded-xl border border-subtle bg-surface p-4"
    >
      <header className="flex items-center gap-2">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent">
          <Bot size={17} aria-hidden="true" />
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="text-sm font-semibold text-primary">
            {strings.recordAgentTitle}
          </span>
          {agent !== null && agent !== undefined && (
            <span className="truncate text-xs text-tertiary">
              @{agent.handle}
            </span>
          )}
        </span>
      </header>

      <p className="m-0 text-sm text-secondary">
        {origin === null ? strings.recordAgentOriginNone : originSentence(origin)}
        {sourcePath !== null && (
          <>
            {" "}
            <button
              type="button"
              className="cursor-pointer border-0 bg-transparent p-0 text-sm font-medium text-accent hover:underline"
              onClick={() => goTo(sourcePath)}
            >
              {strings.recordAgentOpenSource}
            </button>
          </>
        )}
      </p>

      {verbs.length > 0 && agent !== null && agent !== undefined && (
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-tertiary">
            {strings.recordAgentCanDo(agent.handle)}
          </span>
          <div className="flex flex-wrap gap-2">
            {verbs.map((verb) => (
              <Button
                key={verb.tool}
                variant="ghost"
                size="sm"
                onClick={() => void startVerb(verb)}
                disabled={busyVerb !== null}
              >
                {verb.label()}
              </Button>
            ))}
          </div>
        </div>
      )}

      {agent !== null && agent !== undefined && (
        <form
          // Wraps rather than squeezing: the panel is 300px wide in a day
          // panel and full width in a drawer, and a one-line ask box clipped
          // mid-placeholder is not a field anybody trusts.
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void ask();
          }}
        >
          <Input
            className="min-w-[12rem] flex-1"
            value={question}
            onChange={(event) => setQuestion(event.target.value)}
            placeholder={strings.recordAgentAskPlaceholder(agent.handle)}
            aria-label={strings.recordAgentAskPlaceholder(agent.handle)}
            disabled={asking}
          />
          <Button
            type="submit"
            size="sm"
            className="shrink-0"
            disabled={question.trim() === "" || asking}
          >
            {strings.recordAgentAsk}
          </Button>
        </form>
      )}

      {asking && answer !== null && answer.body === null && (
        <p className="m-0 flex items-center gap-2 text-sm text-tertiary">
          <Spinner size={14} />
          {agent !== null && agent !== undefined
            ? strings.recordAgentAsking(agent.handle)
            : null}
        </p>
      )}

      {answer !== null && (answer.body !== null || !asking) && (
        <div className="flex flex-col gap-1 rounded-lg bg-raised p-3">
          <p className="m-0 whitespace-pre-wrap text-sm text-primary">
            {answer.body ?? strings.recordAgentNoAnswerYet}
          </p>
          <button
            type="button"
            className="w-fit cursor-pointer border-0 bg-transparent p-0 text-sm font-medium text-accent hover:underline"
            onClick={() =>
              goTo(`/chat?channel=${encodeURIComponent(answer.channel)}`)
            }
          >
            {strings.recordAgentOpenConversation}
          </button>
        </div>
      )}

      {error !== null && (
        <p className="m-0 text-sm text-danger" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
