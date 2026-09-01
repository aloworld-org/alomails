import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { strings } from "../../i18n";
import type { EmailFull } from "../../jmap";
import { ReadingPane } from "./ReadingPane";

vi.mock("../../auth", () => ({
  useAuth: () => ({
    identity: { sub: "user-1", email: "billing@alo.example", name: "Alo Billing" },
  }),
}));

vi.mock("../../jmap", async (original) => {
  const actual = await original<typeof import("../../jmap")>();
  return {
    ...actual,
    useJmapClient: () => ({ aiEnabled: async () => false }),
  };
});

vi.mock("../../agents", () => ({ RecordAgentPanel: () => null }));
vi.mock("./ThreadMessage", () => ({ ThreadMessage: () => <p>Prepared message</p> }));

const prepared = {
  id: "mail-quote-1",
  threadId: "thread-1",
  blobId: "blob-1",
  mailboxIds: { drafts: true },
  keywords: { $draft: true },
  from: [{ name: "Alo Billing", email: "billing@alo.example" }],
  to: [{ name: "Customer", email: "accounts@customer.example" }],
  cc: [],
  bcc: [],
  subject: "Quotation QUO-2026-00004",
  preview: "Please find your quotation attached.",
  receivedAt: "2026-08-30T10:00:00Z",
  messageId: [],
  inReplyTo: [],
  references: [],
  hasAttachment: true,
  size: 32000,
  textBody: [],
  htmlBody: [],
  bodyValues: {},
  attachments: [],
} as unknown as EmailFull;

function props(onEditDraft: () => void, onSendDraft: () => void) {
  const noop = vi.fn();
  return {
    thread: { status: "ready" as const, data: [prepared], error: null, reload: noop },
    mailboxes: [],
    currentMailboxId: "drafts",
    flagOverrides: new Map<string, boolean>(),
    onReply: noop,
    onReplyAll: noop,
    onForward: noop,
    onEditDraft,
    onSendDraft,
    onToggleFlag: noop,
    onArchive: noop,
    onDelete: noop,
    onMove: noop,
    onMarkUnread: noop,
    onSnooze: noop,
    onReportSpam: noop,
    onForwardAttachment: noop,
    onSmartReply: noop,
    onCancelSend: noop,
    onBlockSender: noop,
    isScheduled: false,
    isJunk: false,
    categories: [],
    onToggleCategory: noop,
    onUnsubscribe: noop,
    canSnooze: false,
    onSetFlagDue: noop,
    onCreateTask: noop,
    onCreateOpportunity: noop,
    onSuggestTasks: noop,
    onCompose: noop,
  };
}

describe("ReadingPane", () => {
  test("a prepared customer draft opens in the editor and retains quick send", () => {
    const edit = vi.fn();
    const send = vi.fn();
    render(<ReadingPane {...props(edit, send)} />);

    fireEvent.click(screen.getByRole("button", { name: strings.composeEdit }));
    fireEvent.click(screen.getByRole("button", { name: strings.composeSend }));
    expect(edit).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: strings.reply })).toBeNull();
  });

  test("offers a reviewed Sales opportunity handoff from the conversation", () => {
    const createOpportunity = vi.fn();
    const preparedProps = props(vi.fn(), vi.fn());
    render(<ReadingPane {...preparedProps} onCreateOpportunity={createOpportunity} />);

    const actionGroups = screen.getAllByRole("group", {
      name: strings.conversationActions,
    });
    const actions = within(actionGroups[actionGroups.length - 1] as HTMLElement);
    const menus = actions.getAllByRole("button", { name: strings.moreActions });
    fireEvent.click(menus[menus.length - 1] as HTMLElement);
    fireEvent.click(screen.getByRole("menuitem", { name: strings.crmCreateOpportunity }));
    expect(createOpportunity).toHaveBeenCalledTimes(1);
  });
});
