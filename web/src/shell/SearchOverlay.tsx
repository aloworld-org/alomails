// Workspace search + AI (ADR 0029) — a global command-palette-style overlay.
// Typing searches across the caller's files, tasks, and mail (names + content);
// "Ask AI" answers a question from those same access-scoped results and cites
// its sources. Opened from the rail or Ctrl/Cmd-K; a result (or a citation)
// navigates to its module and opens it.
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  FileText,
  Folder,
  File as FileIcon,
  ListChecks,
  Mail,
  Search,
  Sparkles,
  Table2,
  X,
  type LucideIcon,
} from "lucide-react";

import { strings } from "../i18n";
import {
  useJmapClient,
  type AgentAnswerDto,
  type AgentResultDto,
  type SearchHitDto,
} from "../jmap";
import { surface } from "../product";
import { Spinner } from "../ds";
import { AgentActionCard } from "./AgentActionCard";
import { AgentResultCard } from "./AgentResultCard";
import styles from "./SearchOverlay.module.css";

type ExecState = "idle" | "running" | "done" | "error";

function hitIcon(kind: string): LucideIcon {
  switch (kind) {
    case "folder":
      return Folder;
    case "doc":
      return FileText;
    case "base":
      return Table2;
    case "task":
      return ListChecks;
    case "message":
      return Mail;
    default:
      return FileIcon;
  }
}

/** The rail module a hit opens in — Drive for any file kind, else its own. */
function moduleFor(kind: string): string {
  if (kind === "task") return "tasks";
  if (kind === "message") return "mail";
  return "drive";
}

/**
 * A hit is shown only when the active product surface actually mounts the module
 * that opens it: the standalone Drive app (app.alodrives.com) has no mail or
 * tasks module, so those hits must never appear there (clicking would 404).
 */
function surfaceHasModule(id: string): boolean {
  return surface.modules.some((m) => m.id === id && m.enabled);
}

/** Keeps only the hits this product can actually open (a Drive-only app has no
 * mail/tasks module, so those would navigate nowhere). */
function openable(hits: SearchHitDto[]): SearchHitDto[] {
  return hits.filter((hit) => surfaceHasModule(moduleFor(hit.kind)));
}

export function SearchOverlay({ onClose }: { onClose: () => void }) {
  const client = useJmapClient();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<SearchHitDto[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [answer, setAnswer] = useState<AgentAnswerDto | null>(null);
  const [asking, setAsking] = useState(false);
  const [exec, setExec] = useState<ExecState>("idle");
  // What the executed action produced. Kept because two tools (B3.10a) answer
  // with something worth reading — a suggested timesheet entry, a project's
  // figures — and "Done." would throw it away.
  const [outcome, setOutcome] = useState<AgentResultDto | null>(null);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    // A new query invalidates any prior AI answer/proposed action.
    setAnswer(null);
    setExec("idle");
    setOutcome(null);
    if (q.trim() === "") {
      setHits(null);
      setLoading(false);
      return undefined;
    }
    setLoading(true);
    if (timer.current !== null) window.clearTimeout(timer.current);
    let live = true;
    timer.current = window.setTimeout(() => {
      void client
        .search(q)
        .then((h) => live && setHits(openable(h)))
        .catch(() => live && setHits([]))
        .finally(() => live && setLoading(false));
    }, 200);
    return () => {
      live = false;
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [q, client]);

  function ask() {
    const question = q.trim();
    if (question === "" || asking) return;
    setAsking(true);
    setAnswer(null);
    setExec("idle");
    setOutcome(null);
    void client
      .askAgent(question)
      .then((a) => setAnswer({ ...a, sources: openable(a.sources) }))
      .catch(() =>
        setAnswer({
          answer: null,
          action: null,
          reason: "unreachable",
          sources: [],
        }),
      )
      .finally(() => setAsking(false));
  }

  /** Run the proposed action — the only path that acts, and only on the user's
   *  approval (propose-then-approve, ADR 0034). On success the action card is
   *  cleared and a confirmation is shown. */
  function approve() {
    const action = answer?.action;
    if (!action || exec === "running") return;
    setExec("running");
    void client
      .executeAgentAction(action.tool, action.args)
      .then((done) => {
        setExec("done");
        setOutcome(done.result);
        setAnswer((a) => (a ? { ...a, action: null } : a));
      })
      .catch(() => setExec("error"));
  }

  function discard() {
    setAnswer((a) => (a ? { ...a, action: null } : a));
    setExec("idle");
    setOutcome(null);
  }

  useEffect(() => {
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onClose]);

  function open(hit: SearchHitDto) {
    if (hit.kind === "task") {
      navigate(`/tasks?open=${encodeURIComponent(hit.id)}`);
    } else if (hit.kind === "message") {
      navigate(`/mail?open=${encodeURIComponent(hit.id)}`);
    } else {
      const space = hit.space ? `&space=${encodeURIComponent(hit.space)}` : "";
      navigate(`/drive?open=${encodeURIComponent(hit.id)}${space}`);
    }
    onClose();
  }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.searchRow}>
          <Search size={18} className={styles.searchIcon} />
          <input
            className={styles.input}
            type="search"
            name="workspace-search"
            autoComplete="off"
            aria-label={strings.searchPlaceholder}
            autoFocus
            value={q}
            placeholder={strings.searchPlaceholder}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") ask();
            }}
          />
          <button
            type="button"
            className={styles.close}
            onClick={onClose}
            aria-label={strings.close}
          >
            <X size={18} />
          </button>
        </div>
        {q.trim() !== "" && (
          <button
            type="button"
            className={styles.askRow}
            onClick={ask}
            disabled={asking}
          >
            <Sparkles size={16} className={styles.askIcon} />
            <span className={styles.askLabel}>
              {strings.aiAskAbout(q.trim())}
            </span>
            {asking && <Spinner size={14} />}
          </button>
        )}
        {answer && (
          <div className={styles.answer}>
            {answer.answer && (
              <p className={styles.answerText}>{answer.answer}</p>
            )}
            {/* A proposed action — the agent never acts without this approval. */}
            {answer.action && (
              <AgentActionCard
                action={answer.action}
                running={exec === "running"}
                onApprove={approve}
                onDiscard={discard}
              />
            )}
            {exec === "done" &&
              (outcome === null ? (
                <p className={styles.actionDone}>{strings.agentDone}</p>
              ) : (
                <AgentResultCard result={outcome} />
              ))}
            {exec === "error" && (
              <p className={styles.answerNote}>{strings.agentFailed}</p>
            )}
            {answer.answer === null && answer.action === null && (
              <p className={styles.answerNote}>
                {answer.reason === "unconfigured"
                  ? strings.aiUnconfigured
                  : strings.aiUnreachable}
              </p>
            )}
            {answer.sources.length > 0 && (
              <>
                <div className={styles.answerSourcesLabel}>
                  {strings.aiSources}
                </div>
                <ul className={styles.list}>
                  {answer.sources.map((h, i) => {
                    const Icon = hitIcon(h.kind);
                    return (
                      <li key={`src:${h.kind}:${h.id}`}>
                        <button
                          type="button"
                          className={styles.hit}
                          onClick={() => open(h)}
                        >
                          <span className={styles.citeNum}>{i + 1}</span>
                          <Icon size={16} className={styles.hitIcon} />
                          <span className={styles.hitTitle}>{h.title}</span>
                          <span className={styles.hitKind}>
                            {strings.searchKind(h.kind)}
                          </span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </>
            )}
          </div>
        )}
        <div className={styles.results}>
          {loading && hits === null ? (
            <div className={styles.state}>
              <Spinner size={18} />
            </div>
          ) : hits === null ? (
            <div className={styles.state}>{strings.searchHint}</div>
          ) : hits.length === 0 ? (
            <div className={styles.state}>{strings.searchNoResults}</div>
          ) : (
            <ul className={styles.list}>
              {hits.map((h) => {
                const Icon = hitIcon(h.kind);
                return (
                  <li key={`${h.kind}:${h.id}`}>
                    <button
                      type="button"
                      className={styles.hit}
                      onClick={() => open(h)}
                    >
                      <Icon size={16} className={styles.hitIcon} />
                      <span className={styles.hitTitle}>{h.title}</span>
                      <span className={styles.hitKind}>
                        {strings.searchKind(h.kind)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
