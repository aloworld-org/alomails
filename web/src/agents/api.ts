// The client for the two agent-facing routes the record panel reads: the
// directory (what each agent may do, rendered from the intent registry) and
// the agent's one-to-one (where a verb or an ask actually goes).
//
// Its own small client rather than more methods on `ChatApi`: the record
// panel lives in every module's detail view, and what it needs from the wire
// is the agent surface, not a conversation. It shares chat's error shape so
// the server's own sentence reaches the user unchanged (UX law 8).
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";
import { ChatError } from "../chat/api";

/** One verb as the directory reports it: a stable id and its effect bit,
 *  rendered through the client's own catalogue — the route carries no
 *  English. */
export interface DirectoryTool {
  name: string;
  effect: "read" | "write";
}

/** One agent as `GET /chat/agents/directory` lists it: the chat roster's
 *  fields plus what the registry says it may touch. */
export interface DirectoryAgent {
  id: string;
  handle: string;
  name: string;
  /** Which product it is the agent *of* — the same word the rail uses for
   *  the module, so a record view finds its agent without a mapping. */
  product: string;
  /** Retired: keeps its past messages, takes no new turns. */
  disabled: boolean;
  tools: DirectoryTool[];
}

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** The agent-surface client. One method per route, like chat's. */
export class AgentsApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** Every agent the tenant has, each with what it may do — rendered from
   *  the intent registry, so the panel can never offer a verb the execution
   *  boundary would refuse. */
  async directory(): Promise<DirectoryAgent[]> {
    const body = await this.#json<{ agents: DirectoryAgent[] }>(
      await this.#send("/chat/agents/directory", {}),
    );
    return body.agents ?? [];
  }

  /** Open (or return) the caller's one-to-one with an agent — where a
   *  pre-filled verb or an ask about a record is said. */
  async openDm(agentId: string): Promise<{ id: string }> {
    return this.#json<{ id: string }>(
      await this.#send(`/chat/agents/${encodeURIComponent(agentId)}/dm`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{}",
      }),
    );
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new ChatError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) {
      const problem = (await res.json().catch(() => ({}))) as {
        detail?: unknown;
      };
      const detail = typeof problem.detail === "string" ? problem.detail : null;
      throw new ChatError(res.status, detail);
    }
    return (await res.json()) as T;
  }
}

/** The agent-surface client bound to the current session. Memoized per auth
 *  context, so effects keyed on it do not loop. */
export function useAgentsApi(): AgentsApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new AgentsApi(authorizedFetch), [authorizedFetch]);
}
