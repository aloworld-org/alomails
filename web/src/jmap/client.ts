// A small typed JMAP client. It is constructed with the auth layer's
// `authorizedFetch` (bearer + refresh handled there), fetches and caches the
// session, and exposes the handful of mail calls the UI needs. Errors are
// normalized to a single JmapError the UI can render.
import {
  CORE_CAPABILITY,
  MAIL_CAPABILITY,
  SUBMISSION_CAPABILITY,
  CATEGORIES_CAPABILITY,
  CONTACTS_CAPABILITY,
  categoryKeyword,
  type AdminGroup,
  type AdminUser,
  type TenantRole,
  type AiProvider,
  type AuditEntry,
  type Calendar,
  type CalendarEvent,
  type CalendarGrant,
  type Category,
  type Contact,
  type ContactDraft,
  type ControlDomain,
  type EventInput,
  type ControlTenant,
  type EmailAddress,
  type MailFilterRule,
  type SecurityCheck,
  type FreeBusyPerson,
  type Task,
  type TaskProject,
  type TaskDetailData,
  type TaskAttachmentDto,
  type ProjectFileDto,
  type TaskLabelDto,
  type TaskDepEdgeDto,
  type TaskInput,
  type ShareableGroup,
  type SharedMailbox,
  type Delegate,
  type SendMode,
  type EmailFull,
  type EmailHeaders,
  type JmapRequest,
  type JmapResponse,
  type Mailbox,
  type MethodCall,
  type RsvpResponse,
  type Session,
  type SpaceDto,
  type SpaceDetailDto,
  type SpaceRole,
  type DriveNodeDto,
  type DriveVersionDto,
  type BaseDto,
  type BaseFieldType,
  type BaseViewKind,
  type SearchHitDto,
  type AiAnswerDto,
  type AgentAnswerDto,
  type AgentExecuteResultDto,
} from "./types";
import { API_BASE } from "../platform/runtime";

export class JmapError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "JmapError";
  }
}

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

const HEADER_PROPS = [
  "id",
  "threadId",
  "blobId",
  "mailboxIds",
  "keywords",
  "from",
  "to",
  "cc",
  "bcc",
  "subject",
  "receivedAt",
  "size",
  "preview",
  "hasAttachment",
  "messageId",
  "references",
  "alo:authentication",
];

/** A document list entry from `/docs` (metadata only). */
export interface DocsSummaryDto {
  id: string;
  title: string;
  updatedAt: string;
}

/** A full document from `/docs/{id}`; `blocks` is the opaque block array. */
export interface DocsDto extends DocsSummaryDto {
  blocks: unknown[];
}

export class JmapClient {
  #fetch: AuthorizedFetch;
  #session: Session | null = null;
  /** When set to a shared mailbox's id, all mail calls target that account
   * instead of the user's own (ADR 0017 delegation). */
  #activeAccountId: string | null = null;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** Switch the account all mail calls target: a shared mailbox id, or null to
   * return to the user's own mailbox. Ignored if the id isn't an account the
   * session grants. */
  setActiveAccountId(id: string | null): void {
    this.#activeAccountId = id;
  }

  /** Fetch (and cache) the JMAP session; yields the primary mail account id. */
  async session(): Promise<Session> {
    if (this.#session !== null) return this.#session;
    let response: Response;
    try {
      response = await this.#fetch("/.well-known/jmap");
    } catch (err) {
      throw new JmapError(err instanceof Error ? err.message : "network error");
    }
    if (!response.ok) throw new JmapError(`session ${response.status}`);
    const session = (await response.json()) as Session;
    this.#session = session;
    return session;
  }

  async accountId(): Promise<string> {
    const session = await this.session();
    // A selected shared mailbox wins — but only if the session actually grants
    // it, so a stale selection can never target an unauthorized account.
    const active = this.#activeAccountId;
    if (active !== null && session.accounts?.[active] !== undefined)
      return active;
    const id = session.primaryAccounts[MAIL_CAPABILITY];
    if (id === undefined) throw new JmapError("no mail account");
    return id;
  }

  /** Subscribe to the server's push stream (RFC 8620 EventSource) and invoke
   * `onChange` with the account ids whose Mail/Mailbox data changed — the
   * user's own and any shared mailboxes they're delegated — and whether the
   * user's *set* of shared mailboxes changed (a grant added/revoked), so the
   * caller can re-list them. Uses a streaming fetch (not the EventSource API)
   * so the bearer token can be sent. Resolves when the stream ends (the caller
   * reconnects); throws on a failed open. */
  async subscribeChanges(
    onChange: (accountIds: string[], delegationChanged: boolean) => void,
    signal: AbortSignal,
  ): Promise<void> {
    const session = await this.session();
    const url = session.eventSourceUrl
      .replace("{types}", "Email,Mailbox,Thread")
      .replace("{closeafter}", "no")
      .replace("{ping}", "30");
    const res = await this.#fetch(url, {
      signal,
      headers: { accept: "text/event-stream" },
    });
    if (!res.ok || res.body === null)
      throw new JmapError(`eventsource ${res.status}`);
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) return;
      buf += decoder.decode(value, { stream: true });
      let sep: number;
      // SSE events are separated by a blank line.
      while ((sep = buf.indexOf("\n\n")) >= 0) {
        const raw = buf.slice(0, sep);
        buf = buf.slice(sep + 2);
        const dataLine = raw.split("\n").find((l) => l.startsWith("data:"));
        if (dataLine === undefined) continue;
        try {
          const payload = JSON.parse(dataLine.slice(5).trim()) as {
            changed?: Record<string, Record<string, unknown>>;
          };
          const changed = payload.changed ?? {};
          const ids = Object.keys(changed);
          // A "Delegation" change type signals the user's shared-mailbox set
          // changed (a grant added/revoked), not a data change.
          const delegationChanged = Object.values(changed).some(
            (types) =>
              typeof types === "object" &&
              types !== null &&
              "Delegation" in types,
          );
          if (ids.length > 0) onChange(ids, delegationChanged);
        } catch {
          // ignore a malformed/keep-alive frame
        }
      }
    }
  }

  /** Subscribe to the push stream for **chat** changes (alo Chat, ADR 0038).
   *  Invokes `onChat` whenever a room the user is in gains a message, an edit,
   *  a withdrawal, or a membership change — the client then refetches
   *  `/chat/channels` (which carries every room's unread count and last
   *  sequence) and, for the open room, its newest messages.
   *
   *  A second stream beside {@link subscribeChanges} rather than one shared
   *  fan-out: mail and chat mount independently, and merging them means
   *  refactoring the mail module for no capability. Folding both onto one
   *  connection is a known, deliberate seam. */
  async subscribeChat(onChat: () => void, signal: AbortSignal): Promise<void> {
    const session = await this.session();
    const url = session.eventSourceUrl
      .replace("{types}", "Chat")
      .replace("{closeafter}", "no")
      .replace("{ping}", "30");
    const res = await this.#fetch(url, {
      signal,
      headers: { accept: "text/event-stream" },
    });
    if (!res.ok || res.body === null)
      throw new JmapError(`eventsource ${res.status}`);
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) return;
      buf += decoder.decode(value, { stream: true });
      let sep: number;
      while ((sep = buf.indexOf("\n\n")) >= 0) {
        const raw = buf.slice(0, sep);
        buf = buf.slice(sep + 2);
        const dataLine = raw.split("\n").find((l) => l.startsWith("data:"));
        if (dataLine === undefined) continue;
        try {
          const payload = JSON.parse(dataLine.slice(5).trim()) as {
            changed?: Record<string, Record<string, unknown>>;
          };
          const touchedChat = Object.values(payload.changed ?? {}).some(
            (types) =>
              typeof types === "object" && types !== null && "Chat" in types,
          );
          if (touchedChat) onChat();
        } catch {
          // ignore a malformed/keep-alive frame
        }
      }
    }
  }

  /** The shared mailboxes the user was delegated (session's non-personal
   * accounts) — for the mailbox switcher. */
  async sharedMailboxes(): Promise<SharedMailbox[]> {
    const session = await this.session();
    const own = session.primaryAccounts[MAIL_CAPABILITY];
    return Object.entries(session.accounts ?? {})
      .filter(([id, a]) => !a.isPersonal && id !== own)
      .map(([id, a]) => ({
        id,
        name: a.name,
        canSend: a["alo:canSend"] === true,
        readOnly: a.isReadOnly === true,
      }));
  }

  async #request(methodCalls: MethodCall[]): Promise<JmapResponse> {
    const session = await this.session();
    const body: JmapRequest = {
      using: [
        CORE_CAPABILITY,
        MAIL_CAPABILITY,
        SUBMISSION_CAPABILITY,
        CATEGORIES_CAPABILITY,
        CONTACTS_CAPABILITY,
      ],
      methodCalls,
    };
    let response: Response;
    try {
      response = await this.#fetch(session.apiUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch (err) {
      throw new JmapError(err instanceof Error ? err.message : "network error");
    }
    if (!response.ok) throw new JmapError(`request ${response.status}`);
    return (await response.json()) as JmapResponse;
  }

  #result(res: JmapResponse, callId: string): Record<string, unknown> {
    const found = res.methodResponses.find((m) => m[2] === callId);
    if (found === undefined) throw new JmapError("missing method response");
    if (found[0] === "error") {
      const type = (found[1] as { type?: string }).type ?? "unknown";
      throw new JmapError(`JMAP error: ${type}`);
    }
    return found[1];
  }

  /** All of the account's mailboxes (folders). */
  async mailboxes(): Promise<Mailbox[]> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Mailbox/get", { accountId, ids: null }, "m"],
    ]);
    return (this.#result(res, "m").list as Mailbox[]) ?? [];
  }

  /** The signed-in user's own (personal) mail account id, ignoring any active
   * shared-mailbox selection — stable for the session. */
  async ownAccountId(): Promise<string> {
    const session = await this.session();
    const id = session.primaryAccounts[MAIL_CAPABILITY];
    if (id === undefined) throw new JmapError("no mail account");
    return id;
  }

  /** All mailboxes of a specific account (own or a delegated shared one),
   * independent of the active-account selection — for the always-mounted
   * sidebar which shows every accessible mailbox's folders at once. */
  async mailboxesFor(accountId: string): Promise<Mailbox[]> {
    const res = await this.#request([
      ["Mailbox/get", { accountId, ids: null }, "m"],
    ]);
    return (this.#result(res, "m").list as Mailbox[]) ?? [];
  }

  /** Set (or clear, with null) a mailbox/label's display color ("#rrggbb"). */
  async setMailboxColor(
    mailboxId: string,
    color: string | null,
  ): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Mailbox/set", { accountId, update: { [mailboxId]: { color } } }, "s"],
    ]);
    const result = this.#result(res, "s");
    const notUpdated = (
      result.notUpdated as Record<string, unknown> | undefined
    )?.[mailboxId];
    if (notUpdated !== undefined)
      throw new JmapError("could not set the label color");
  }

  /** Create a folder/label, optionally nested under `parentId`. Returns its id. */
  async createMailbox(name: string, parentId?: string | null): Promise<string> {
    const accountId = await this.accountId();
    const props: Record<string, unknown> = { name };
    if (parentId != null) props.parentId = parentId;
    const res = await this.#request([
      ["Mailbox/set", { accountId, create: { m: props } }, "s"],
    ]);
    const result = this.#result(res, "s");
    const created = (
      result.created as Record<string, { id: string }> | undefined
    )?.m;
    if (created === undefined)
      throw new JmapError("could not create the folder");
    return created.id;
  }

  /** Rename a folder/label. */
  async renameMailbox(id: string, name: string): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Mailbox/set", { accountId, update: { [id]: { name } } }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notUpdated as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not rename the folder");
  }

  /** Re-parent a folder (drag into another, or to the root with null). */
  async moveMailbox(id: string, parentId: string | null): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Mailbox/set", { accountId, update: { [id]: { parentId } } }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notUpdated as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not move the folder");
  }

  /** Delete a folder/label. */
  async deleteMailbox(id: string): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Mailbox/set", { accountId, destroy: [id] }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notDestroyed as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not delete the folder");
  }

  // ---- categories (colored message labels) --------------------------

  /** The account's categories (catalog of name + color), in the user's order. */
  async categories(): Promise<Category[]> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Category/get", { accountId, ids: null }, "c"],
    ]);
    return (this.#result(res, "c").list as Category[]) ?? [];
  }

  /** Creates a category; returns the created catalog entry (id + keyword). */
  async createCategory(name: string, color: string | null): Promise<Category> {
    const accountId = await this.accountId();
    const props: Record<string, unknown> = { name };
    if (color !== null) props.color = color;
    const res = await this.#request([
      ["Category/set", { accountId, create: { c: props } }, "s"],
    ]);
    const created = (
      this.#result(res, "s").created as Record<string, Category> | undefined
    )?.c;
    if (created === undefined)
      throw new JmapError("could not create the category");
    return created;
  }

  /** Renames and/or recolors a category (color null clears it). */
  async updateCategory(
    id: string,
    name: string,
    color: string | null,
  ): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Category/set", { accountId, update: { [id]: { name, color } } }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notUpdated as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not update the category");
  }

  /** Deletes a category, stripping its tag from every message that carried it. */
  async deleteCategory(id: string): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Category/set", { accountId, destroy: [id] }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notDestroyed as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not delete the category");
  }

  /** The account's saved address-book contacts, in name order. */
  async contacts(): Promise<Contact[]> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Contact/get", { accountId, ids: null }, "c"],
    ]);
    return (this.#result(res, "c").list as Contact[]) ?? [];
  }

  /** Creates a contact from a draft; returns its new id. */
  async createContact(draft: ContactDraft): Promise<string> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Contact/set", { accountId, create: { c: draft } }, "s"],
    ]);
    const result = this.#result(res, "s");
    const created = (
      result.created as Record<string, { id: string }> | undefined
    )?.c;
    if (created === undefined) {
      const bad = (
        result.notCreated as
          Record<string, { description?: string }> | undefined
      )?.c;
      throw new JmapError(bad?.description ?? "could not create the contact");
    }
    return created.id;
  }

  /** Replaces a contact's fields (a full draft, not a partial patch). */
  async updateContact(id: string, draft: ContactDraft): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Contact/set", { accountId, update: { [id]: draft } }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notUpdated as
        Record<string, { description?: string }> | undefined
    )?.[id];
    if (bad !== undefined)
      throw new JmapError(bad.description ?? "could not update the contact");
  }

  /** Deletes a contact. */
  async deleteContact(id: string): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Contact/set", { accountId, destroy: [id] }, "s"],
    ]);
    const bad = (
      this.#result(res, "s").notDestroyed as Record<string, unknown> | undefined
    )?.[id];
    if (bad !== undefined) throw new JmapError("could not delete the contact");
  }

  /** Imports a `.vcf` document (one or many cards); returns how many were
   * imported and how many skipped. */
  async importContacts(
    vcf: string,
  ): Promise<{ imported: number; skipped: number }> {
    const res = await this.#fetch(`${API_BASE}/contacts/import`, {
      method: "POST",
      headers: { "content-type": "text/vcard" },
      body: vcf,
    });
    if (!res.ok) throw new JmapError(`import ${res.status}`);
    return (await res.json()) as { imported: number; skipped: number };
  }

  /** The whole address book as a `.vcf` document (for download). */
  async exportContacts(): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/contacts/export`, {
      method: "GET",
    });
    if (!res.ok) throw new JmapError(`export ${res.status}`);
    return await res.text();
  }

  /** Imports recent mail from a remote IMAP server (Gmail/Outlook/…) into
   * the Inbox. Returns the counts, or throws with the server's message
   * (e.g. bad credentials) so the wizard can show it. */
  async importImap(input: {
    host: string;
    port?: number;
    username: string;
    password: string;
  }): Promise<{ imported: number; skipped: number; failed: number }> {
    const res = await this.#fetch(`${API_BASE}/import/imap`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    });
    if (!res.ok) {
      const detail = (await res.json().catch(() => ({}))) as {
        detail?: string;
      };
      throw new JmapError(detail.detail ?? `import ${res.status}`);
    }
    return (await res.json()) as {
      imported: number;
      skipped: number;
      failed: number;
    };
  }

  /** Tags (or untags) a message with a category. */
  async setCategory(
    emailId: string,
    categoryId: string,
    on: boolean,
  ): Promise<void> {
    await this.#setKeyword(emailId, categoryKeyword(categoryId), on);
  }

  /** Tags/untags several messages with a category in one call (whole thread). */
  async setCategoryMany(
    emailIds: string[],
    categoryId: string,
    on: boolean,
  ): Promise<void> {
    if (emailIds.length === 0) return;
    const accountId = await this.accountId();
    const patch = {
      [`keywords/${categoryKeyword(categoryId)}`]: on ? true : null,
    };
    const update: Record<string, unknown> = {};
    for (const id of emailIds) update[id] = patch;
    const res = await this.#request([
      ["Email/set", { accountId, update }, "s"],
    ]);
    this.#result(res, "s");
  }

  /** Header rows for a mailbox, newest first, via query + back-referenced get.
   * When `categoryId` is given, the folder is further filtered to messages
   * tagged with that category (server-side `hasKeyword`). */
  async emailHeaders(
    mailboxId: string,
    limit = 60,
    categoryId?: string,
  ): Promise<EmailHeaders[]> {
    const accountId = await this.accountId();
    const filter: Record<string, unknown> = { inMailbox: mailboxId };
    if (categoryId !== undefined)
      filter.hasKeyword = categoryKeyword(categoryId);
    const res = await this.#request([
      [
        "Email/query",
        {
          accountId,
          filter,
          sort: [{ property: "receivedAt", isAscending: false }],
          limit,
        },
        "q",
      ],
      [
        "Email/get",
        {
          accountId,
          "#ids": { resultOf: "q", name: "Email/query", path: "/ids" },
          properties: HEADER_PROPS,
        },
        "g",
      ],
    ]);
    return (this.#result(res, "g").list as EmailHeaders[]) ?? [];
  }

  /** Full-text search across the whole account (server-side `Email/query` text
   * filter), newest first, as header rows. Empty query returns nothing. */
  async searchEmails(query: string, limit = 60): Promise<EmailHeaders[]> {
    const q = query.trim();
    if (q === "") return [];
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "Email/query",
        {
          accountId,
          filter: { text: q },
          sort: [{ property: "receivedAt", isAscending: false }],
          limit,
        },
        "q",
      ],
      [
        "Email/get",
        {
          accountId,
          "#ids": { resultOf: "q", name: "Email/query", path: "/ids" },
          properties: HEADER_PROPS,
        },
        "g",
      ],
    ]);
    return (this.#result(res, "g").list as EmailHeaders[]) ?? [];
  }

  /** Header rows for the cross-folder "Flagged" smart view: every $flagged
   * message across all folders, newest first. Mirrors searchEmails but filters
   * on the keyword instead of text (hasKeyword needs no inMailbox). */
  async flaggedHeaders(limit = 100): Promise<EmailHeaders[]> {
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "Email/query",
        {
          accountId,
          filter: { hasKeyword: "$flagged" },
          sort: [{ property: "receivedAt", isAscending: false }],
          limit,
        },
        "q",
      ],
      [
        "Email/get",
        {
          accountId,
          "#ids": { resultOf: "q", name: "Email/query", path: "/ids" },
          properties: HEADER_PROPS,
        },
        "g",
      ],
    ]);
    return (this.#result(res, "g").list as EmailHeaders[]) ?? [];
  }

  /** Move messages to exactly `target`, replacing all mailbox membership. Unlike
   * `moveMany`, this needs no source folder, so it is correct from a virtual
   * view (e.g. Flagged) where a message's folder isn't the selected one. */
  async moveToFolder(ids: string[], target: string): Promise<void> {
    if (ids.length === 0) return;
    const accountId = await this.accountId();
    const update: Record<string, unknown> = {};
    for (const id of ids) update[id] = { mailboxIds: { [target]: true } };
    const res = await this.#request([
      ["Email/set", { accountId, update }, "m"],
    ]);
    this.#result(res, "m");
  }

  /** All messages of a thread, with bodies, oldest-first (for the conversation
   * view). One request: Thread/get feeds Email/get by back-reference. */
  async threadEmails(threadId: string): Promise<EmailFull[]> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Thread/get", { accountId, ids: [threadId] }, "t"],
      [
        "Email/get",
        {
          accountId,
          "#ids": {
            resultOf: "t",
            name: "Thread/get",
            path: "/list/0/emailIds",
          },
          properties: [
            ...HEADER_PROPS,
            "textBody",
            "htmlBody",
            "bodyValues",
            "attachments",
          ],
          fetchTextBodyValues: true,
          fetchHTMLBodyValues: true,
        },
        "e",
      ],
    ]);
    const list = (this.#result(res, "e").list as EmailFull[]) ?? [];
    return [...list].sort((a, b) => a.receivedAt.localeCompare(b.receivedAt));
  }

  /** One message with its body, for the reading pane. */
  async email(id: string): Promise<EmailFull | null> {
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "Email/get",
        {
          accountId,
          ids: [id],
          properties: [
            ...HEADER_PROPS,
            "textBody",
            "htmlBody",
            "bodyValues",
            "attachments",
          ],
          fetchTextBodyValues: true,
          fetchHTMLBodyValues: true,
        },
        "e",
      ],
    ]);
    const list = this.#result(res, "e").list as EmailFull[];
    return list[0] ?? null;
  }

  /** Whether AI features are enabled for this tenant (session flag). */
  async aiEnabled(): Promise<boolean> {
    const session = await this.session();
    return session["alo:aiEnabled"] === true;
  }

  /** Whether the signed-in user is a tenant admin (session flag). */
  async isAdmin(): Promise<boolean> {
    const session = await this.session();
    return session["alo:isAdmin"] === true;
  }

  /**
   * Whether this user may work the books: a tenant admin, or the holder of the
   * `accountant` role (ADR 0035, B4.12) — the server's own `require_finance`,
   * asked here only to decide whether to *draw* the approver's tab.
   *
   * It is never an access decision. Every finance route checks the same thing
   * again and answers `403`, so a stale session hides a tab at worst and opens
   * nothing at all.
   */
  async canWorkTheBooks(): Promise<boolean> {
    const session = await this.session();
    return (
      session["alo:isAdmin"] === true ||
      (session["alo:roles"] ?? []).includes("accountant")
    );
  }

  /** The addresses this user may send from (canonical + aliases), for the
   * compose From picker. Empty if the session doesn't carry the list. */
  async sendableAddresses(): Promise<string[]> {
    const session = await this.session();
    return session["alo:sendAs"] ?? [];
  }

  async #admin(path: string, init: RequestInit): Promise<unknown> {
    const res = await this.#fetch(`${API_BASE}${path}`, init);
    if (!res.ok) throw new JmapError(`admin ${res.status}`);
    return res.json();
  }

  /** All AI providers configured for this tenant (admin). */
  async listProviders(): Promise<AiProvider[]> {
    const out = (await this.#admin("/admin/ai/providers", {
      method: "GET",
    })) as {
      providers: AiProvider[];
    };
    return out.providers;
  }

  /** Create or update a provider (admin). Omit `apiKey` to keep the stored key. */
  async upsertProvider(p: {
    id: string;
    kind: string;
    label: string;
    baseUrl: string;
    model: string;
    enabled: boolean;
    apiKey?: string;
  }): Promise<void> {
    await this.#admin("/admin/ai/providers", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(p),
    });
  }

  /** Make a provider the tenant's default (admin). */
  async setDefaultProvider(id: string): Promise<void> {
    await this.#admin("/admin/ai/providers/default", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id }),
    });
  }

  /** Delete a provider (admin). */
  async deleteProvider(id: string): Promise<void> {
    await this.#admin(`/admin/ai/providers/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  /** Test connectivity to a backend without saving it (admin). */
  async testConnection(
    baseUrl: string,
    apiKey?: string,
  ): Promise<{ ok: boolean; models: number }> {
    return (await this.#admin("/admin/ai/test", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(
        apiKey !== undefined && apiKey.length > 0
          ? { baseUrl, apiKey }
          : { baseUrl },
      ),
    })) as { ok: boolean; models: number };
  }

  #adminPost(path: string, body: unknown): Promise<unknown> {
    return this.#admin(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  /** All users in the tenant (admin). */
  async listUsers(): Promise<AdminUser[]> {
    const out = (await this.#admin("/admin/users", { method: "GET" })) as {
      users: AdminUser[];
    };
    return out.users;
  }

  /** Create a user with a mailbox (admin). */
  async createUser(email: string, password: string): Promise<void> {
    await this.#adminPost("/admin/users", { email, password });
  }

  /** Reset a user's password (admin). */
  async resetPassword(userId: string, password: string): Promise<void> {
    await this.#adminPost("/admin/users/password", { userId, password });
  }

  /** Grant or revoke a user's tenant-admin flag (admin). */
  async setUserAdmin(userId: string, isAdmin: boolean): Promise<void> {
    await this.#adminPost("/admin/users/admin", { userId, isAdmin });
  }

  /** Grant or revoke a tenant-wide scoped role, e.g. the accountant
   * (ADR 0035, B4.12). Its own call rather than a field beside `isAdmin`:
   * the admin flag is the console, a role is a scope. */
  async setUserRole(
    userId: string,
    role: TenantRole,
    granted: boolean,
  ): Promise<void> {
    await this.#adminPost("/admin/users/roles", { userId, role, granted });
  }

  /** Delete a user and their mail (admin). */
  async deleteUser(userId: string): Promise<void> {
    await this.#admin(`/admin/users/${encodeURIComponent(userId)}`, {
      method: "DELETE",
    });
  }

  /** Add an alias address to a user (admin). */
  async addAlias(userId: string, address: string): Promise<void> {
    await this.#adminPost("/admin/users/alias", { userId, address });
  }

  /** Remove an alias address (admin). */
  async removeAlias(address: string): Promise<void> {
    await this.#adminPost("/admin/users/alias/remove", { address });
  }

  /** All groups in the tenant, with members and list address (admin). */
  async listGroups(): Promise<AdminGroup[]> {
    const out = (await this.#admin("/admin/groups", { method: "GET" })) as {
      groups: AdminGroup[];
    };
    return out.groups;
  }

  /** Create a group (admin); returns its id. */
  async createGroup(name: string): Promise<string> {
    const out = (await this.#adminPost("/admin/groups", { name })) as {
      id: string;
    };
    return out.id;
  }

  /** Delete a group (admin). */
  async deleteGroup(id: string): Promise<void> {
    await this.#admin(`/admin/groups/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  /** Rename a group / distribution list (admin). */
  async renameGroup(id: string, name: string): Promise<void> {
    await this.#adminPost("/admin/groups/name", { groupId: id, name });
  }

  // ---- mailbox delegation / shared mailboxes (ADR 0017) --------------

  /** Who can access `ownerId`'s mailbox (admin). */
  async listDelegates(ownerId: string): Promise<Delegate[]> {
    const res = (await this.#admin(
      `/admin/delegates/${encodeURIComponent(ownerId)}`,
      {
        method: "GET",
      },
    )) as { delegates: Delegate[] };
    return res.delegates;
  }

  /** Grant `delegateId` access to `ownerId`'s mailbox (admin). `sendMode` is
   * "none" | "as" | "on_behalf". `folders` confines access to those mailbox ids
   * (empty = whole mailbox). */
  async grantDelegate(
    ownerId: string,
    delegateId: string,
    canWrite: boolean,
    sendMode: SendMode,
    folders: string[],
  ): Promise<void> {
    await this.#adminPost("/admin/delegates", {
      ownerId,
      delegateId,
      canWrite,
      sendMode,
      folders,
    });
  }

  /** A user's folders (id + name), for the admin per-folder delegation picker. */
  async adminUserMailboxes(
    userId: string,
  ): Promise<{ id: string; name: string }[]> {
    const res = (await this.#admin(
      `/admin/users/${encodeURIComponent(userId)}/mailboxes`,
      {
        method: "GET",
      },
    )) as { mailboxes: { id: string; name: string }[] };
    return res.mailboxes;
  }

  /** Revoke `delegateId`'s access to `ownerId`'s mailbox (admin). */
  async revokeDelegate(ownerId: string, delegateId: string): Promise<void> {
    await this.#adminPost("/admin/delegates/remove", { ownerId, delegateId });
  }

  /** Self-service: who can access MY mailbox. */
  async myDelegates(): Promise<Delegate[]> {
    const res = (await this.#admin("/jmap/delegates", { method: "GET" })) as {
      delegates: Delegate[];
    };
    return res.delegates;
  }

  /** Self-service: share my mailbox with a person (by email, same tenant).
   * `folders` restricts access to those mailbox ids (empty = whole mailbox). */
  async shareMyMailbox(
    email: string,
    canWrite: boolean,
    sendMode: SendMode,
    folders: string[],
  ): Promise<void> {
    await this.#adminPost("/jmap/delegates", {
      email,
      canWrite,
      sendMode,
      folders,
    });
  }

  /** Self-service: stop sharing my mailbox with a person. */
  async unshareMyMailbox(delegateId: string): Promise<void> {
    await this.#adminPost("/jmap/delegates/remove", { delegateId });
  }

  /** Set or clear a group's distribution-list address (admin). */
  async setGroupAddress(
    groupId: string,
    address: string | null,
  ): Promise<void> {
    await this.#adminPost(
      "/admin/groups/address",
      address === null ? { groupId } : { groupId, address },
    );
  }

  /** Add a user to a group (admin). */
  async addGroupMember(groupId: string, userId: string): Promise<void> {
    await this.#adminPost("/admin/groups/members", { groupId, userId });
  }

  /** Remove a user from a group (admin). */
  async removeGroupMember(groupId: string, userId: string): Promise<void> {
    await this.#adminPost("/admin/groups/members/remove", { groupId, userId });
  }

  /** Run the live deliverability checks for the mail domain (admin). */
  async securityChecks(): Promise<{ domain: string; checks: SecurityCheck[] }> {
    return (await this.#admin("/admin/security/checks", { method: "GET" })) as {
      domain: string;
      checks: SecurityCheck[];
    };
  }

  // ---- mail settings (signature + org footer) -------------------------

  /** The signed-in user's signature, the tenant's org footer, and the user's
   * out-of-office state. */
  async mailSettings(): Promise<{
    signature: string;
    orgFooter: string;
    outOfOffice: { enabled: boolean; subject: string; message: string };
  }> {
    return (await this.#admin("/settings/mail", { method: "GET" })) as {
      signature: string;
      orgFooter: string;
      outOfOffice: { enabled: boolean; subject: string; message: string };
    };
  }

  /** Save the caller's signature (HTML; empty clears it). */
  async setSignature(signature: string): Promise<void> {
    await this.#adminPost("/settings/signature", { signature });
  }

  /** Set the caller's out-of-office auto-reply (a message is required to
   * enable). */
  async setOutOfOffice(
    enabled: boolean,
    subject: string,
    message: string,
  ): Promise<void> {
    await this.#adminPost("/settings/out-of-office", {
      enabled,
      subject,
      message,
    });
  }

  /** Set the tenant's organization footer (admin; HTML; empty clears it). */
  async setOrgFooter(footer: string): Promise<void> {
    await this.#adminPost("/admin/org-footer", { footer });
  }

  /** This tenant's recent administrative actions, newest first (admin). */
  async adminAuditLog(): Promise<AuditEntry[]> {
    const out = (await this.#admin("/admin/audit", { method: "GET" })) as {
      entries: AuditEntry[];
    };
    return out.entries;
  }

  /** This tenant's own domains (admin). */
  async adminListDomains(): Promise<ControlDomain[]> {
    const out = (await this.#admin("/admin/domains", { method: "GET" })) as {
      domains: ControlDomain[];
    };
    return out.domains;
  }

  /** Register a domain to this tenant; returns the DNS record to publish (admin). */
  async adminCreateDomain(domain: string): Promise<ControlDomain> {
    return (await this.#adminPost("/admin/domains", {
      domain,
    })) as ControlDomain;
  }

  /** Check the DNS proof and mark this tenant's domain verified (admin). */
  async adminVerifyDomain(
    domain: string,
  ): Promise<{ domain: string; verified: boolean }> {
    return (await this.#adminPost("/admin/domains/verify", { domain })) as {
      domain: string;
      verified: boolean;
    };
  }

  /** Remove one of this tenant's domains (admin). */
  async adminDeleteDomain(domain: string): Promise<void> {
    await this.#adminPost("/admin/domains/delete", { domain });
  }

  /** Rotate the DKIM key for one of this tenant's domains (admin, ADR 0014). */
  async adminRotateDkim(
    domain: string,
  ): Promise<{ domain: string; dkim: ControlDomain["dkim"] }> {
    return (await this.#adminPost("/admin/domains/dkim/rotate", {
      domain,
    })) as {
      domain: string;
      dkim: ControlDomain["dkim"];
    };
  }

  // ---- control plane (platform operator; ADR 0012) --------------------

  /** Whether the signed-in user is a platform operator (gates the console). */
  async isOperator(): Promise<boolean> {
    try {
      const out = (await this.#admin("/control/me", { method: "GET" })) as {
        isOperator?: boolean;
      };
      return out.isOperator === true;
    } catch {
      return false;
    }
  }

  /** Every tenant on the deployment (operator). */
  async listTenants(): Promise<ControlTenant[]> {
    const out = (await this.#admin("/control/tenants", { method: "GET" })) as {
      tenants: ControlTenant[];
    };
    return out.tenants;
  }

  /** Provision a tenant and its first admin (operator). */
  async createTenant(t: {
    name: string;
    adminEmail: string;
    adminPassword: string;
  }): Promise<{ id: string; adminUserId: string }> {
    return (await this.#adminPost("/control/tenants", t)) as {
      id: string;
      adminUserId: string;
    };
  }

  /** Suspend or resume a tenant (operator). */
  async setTenantStatus(
    id: string,
    status: "active" | "suspended",
  ): Promise<void> {
    await this.#adminPost(`/control/tenants/${encodeURIComponent(id)}/status`, {
      status,
    });
  }

  /** Set a tenant's storage quota in bytes, or null for unlimited (operator). */
  async setTenantQuota(id: string, quotaBytes: number | null): Promise<void> {
    await this.#adminPost(`/control/tenants/${encodeURIComponent(id)}/quota`, {
      quotaBytes,
    });
  }

  /** Permanently delete a tenant (operator). `confirm` must echo the id. */
  async deleteTenant(id: string, confirm: string): Promise<void> {
    await this.#admin(`/control/tenants/${encodeURIComponent(id)}`, {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ confirm }),
    });
  }

  /** Every registered domain on the deployment (operator). */
  async listDomains(): Promise<ControlDomain[]> {
    const out = (await this.#admin("/control/domains", { method: "GET" })) as {
      domains: ControlDomain[];
    };
    return out.domains;
  }

  /** Register a domain to a tenant; returns the DNS record to publish. */
  async createDomain(tenantId: string, domain: string): Promise<ControlDomain> {
    return (await this.#adminPost("/control/domains", {
      tenantId,
      domain,
    })) as ControlDomain;
  }

  /** Check the DNS TXT proof and mark verified if present (operator). */
  async verifyDomain(
    domain: string,
  ): Promise<{ domain: string; verified: boolean; detail?: string }> {
    return (await this.#adminPost("/control/domains/verify", { domain })) as {
      domain: string;
      verified: boolean;
      detail?: string;
    };
  }

  /** Remove a domain registration (operator). */
  async deleteDomain(domain: string): Promise<void> {
    await this.#adminPost("/control/domains/delete", { domain });
  }

  /** Improve a draft via the tenant's configured AI backend (ADR 0011).
   * Returns the improved text; throws if AI is unavailable or the backend fails
   * (the caller keeps the user's original draft either way). */
  async improveDraft(text: string): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/ai/improve`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new JmapError(`improve ${res.status}`);
    const json = (await res.json()) as { text: string };
    return json.text;
  }

  /** Summarize an email thread via the tenant's AI backend (ADR 0011); throws
   * if AI is unavailable (the reading pane then just hides the summary card). */
  async summarizeThread(text: string): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/ai/summarize`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new JmapError(`summarize ${res.status}`);
    const json = (await res.json()) as { summary: string };
    return json.summary;
  }

  /** Suggest up to three short, ready-to-send replies for a conversation.
   * `text` is the same flattened thread text used for summarization. */
  async smartReplies(text: string): Promise<string[]> {
    const res = await this.#fetch(`${API_BASE}/ai/replies`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new JmapError(`replies ${res.status}`);
    const json = (await res.json()) as { replies: string[] };
    return json.replies;
  }

  /** Snooze conversations: hide the given messages (in `mailboxId`) until
   * `until` (Unix seconds); a server sweeper returns them to the Inbox. */
  async snooze(ids: string[], mailboxId: string, until: number): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/snooze`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ids, mailboxId, until }),
    });
    if (!res.ok) throw new JmapError(`snooze ${res.status}`);
  }

  // ---- alo Docs (ADR 0015): tenant/owner-scoped technical-authoring docs ----

  /** List the caller's documents (metadata only), newest-first. */
  async listDocs(): Promise<DocsSummaryDto[]> {
    const res = await this.#fetch(`${API_BASE}/docs`, { method: "GET" });
    if (!res.ok) throw new JmapError(`listDocs ${res.status}`);
    const json = (await res.json()) as { documents: DocsSummaryDto[] };
    return json.documents;
  }

  /** Create a document and return it (with empty blocks). */
  async createDoc(title: string): Promise<DocsDto> {
    const res = await this.#fetch(`${API_BASE}/docs`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title }),
    });
    if (!res.ok) throw new JmapError(`createDoc ${res.status}`);
    return (await res.json()) as DocsDto;
  }

  /** Load one document with its blocks. */
  async getDoc(id: string): Promise<DocsDto> {
    const res = await this.#fetch(
      `${API_BASE}/docs/${encodeURIComponent(id)}`,
      {
        method: "GET",
      },
    );
    if (!res.ok) throw new JmapError(`getDoc ${res.status}`);
    return (await res.json()) as DocsDto;
  }

  /** Save a document's title and blocks. */
  async saveDoc(id: string, title: string, blocks: unknown[]): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/docs/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ title, blocks }),
      },
    );
    if (!res.ok) throw new JmapError(`saveDoc ${res.status}`);
  }

  /** Delete a document. */
  async deleteDoc(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/docs/${encodeURIComponent(id)}`,
      {
        method: "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`deleteDoc ${res.status}`);
  }

  /** Fetch an attachment's bytes as a Blob (authorized), for saving. Resolves
   * the session download URL template with the account, blob id, and name. */
  async downloadAttachment(blobId: string, name: string): Promise<Blob> {
    const session = await this.session();
    const accountId = await this.accountId();
    const url = session.downloadUrl
      .replace("{accountId}", encodeURIComponent(accountId))
      .replace("{blobId}", encodeURIComponent(blobId))
      .replace("{name}", encodeURIComponent(name));
    const res = await this.#fetch(url, { method: "GET" });
    if (!res.ok) throw new JmapError(`download ${res.status}`);
    return res.blob();
  }

  /** Mark a message read/unread by toggling the $seen keyword. */
  async setSeen(id: string, seen: boolean): Promise<void> {
    await this.#setKeyword(id, "$seen", seen);
  }

  /** Flag/unflag a message ($flagged keyword). */
  async setFlagged(id: string, flagged: boolean): Promise<void> {
    await this.#setKeyword(id, "$flagged", flagged);
  }

  /** Set (epoch seconds) or clear (null) a flagged message's follow-up due-date.
   * Setting a date also flags the message server-side. */
  async setFlagDue(id: string, dueAt: number | null): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/jmap/flag-due`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ emailId: id, dueAt }),
    });
    if (!res.ok) throw new JmapError(`flag-due ${res.status}`);
  }

  async #setKeyword(id: string, keyword: string, on: boolean): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "Email/set",
        {
          accountId,
          update: { [id]: { [`keywords/${keyword}`]: on ? true : null } },
        },
        "s",
      ],
    ]);
    this.#result(res, "s");
  }

  /** Move a message from one mailbox to another (e.g. archive). */
  async move(
    id: string,
    fromMailboxId: string,
    toMailboxId: string,
  ): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "Email/set",
        {
          accountId,
          update: {
            [id]: {
              [`mailboxIds/${fromMailboxId}`]: null,
              [`mailboxIds/${toMailboxId}`]: true,
            },
          },
        },
        "m",
      ],
    ]);
    this.#result(res, "m");
  }

  /** Permanently delete a message. */
  async destroy(id: string): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Email/set", { accountId, destroy: [id] }, "d"],
    ]);
    this.#result(res, "d");
  }

  /** Mark several messages read/unread in one call (whole-conversation). */
  async setSeenMany(ids: string[], seen: boolean): Promise<void> {
    if (ids.length === 0) return;
    const accountId = await this.accountId();
    const update: Record<string, unknown> = {};
    for (const id of ids) update[id] = { "keywords/$seen": seen ? true : null };
    const res = await this.#request([
      ["Email/set", { accountId, update }, "s"],
    ]);
    this.#result(res, "s");
  }

  /** Move several messages from one mailbox to another in one call. */
  async moveMany(
    ids: string[],
    fromMailboxId: string,
    toMailboxId: string,
  ): Promise<void> {
    if (ids.length === 0) return;
    const accountId = await this.accountId();
    const update: Record<string, unknown> = {};
    for (const id of ids) {
      update[id] = {
        [`mailboxIds/${fromMailboxId}`]: null,
        [`mailboxIds/${toMailboxId}`]: true,
      };
    }
    const res = await this.#request([
      ["Email/set", { accountId, update }, "m"],
    ]);
    this.#result(res, "m");
  }

  /** Permanently delete several messages in one call. */
  async destroyMany(ids: string[]): Promise<void> {
    if (ids.length === 0) return;
    const accountId = await this.accountId();
    const res = await this.#request([
      ["Email/set", { accountId, destroy: ids }, "d"],
    ]);
    this.#result(res, "d");
  }

  /** Upload a file's bytes to the blob store; returns its blob id + type/size.
   * The blob is unreferenced until a draft embeds it (and is GC'd if never used). */
  async uploadFile(
    file: File,
  ): Promise<{ blobId: string; type: string; size: number }> {
    const session = await this.session();
    const accountId = await this.accountId();
    const absolute = session.uploadUrl.replace(
      "{accountId}",
      encodeURIComponent(accountId),
    );
    // The session's uploadUrl is absolute (the server's own origin). Use only its
    // path against our API base — same-origin in the browser (proxied in dev, so
    // no CORS), and the hosted server in the desktop app — like every other call.
    const { pathname, search } = new URL(absolute, API_BASE);
    const url = `${API_BASE}${pathname}${search}`;
    const res = await this.#fetch(url, {
      method: "POST",
      headers: {
        "content-type":
          file.type.length > 0 ? file.type : "application/octet-stream",
      },
      body: file,
    });
    if (!res.ok) throw new JmapError(`upload ${res.status}`);
    const json = (await res.json()) as {
      blobId: string;
      type: string;
      size: number;
    };
    return { blobId: json.blobId, type: json.type, size: json.size };
  }

  // ---- Spaces (ADR 0026) ----------------------------------------------------

  /** The Spaces the caller belongs to. */
  async spaces(): Promise<SpaceDto[]> {
    const res = await this.#fetch(`${API_BASE}/spaces`);
    if (!res.ok) throw new JmapError(`spaces ${res.status}`);
    return ((await res.json()) as { spaces: SpaceDto[] }).spaces;
  }

  /** Create a Space (caller becomes its manager). */
  async createSpace(name: string): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/spaces`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
    });
    if (!res.ok) throw new JmapError(`createSpace ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** A Space with its members and modules. */
  async spaceDetail(id: string): Promise<SpaceDetailDto> {
    const res = await this.#fetch(
      `${API_BASE}/spaces/${encodeURIComponent(id)}`,
    );
    if (!res.ok) throw new JmapError(`spaceDetail ${res.status}`);
    return (await res.json()) as SpaceDetailDto;
  }

  /** Rename and/or (un)archive a Space (manager only). */
  async updateSpace(
    id: string,
    patch: { name?: string; archived?: boolean },
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/spaces/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(patch),
      },
    );
    if (!res.ok) throw new JmapError(`updateSpace ${res.status}`);
  }

  /** Add or re-role a member by email (manager only). */
  async addSpaceMember(
    id: string,
    email: string,
    role: SpaceRole,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/spaces/${encodeURIComponent(id)}/members`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, role }),
      },
    );
    if (!res.ok) throw new JmapError(`addSpaceMember ${res.status}`);
  }

  /** Remove a member (manager only). */
  async removeSpaceMember(id: string, userId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/spaces/${encodeURIComponent(id)}/members/${encodeURIComponent(userId)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`removeSpaceMember ${res.status}`);
  }

  // ---- Drive (ADR 0027) -----------------------------------------------------

  /** A folder's live contents. `space` = null lists personal My Files; `parent`
   *  = null lists the location root. */
  async driveList(
    space: string | null,
    parent: string | null,
  ): Promise<DriveNodeDto[]> {
    const q = new URLSearchParams();
    if (space) q.set("space", space);
    if (parent) q.set("parent", parent);
    const res = await this.#fetch(`${API_BASE}/drive/list?${q.toString()}`);
    if (!res.ok) throw new JmapError(`driveList ${res.status}`);
    return ((await res.json()) as { nodes: DriveNodeDto[] }).nodes;
  }

  /** A single Drive node the caller can read (for opening a search result). */
  async driveNode(id: string): Promise<DriveNodeDto | null> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}`,
    );
    if (res.status === 404) return null;
    if (!res.ok) throw new JmapError(`driveNode ${res.status}`);
    return ((await res.json()) as { node: DriveNodeDto }).node;
  }

  /** The trashed nodes of a location. */
  async driveTrash(space: string | null): Promise<DriveNodeDto[]> {
    const q = new URLSearchParams();
    if (space) q.set("space", space);
    const res = await this.#fetch(`${API_BASE}/drive/trash?${q.toString()}`);
    if (!res.ok) throw new JmapError(`driveTrash ${res.status}`);
    return ((await res.json()) as { nodes: DriveNodeDto[] }).nodes;
  }

  /** Create a folder. */
  async driveCreateFolder(
    space: string | null,
    parent: string | null,
    name: string,
  ): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/drive/folders`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ space, parent, name }),
    });
    if (!res.ok) throw new JmapError(`driveCreateFolder ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Upload a file and register it in the drive at the given location. */
  async driveUpload(
    space: string | null,
    parent: string | null,
    file: File,
  ): Promise<string> {
    const { blobId, size } = await this.uploadFile(file);
    const res = await this.#fetch(`${API_BASE}/drive/files`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        space,
        parent,
        name: file.name,
        blobId,
        size,
        contentType: file.type.length > 0 ? file.type : null,
      }),
    });
    if (!res.ok) throw new JmapError(`driveUpload ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Upload a file and register it in Drive (personal root when no location
   * is given), returning the drive node id AND the underlying blob id — for
   * surfaces that reference the blob directly (site themes, site images)
   * while the Drive entry keeps the blob referenced and visible to the user. */
  async driveUploadBlob(
    space: string | null,
    parent: string | null,
    file: File,
  ): Promise<{ id: string; blobId: string; size: number }> {
    const { blobId, size } = await this.uploadFile(file);
    const res = await this.#fetch(`${API_BASE}/drive/files`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        space,
        parent,
        name: file.name,
        blobId,
        size,
        contentType: file.type.length > 0 ? file.type : null,
      }),
    });
    if (!res.ok) throw new JmapError(`driveUploadBlob ${res.status}`);
    return { id: ((await res.json()) as { id: string }).id, blobId, size };
  }

  /** Rename a node. */
  async driveRename(id: string, name: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name }),
      },
    );
    if (!res.ok) throw new JmapError(`driveRename ${res.status}`);
  }

  /** Move a node to another location/parent (re-scopes access). */
  async driveMove(
    id: string,
    space: string | null,
    parent: string | null,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/move`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ space, parent }),
      },
    );
    if (!res.ok) throw new JmapError(`driveMove ${res.status}`);
  }

  /** Copy a node to another location/parent. */
  async driveCopy(
    id: string,
    space: string | null,
    parent: string | null,
  ): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/copy`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ space, parent }),
      },
    );
    if (!res.ok) throw new JmapError(`driveCopy ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Move a node (and its subtree) to trash. */
  async driveTrashNode(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/trash`,
      {
        method: "POST",
      },
    );
    if (!res.ok) throw new JmapError(`driveTrashNode ${res.status}`);
  }

  /** Restore a node from trash. */
  async driveRestoreNode(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/restore`,
      {
        method: "POST",
      },
    );
    if (!res.ok) throw new JmapError(`driveRestoreNode ${res.status}`);
  }

  /** Permanently delete a node (and its subtree) from trash. */
  async drivePurge(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}`,
      {
        method: "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`drivePurge ${res.status}`);
  }

  /** A node's version history. */
  async driveVersions(id: string): Promise<DriveVersionDto[]> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/versions`,
    );
    if (!res.ok) throw new JmapError(`driveVersions ${res.status}`);
    return ((await res.json()) as { versions: DriveVersionDto[] }).versions;
  }

  /** Restore an old version as a new current one. */
  async driveRestoreVersion(id: string, versionNo: number): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/versions/${versionNo}/restore`,
      { method: "POST" },
    );
    if (!res.ok) throw new JmapError(`driveRestoreVersion ${res.status}`);
  }

  /** Download a node's current bytes (gated by read access). */
  async driveDownload(id: string): Promise<Blob> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/download`,
    );
    if (!res.ok) throw new JmapError(`driveDownload ${res.status}`);
    return res.blob();
  }

  /** Mint a WOPI token to open a file in the Collabora office editor. */
  async driveOfficeToken(id: string): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/office`,
    );
    if (!res.ok) throw new JmapError(`driveOfficeToken ${res.status}`);
    return ((await res.json()) as { token: string }).token;
  }

  /** Workspace search: files + tasks by name/title (ADR 0029). */
  async search(query: string): Promise<SearchHitDto[]> {
    const res = await this.#fetch(
      `${API_BASE}/search?q=${encodeURIComponent(query)}`,
    );
    if (!res.ok) throw new JmapError(`search ${res.status}`);
    return ((await res.json()) as { hits: SearchHitDto[] }).hits;
  }

  /** Ask a question across the workspace (ADR 0029): a cited answer over the
   * caller's access-scoped files, tasks, and mail. The matches come back even
   * when no model is configured (`answer` null, `reason` set), so the caller
   * can always show results. */
  async askWorkspace(query: string): Promise<AiAnswerDto> {
    const res = await this.#fetch(`${API_BASE}/ai/ask`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ q: query }),
    });
    if (!res.ok) throw new JmapError(`ask ${res.status}`);
    return (await res.json()) as AiAnswerDto;
  }

  /** The "Ask alo" agent (ADR 0034): like `askWorkspace`, but the model may
   * return a *proposed action* instead of an answer. Nothing is executed here —
   * a returned `action` must be approved by the user, which then calls
   * `executeAgentAction`. Matches come back even with no model configured. */
  async askAgent(query: string): Promise<AgentAnswerDto> {
    const res = await this.#fetch(`${API_BASE}/ai/agent`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        q: query,
        tz: Intl.DateTimeFormat().resolvedOptions().timeZone,
      }),
    });
    if (!res.ok) throw new JmapError(`agent ${res.status}`);
    return (await res.json()) as AgentAnswerDto;
  }

  /** Execute an agent action the user approved (ADR 0034). The only path that
   * acts; the server re-validates the tool + args and runs it within the caller's
   * tenant. Throws on any non-2xx so the caller can surface a failure. */
  async executeAgentAction(
    tool: string,
    args: Record<string, unknown>,
  ): Promise<AgentExecuteResultDto> {
    const res = await this.#fetch(`${API_BASE}/ai/agent/execute`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tool, args }),
    });
    if (!res.ok) throw new JmapError(`agent/execute ${res.status}`);
    return (await res.json()) as AgentExecuteResultDto;
  }

  /** Propose document text from an instruction + the current doc (ADR 0029 §3).
   * Returns proposed Markdown the caller shows for approval; throws if AI is
   * unavailable (the editor then surfaces a hint). Never applies anything
   * itself — approval happens in the editor. */
  async composeDoc(instruction: string, context: string): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/ai/compose`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ instruction, context }),
    });
    if (!res.ok) throw new JmapError(`compose ${res.status}`);
    return ((await res.json()) as { proposal: string }).proposal;
  }

  /** Append a new version to a node from an already-uploaded blob. */
  async driveAddVersion(
    id: string,
    blobId: string,
    size: number,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/nodes/${encodeURIComponent(id)}/versions`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ blobId, size }),
      },
    );
    if (!res.ok) throw new JmapError(`driveAddVersion ${res.status}`);
  }

  // ---- alo Doc (ADR 0031): a doc is a drive node (kind=doc) whose blob holds a
  //      BlockNote block tree (JSON). Content in a blob, versioned by Drive.

  /** Create an empty alo Doc and return its node id. */
  async driveCreateDoc(
    space: string | null,
    parent: string | null,
    name: string,
  ): Promise<string> {
    const { blobId, size } = await this.uploadFile(
      new File(["[]"], `${name}.json`, { type: "application/json" }),
    );
    const res = await this.#fetch(`${API_BASE}/drive/files`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        space,
        parent,
        name,
        blobId,
        size,
        contentType: "application/json",
        kind: "doc",
      }),
    });
    if (!res.ok) throw new JmapError(`driveCreateDoc ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Load an alo Doc's block content (the parsed BlockNote document). */
  async driveDocContent(id: string): Promise<unknown[]> {
    const text = await (await this.driveDownload(id)).text();
    if (text.trim() === "") return [];
    try {
      const parsed = JSON.parse(text);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  /** Save an alo Doc's block content as a new version. */
  async driveSaveDoc(id: string, content: unknown[]): Promise<void> {
    const json = JSON.stringify(content);
    const { blobId, size } = await this.uploadFile(
      new File([json], "doc.json", { type: "application/json" }),
    );
    await this.driveAddVersion(id, blobId, size);
  }

  /** Create an alo Sheet (a Univer workbook): a drive node whose blob holds the
   *  workbook snapshot JSON. Starts empty ("{}" → a blank default workbook). */
  async driveCreateSheet(
    space: string | null,
    parent: string | null,
    name: string,
  ): Promise<string> {
    const { blobId, size } = await this.uploadFile(
      new File(["{}"], `${name}.json`, { type: "application/json" }),
    );
    const res = await this.#fetch(`${API_BASE}/drive/files`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        space,
        parent,
        name,
        blobId,
        size,
        contentType: "application/json",
        kind: "sheet",
      }),
    });
    if (!res.ok) throw new JmapError(`driveCreateSheet ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Load an alo Sheet's workbook snapshot (the parsed Univer object). */
  async driveSheetContent(id: string): Promise<Record<string, unknown>> {
    const text = await (await this.driveDownload(id)).text();
    if (text.trim() === "") return {};
    try {
      const parsed = JSON.parse(text) as unknown;
      return parsed !== null && typeof parsed === "object"
        ? (parsed as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  }

  /** Save an alo Sheet's workbook snapshot as a new version. */
  async driveSaveSheet(
    id: string,
    workbook: Record<string, unknown>,
  ): Promise<void> {
    const { blobId, size } = await this.uploadFile(
      new File([JSON.stringify(workbook)], "sheet.json", {
        type: "application/json",
      }),
    );
    await this.driveAddVersion(id, blobId, size);
  }

  // ---- alo Base (ADR 0032): a relational base is a drive node (kind=base)
  //      whose tables/fields/records/views are relational data.

  /** Create a Base (its node + a default table). Returns the base node id. */
  async createBase(
    space: string | null,
    parent: string | null,
    name: string,
  ): Promise<string> {
    const res = await this.#fetch(`${API_BASE}/drive/base`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ space, parent, name }),
    });
    if (!res.ok) throw new JmapError(`createBase ${res.status}`);
    return ((await res.json()) as { nodeId: string }).nodeId;
  }

  /** The whole Base at a node. */
  async base(nodeId: string): Promise<BaseDto> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base/${encodeURIComponent(nodeId)}`,
    );
    if (!res.ok) throw new JmapError(`base ${res.status}`);
    return (await res.json()) as BaseDto;
  }

  /** Add a table to a Base. */
  async baseAddTable(nodeId: string, name: string): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base/${encodeURIComponent(nodeId)}/tables`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name }),
      },
    );
    if (!res.ok) throw new JmapError(`baseAddTable ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Add a typed field (column) to a table. */
  async baseAddField(
    tableId: string,
    name: string,
    type: BaseFieldType,
    options?: Record<string, unknown>,
  ): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base-tables/${encodeURIComponent(tableId)}/fields`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name, type, options: options ?? {} }),
      },
    );
    if (!res.ok) throw new JmapError(`baseAddField ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Add a record (row) to a table. */
  async baseAddRecord(
    tableId: string,
    cells?: Record<string, unknown>,
  ): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base-tables/${encodeURIComponent(tableId)}/records`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ cells: cells ?? {} }),
      },
    );
    if (!res.ok) throw new JmapError(`baseAddRecord ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Replace a record's cells. */
  async baseUpdateRecord(
    recordId: string,
    cells: Record<string, unknown>,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base-records/${encodeURIComponent(recordId)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ cells }),
      },
    );
    if (!res.ok) throw new JmapError(`baseUpdateRecord ${res.status}`);
  }

  /** Delete a record. */
  async baseDeleteRecord(recordId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base-records/${encodeURIComponent(recordId)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`baseDeleteRecord ${res.status}`);
  }

  /** Add a view over a table. */
  async baseAddView(
    tableId: string,
    kind: BaseViewKind,
    name: string,
    config?: Record<string, unknown>,
  ): Promise<string> {
    const res = await this.#fetch(
      `${API_BASE}/drive/base-tables/${encodeURIComponent(tableId)}/views`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind, name, config: config ?? {} }),
      },
    );
    if (!res.ok) throw new JmapError(`baseAddView ${res.status}`);
    return ((await res.json()) as { id: string }).id;
  }

  /** Upload a large file as an expiring public share link (alo Transfer),
   * instead of an inline attachment. `days` is the link lifetime chosen by the
   * sender. The file is streamed, so there is no size limit. */
  async uploadShare(
    file: File,
    days: number,
  ): Promise<{
    url: string;
    filename: string;
    size: number;
    expiresAt: number;
  }> {
    const url =
      `${API_BASE}/share/upload` +
      `?name=${encodeURIComponent(file.name)}&days=${encodeURIComponent(String(days))}`;
    const res = await this.#fetch(url, {
      method: "POST",
      headers: {
        "content-type":
          file.type.length > 0 ? file.type : "application/octet-stream",
      },
      body: file,
    });
    if (!res.ok) throw new JmapError(`share ${res.status}`);
    return (await res.json()) as {
      url: string;
      filename: string;
      size: number;
      expiresAt: number;
    };
  }

  /** Create a draft message; returns the new email id. */
  async createDraft(params: {
    mailboxId: string;
    from: EmailAddress;
    to: EmailAddress[];
    cc?: EmailAddress[];
    bcc?: EmailAddress[];
    subject: string;
    bodyText: string;
    bodyHtml?: string;
    inReplyTo?: string[];
    references?: string[];
    attachments?: { blobId: string; type: string; name: string }[];
  }): Promise<string> {
    const accountId = await this.accountId();
    const bodyValues: Record<string, { value: string }> = {
      text: { value: params.bodyText },
    };
    const email: Record<string, unknown> = {
      mailboxIds: { [params.mailboxId]: true },
      keywords: { $draft: true },
      from: [params.from],
      to: params.to,
      subject: params.subject,
      bodyValues,
      textBody: [{ partId: "text", type: "text/plain" }],
    };
    if (params.bodyHtml !== undefined && params.bodyHtml.length > 0) {
      bodyValues.html = { value: params.bodyHtml };
      email.htmlBody = [{ partId: "html", type: "text/html" }];
    }
    if (params.attachments !== undefined && params.attachments.length > 0) {
      email.attachments = params.attachments.map((a) => ({
        blobId: a.blobId,
        type: a.type,
        name: a.name,
        disposition: "attachment",
      }));
    }
    if (params.cc !== undefined && params.cc.length > 0) email.cc = params.cc;
    // Bcc goes into the draft (so the sender's Sent copy records it); the server
    // strips the Bcc header from the bytes it transmits to recipients.
    if (params.bcc !== undefined && params.bcc.length > 0)
      email.bcc = params.bcc;
    if (params.inReplyTo !== undefined && params.inReplyTo.length > 0)
      email.inReplyTo = params.inReplyTo;
    if (params.references !== undefined && params.references.length > 0)
      email.references = params.references;
    const res = await this.#request([
      ["Email/set", { accountId, create: { draft: email } }, "c"],
    ]);
    const result = this.#result(res, "c");
    const created = (
      result.created as Record<string, { id: string }> | undefined
    )?.draft;
    if (created === undefined) {
      throw new JmapError("the draft could not be created");
    }
    return created.id;
  }

  /** Submit a draft for delivery; the server sends it and files it to Sent. */
  async submitEmail(
    emailId: string,
    mailFrom: string,
    rcptTo: string[],
  ): Promise<void> {
    const accountId = await this.accountId();
    const res = await this.#request([
      [
        "EmailSubmission/set",
        {
          accountId,
          create: {
            sub: {
              emailId,
              envelope: {
                mailFrom: { email: mailFrom },
                rcptTo: rcptTo.map((email) => ({ email })),
              },
            },
          },
        },
        "s",
      ],
    ]);
    const result = this.#result(res, "s");
    const created = (result.created as Record<string, unknown> | undefined)
      ?.sub;
    if (created === undefined) {
      const notCreated = (
        result.notCreated as
          Record<string, { description?: string; type?: string }> | undefined
      )?.sub;
      throw new JmapError(
        notCreated?.description ??
          notCreated?.type ??
          "the message could not be sent",
      );
    }
  }

  /** Schedule a draft to be sent at `sendAt` (Unix seconds) instead of now. The
   * draft moves to the Scheduled mailbox; a server sweeper submits it when due.
   * Same send-from validation as an immediate submission (rejects up front). */
  async scheduleSend(
    emailId: string,
    mailFrom: string,
    rcptTo: string[],
    sendAt: number,
  ): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/send-later`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        emailId,
        envelope: {
          mailFrom: { email: mailFrom },
          rcptTo: rcptTo.map((email) => ({ email })),
        },
        sendAt,
      }),
    });
    if (!res.ok) throw new JmapError(`send-later ${res.status}`);
  }

  /** Cancel a scheduled send: the draft returns to Drafts, editable again. */
  async cancelScheduledSend(emailId: string): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/send-later/cancel`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ emailId }),
    });
    if (!res.ok) throw new JmapError(`send-later/cancel ${res.status}`);
  }

  /** Recent correspondents for compose recipient autocomplete, most frequent +
   * recent first. Fetched once per compose session; the client filters locally. */
  async recentContacts(): Promise<EmailAddress[]> {
    const res = await this.#fetch(`${API_BASE}/contacts`, { method: "GET" });
    if (!res.ok) throw new JmapError(`contacts ${res.status}`);
    const json = (await res.json()) as { contacts: EmailAddress[] };
    return json.contacts;
  }

  /** Calendar events overlapping `[fromIso, toIso)` (RFC 3339 UTC). */
  async calendarEvents(
    fromIso: string,
    toIso: string,
  ): Promise<CalendarEvent[]> {
    const url = `${API_BASE}/calendar/events?from=${encodeURIComponent(
      fromIso,
    )}&to=${encodeURIComponent(toIso)}`;
    const res = await this.#fetch(url, { method: "GET" });
    if (!res.ok) throw new JmapError(`calendar ${res.status}`);
    const json = (await res.json()) as { events: CalendarEvent[] };
    return json.events;
  }

  /** The stored (unexpanded) event by id — used to edit a recurring series
   *  template, since range listings return occurrences sharing the master id. */
  async getEvent(id: string): Promise<CalendarEvent> {
    const res = await this.#fetch(
      `${API_BASE}/calendar/events/${encodeURIComponent(id)}`,
      { method: "GET" },
    );
    if (!res.ok) throw new JmapError(`calendar get ${res.status}`);
    return (await res.json()) as CalendarEvent;
  }

  /** Create a calendar event; returns the stored event (with its id). */
  async createEvent(input: EventInput): Promise<CalendarEvent> {
    const res = await this.#fetch(`${API_BASE}/calendar/events`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    });
    if (!res.ok) throw new JmapError(`calendar create ${res.status}`);
    return (await res.json()) as CalendarEvent;
  }

  /** Replace a calendar event's fields (the whole event/series). */
  async updateEvent(id: string, input: EventInput): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/calendar/events/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(input),
      },
    );
    if (!res.ok) throw new JmapError(`calendar update ${res.status}`);
  }

  /** Override a single occurrence of a recurring series in place: `occurrence`
   * is that instance's ORIGINAL slot (RFC 3339); `input` carries its new fields
   * (possibly a new time). The rest of the series is untouched. */
  async overrideOccurrence(
    id: string,
    occurrence: string,
    input: EventInput,
  ): Promise<void> {
    const url = `${API_BASE}/calendar/events/${encodeURIComponent(id)}?occurrence=${encodeURIComponent(
      occurrence,
    )}`;
    const res = await this.#fetch(url, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    });
    if (!res.ok) throw new JmapError(`calendar override ${res.status}`);
  }

  /** Delete a calendar event. */
  /** Delete an event. With `occurrence` (a recurring instance's RFC 3339 start),
   * only that instance is skipped (the series stays); without it, the whole
   * event/series is deleted. */
  async deleteEvent(id: string, occurrence?: string): Promise<void> {
    const base = `${API_BASE}/calendar/events/${encodeURIComponent(id)}`;
    const url =
      occurrence !== undefined
        ? `${base}?occurrence=${encodeURIComponent(occurrence)}`
        : base;
    const res = await this.#fetch(url, { method: "DELETE" });
    if (!res.ok) throw new JmapError(`calendar delete ${res.status}`);
  }

  /** The user's calendars (personal + any they created). */
  async calendars(): Promise<Calendar[]> {
    const res = await this.#fetch(`${API_BASE}/calendar/calendars`);
    if (!res.ok) throw new JmapError(`calendars ${res.status}`);
    return ((await res.json()) as { calendars: Calendar[] }).calendars;
  }

  /** Create a calendar; returns it. */
  async createCalendar(name: string, color?: string): Promise<Calendar> {
    const res = await this.#fetch(`${API_BASE}/calendar/calendars`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(color !== undefined ? { name, color } : { name }),
    });
    if (!res.ok) throw new JmapError(`createCalendar ${res.status}`);
    return (await res.json()) as Calendar;
  }

  /** Delete a calendar and its events (the personal one is protected → 409). */
  async deleteCalendar(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/calendar/calendars/${encodeURIComponent(id)}`,
      {
        method: "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`deleteCalendar ${res.status}`);
  }

  /** Who a calendar the caller owns is shared with. */
  async calendarGrants(id: string): Promise<CalendarGrant[]> {
    const res = await this.#fetch(
      `${API_BASE}/calendar/calendars/${encodeURIComponent(id)}/grants`,
    );
    if (!res.ok) throw new JmapError(`calendarGrants ${res.status}`);
    return ((await res.json()) as { grants: CalendarGrant[] }).grants;
  }

  /** Share a calendar with a person (by email) or a group at viewer/editor. */
  async shareCalendar(
    id: string,
    kind: "user" | "group",
    subject: string,
    role: "viewer" | "editor",
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/calendar/calendars/${encodeURIComponent(id)}/grants`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind, subject, role }),
      },
    );
    if (!res.ok) throw new JmapError(`shareCalendar ${res.status}`);
  }

  /** Remove a share from a calendar the caller owns. */
  async unshareCalendar(
    id: string,
    kind: "user" | "group",
    subject: string,
  ): Promise<void> {
    const q = `kind=${encodeURIComponent(kind)}&subject=${encodeURIComponent(subject)}`;
    const res = await this.#fetch(
      `${API_BASE}/calendar/calendars/${encodeURIComponent(id)}/grants?${q}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`unshareCalendar ${res.status}`);
  }

  /** The tenant's groups, for offering team sharing in the share dialog. */
  async shareableGroups(): Promise<ShareableGroup[]> {
    const res = await this.#fetch(`${API_BASE}/calendar/groups`);
    if (!res.ok) throw new JmapError(`shareableGroups ${res.status}`);
    return ((await res.json()) as { groups: ShareableGroup[] }).groups;
  }

  // --- Tasks (ADR 0021–0023) -------------------------------------------------

  /** The caller's visible task projects (personal + team). */
  async taskProjects(): Promise<TaskProject[]> {
    const res = await this.#fetch(`${API_BASE}/tasks/projects`);
    if (!res.ok) throw new JmapError(`taskProjects ${res.status}`);
    return ((await res.json()) as { projects: TaskProject[] }).projects;
  }

  /** Create a team project; returns it. */
  async createTaskProject(name: string, color?: string): Promise<TaskProject> {
    const res = await this.#fetch(`${API_BASE}/tasks/projects`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(color !== undefined ? { name, color } : { name }),
    });
    if (!res.ok) throw new JmapError(`createTaskProject ${res.status}`);
    return (await res.json()) as TaskProject;
  }

  /** The active tasks on a project (the client groups into board/list). */
  async tasks(projectId: string): Promise<Task[]> {
    const res = await this.#fetch(
      `${API_BASE}/tasks?project=${encodeURIComponent(projectId)}`,
    );
    if (!res.ok) throw new JmapError(`tasks ${res.status}`);
    return ((await res.json()) as { tasks: Task[] }).tasks;
  }

  /** Create a task; returns the stored task. */
  async createTask(input: TaskInput): Promise<Task> {
    const res = await this.#fetch(`${API_BASE}/tasks`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    });
    if (!res.ok) throw new JmapError(`createTask ${res.status}`);
    return (await res.json()) as Task;
  }

  /** A task with its subtasks, comments, and activity (the detail panel). */
  async taskDetail(id: string): Promise<TaskDetailData> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}`,
    );
    if (!res.ok) throw new JmapError(`taskDetail ${res.status}`);
    return (await res.json()) as TaskDetailData;
  }

  /** Attach an already-uploaded blob (see uploadFile) to a task. */
  async addTaskAttachment(
    taskId: string,
    blobId: string,
    filename: string,
    size: number,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/attachments`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ blobId, filename, size }),
      },
    );
    if (!res.ok) throw new JmapError(`addTaskAttachment ${res.status}`);
  }

  /** The files on a task. */
  async taskAttachments(taskId: string): Promise<TaskAttachmentDto[]> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/attachments`,
    );
    if (!res.ok) throw new JmapError(`taskAttachments ${res.status}`);
    return ((await res.json()) as { attachments: TaskAttachmentDto[] })
      .attachments;
  }

  /** Remove a file from a task (the blob itself is left in the store). */
  async deleteTaskAttachment(
    taskId: string,
    attachmentId: string,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/attachments/${encodeURIComponent(attachmentId)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`deleteTaskAttachment ${res.status}`);
  }

  /** Every file across a project's tasks (the Files view). */
  async projectFiles(projectId: string): Promise<ProjectFileDto[]> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/files?project=${encodeURIComponent(projectId)}`,
    );
    if (!res.ok) throw new JmapError(`projectFiles ${res.status}`);
    return ((await res.json()) as { files: ProjectFileDto[] }).files;
  }

  /** Every label in the tenant (reusable across tasks). */
  async taskLabels(): Promise<TaskLabelDto[]> {
    const res = await this.#fetch(`${API_BASE}/tasks/labels`);
    if (!res.ok) throw new JmapError(`taskLabels ${res.status}`);
    return ((await res.json()) as { labels: TaskLabelDto[] }).labels;
  }

  /** Create a tenant label. */
  async createTaskLabel(name: string, color?: string): Promise<TaskLabelDto> {
    const res = await this.#fetch(`${API_BASE}/tasks/labels`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(color !== undefined ? { name, color } : { name }),
    });
    if (!res.ok) throw new JmapError(`createTaskLabel ${res.status}`);
    return (await res.json()) as TaskLabelDto;
  }

  /** Delete a label from the tenant (and every task). */
  async deleteTaskLabel(labelId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/labels/${encodeURIComponent(labelId)}`,
      {
        method: "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`deleteTaskLabel ${res.status}`);
  }

  /** Attach a label to a task. */
  async addTaskLabel(taskId: string, labelId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/labels`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ labelId }),
      },
    );
    if (!res.ok) throw new JmapError(`addTaskLabel ${res.status}`);
  }

  /** Remove a label from a task. */
  async removeTaskLabel(taskId: string, labelId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/labels/${encodeURIComponent(labelId)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`removeTaskLabel ${res.status}`);
  }

  /** Follow / stop following a task (the current user). */
  async followTask(taskId: string, follow: boolean): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/followers`,
      {
        method: follow ? "POST" : "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`followTask ${res.status}`);
  }

  /** Record that `taskId` is blocked by `dependsOn` (both must be visible). */
  async addTaskDependency(taskId: string, dependsOn: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/dependencies`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ dependsOn }),
      },
    );
    if (!res.ok) throw new JmapError(`addTaskDependency ${res.status}`);
  }

  /** Drop a "blocked by" edge from `taskId` to `dependsOn`. */
  async removeTaskDependency(taskId: string, dependsOn: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/dependencies/${encodeURIComponent(dependsOn)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`removeTaskDependency ${res.status}`);
  }

  /** Every dependency edge among a project's visible tasks (Timeline arrows). */
  async projectDependencies(projectId: string): Promise<TaskDepEdgeDto[]> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/dependencies?project=${encodeURIComponent(projectId)}`,
    );
    if (!res.ok) throw new JmapError(`projectDependencies ${res.status}`);
    return ((await res.json()) as { edges: TaskDepEdgeDto[] }).edges;
  }

  /** Download a task attachment's bytes (gated by task visibility). */
  async downloadTaskAttachment(
    taskId: string,
    attachmentId: string,
  ): Promise<Blob> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/attachments/${encodeURIComponent(attachmentId)}/download`,
    );
    if (!res.ok) throw new JmapError(`downloadTaskAttachment ${res.status}`);
    return res.blob();
  }

  /** Edit a task's fields (not status/position — that is a move). */
  async updateTask(id: string, input: TaskInput): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(input),
      },
    );
    if (!res.ok) throw new JmapError(`updateTask ${res.status}`);
  }

  /** Move a task to a status/position — board drag or list status-change. */
  async moveTask(id: string, status: string, position: number): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}/move`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ status, position }),
      },
    );
    if (!res.ok) throw new JmapError(`moveTask ${res.status}`);
  }

  async deleteTask(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}`,
      {
        method: "DELETE",
      },
    );
    if (!res.ok) throw new JmapError(`deleteTask ${res.status}`);
  }

  /** "What's on my plate": due/overdue assigned tasks. */
  async myPlate(): Promise<Task[]> {
    const res = await this.#fetch(`${API_BASE}/tasks/today`);
    if (!res.ok) throw new JmapError(`myPlate ${res.status}`);
    return ((await res.json()) as { tasks: Task[] }).tasks;
  }

  /** Propose tasks (AI-detected) — they land in the Suggestions inbox as
   * `proposed`, never on the board (ADR 0023). */
  async proposeTasks(
    tasks: TaskInput[],
    projectId?: string,
  ): Promise<{ created: number }> {
    const res = await this.#fetch(`${API_BASE}/tasks/propose`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(
        projectId !== undefined ? { projectId, tasks } : { tasks },
      ),
    });
    if (!res.ok) throw new JmapError(`proposeTasks ${res.status}`);
    return (await res.json()) as { created: number };
  }

  /** Ask the AI to extract candidate tasks from text (needs a tenant AI
   * provider; 503 when AI is off). The caller feeds these to proposeTasks. */
  async extractTasks(
    text: string,
  ): Promise<{ title: string; dueAt?: string }[]> {
    const res = await this.#fetch(`${API_BASE}/ai/extract-tasks`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new JmapError(`extractTasks ${res.status}`);
    return (
      (await res.json()) as { tasks: { title: string; dueAt?: string }[] }
    ).tasks;
  }

  /** Pending AI proposals (awaiting accept/reject). */
  async taskProposals(): Promise<Task[]> {
    const res = await this.#fetch(`${API_BASE}/tasks/proposals`);
    if (!res.ok) throw new JmapError(`taskProposals ${res.status}`);
    return ((await res.json()) as { tasks: Task[] }).tasks;
  }

  /** Approve a proposal (optionally with edits), making it a real task. */
  async acceptTask(id: string, edits?: TaskInput): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}/accept`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: edits ? JSON.stringify(edits) : "",
      },
    );
    if (!res.ok) throw new JmapError(`acceptTask ${res.status}`);
  }

  /** Drop a proposal. */
  async rejectTask(id: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(id)}/reject`,
      {
        method: "POST",
      },
    );
    if (!res.ok) throw new JmapError(`rejectTask ${res.status}`);
  }

  async addSubtask(taskId: string, title: string): Promise<{ id: string }> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/subtasks`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ title }),
      },
    );
    if (!res.ok) throw new JmapError(`addSubtask ${res.status}`);
    return (await res.json()) as { id: string };
  }

  async setSubtask(
    taskId: string,
    subtaskId: string,
    done: boolean,
  ): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/subtasks/${encodeURIComponent(subtaskId)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ done }),
      },
    );
    if (!res.ok) throw new JmapError(`setSubtask ${res.status}`);
  }

  async deleteSubtask(taskId: string, subtaskId: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/subtasks/${encodeURIComponent(subtaskId)}`,
      { method: "DELETE" },
    );
    if (!res.ok) throw new JmapError(`deleteSubtask ${res.status}`);
  }

  async addTaskComment(taskId: string, body: string): Promise<void> {
    const res = await this.#fetch(
      `${API_BASE}/tasks/${encodeURIComponent(taskId)}/comments`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ body }),
      },
    );
    if (!res.ok) throw new JmapError(`addTaskComment ${res.status}`);
  }

  /** Busy intervals for each person (by email, in the tenant) over `[from,to)`. */
  async freeBusy(
    emails: string[],
    fromIso: string,
    toIso: string,
  ): Promise<FreeBusyPerson[]> {
    const res = await this.#fetch(`${API_BASE}/calendar/freebusy`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ emails, from: fromIso, to: toIso }),
    });
    if (!res.ok) throw new JmapError(`freebusy ${res.status}`);
    return ((await res.json()) as { freebusy: FreeBusyPerson[] }).freebusy;
  }

  /** The user's server-side mail filter rules. */
  async filters(): Promise<MailFilterRule[]> {
    const res = await this.#fetch(`${API_BASE}/filters`, { method: "GET" });
    if (!res.ok) throw new JmapError(`filters ${res.status}`);
    const json = (await res.json()) as { rules: MailFilterRule[] };
    return json.rules;
  }

  /** Replace the user's mail filter rules; the server recompiles the delivery
   * script. Returns the stored rules. */
  async saveFilters(rules: MailFilterRule[]): Promise<MailFilterRule[]> {
    const res = await this.#fetch(`${API_BASE}/filters`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ rules }),
    });
    if (!res.ok) throw new JmapError(`filters ${res.status}`);
    const json = (await res.json()) as { rules: MailFilterRule[] };
    return json.rules;
  }

  /** Block a sender: append a rule filing their mail into Junk (idempotent). */
  async blockSender(email: string): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/filters/block`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email }),
    });
    if (!res.ok) throw new JmapError(`block ${res.status}`);
  }

  /** Perform the RFC 8058 one-click unsubscribe for a message (server-side POST
   * to the sender's List-Unsubscribe endpoint, SSRF-guarded). */
  async unsubscribe(emailId: string): Promise<void> {
    const res = await this.#fetch(`${API_BASE}/jmap/unsubscribe`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ emailId }),
    });
    if (!res.ok) throw new JmapError(`unsubscribe ${res.status}`);
  }

  /** Respond to a received calendar invitation: adds the event to the user's
   * calendar (unless declining) and emails a reply to the organizer. `blobId`
   * is the invitation message's blobId. */
  async rsvp(
    blobId: string,
    response: RsvpResponse,
  ): Promise<{ added: boolean; replied: boolean }> {
    const res = await this.#fetch(`${API_BASE}/calendar/rsvp`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ blobId, response }),
    });
    if (!res.ok) throw new JmapError(`rsvp ${res.status}`);
    return (await res.json()) as { added: boolean; replied: boolean };
  }

  /** Apply an organizer's cancellation: removes the matching event from the
   * user's calendar. `blobId` is the cancellation message's blobId. `removed`
   * is false when the event wasn't on the calendar (already gone). */
  async cancelInvitation(blobId: string): Promise<{ removed: boolean }> {
    const res = await this.#fetch(`${API_BASE}/calendar/cancel`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ blobId }),
    });
    if (!res.ok) throw new JmapError(`cancel ${res.status}`);
    return (await res.json()) as { removed: boolean };
  }

  /** Record a guest's reply (from the REPLY message's blob) on the organizer's
   * event. `applied:false` when the event isn't the caller's to update. */
  async applyReply(
    blobId: string,
  ): Promise<{ applied: boolean; email: string; status: string }> {
    const res = await this.#fetch(`${API_BASE}/calendar/apply-reply`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ blobId }),
    });
    if (!res.ok) throw new JmapError(`applyReply ${res.status}`);
    return (await res.json()) as {
      applied: boolean;
      email: string;
      status: string;
    };
  }
}
